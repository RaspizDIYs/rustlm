use super::network::{hash, Report};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::Mutex,
};

const BEGIN: &str = "# BEGIN RUSTLM RIOT";
const END: &str = "# END RUSTLM RIOT";
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Serialize, Deserialize)]
pub struct Pin {
    pub domain: String,
    pub ip: IpAddr,
}
#[derive(Serialize, Deserialize)]
pub struct HostsPlan {
    pub report_id: String,
    pub before_hash: String,
    pub after: String,
    pub pins: Vec<Pin>,
}
#[derive(Serialize, Deserialize)]
struct HostsBackup {
    before: Vec<u8>,
    after_hash: String,
}

fn hosts_path() -> Result<PathBuf, String> {
    if !cfg!(windows) {
        return Err("Настройка hosts поддерживается только в Windows".into());
    }
    Ok(
        PathBuf::from(std::env::var_os("SystemRoot").ok_or("SystemRoot не определён")?)
            .join("System32/drivers/etc/hosts"),
    )
}
pub fn save_snapshot(destination: &Path) -> Result<(), String> {
    fs::write(
        destination,
        fs::read(hosts_path()?).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
fn write_hosts(target: &Path, expected_hash: &str, bytes: &[u8]) -> Result<(), String> {
    if hash(&fs::read(target).map_err(|e| e.to_string())?) != expected_hash {
        return Err("hosts изменился перед записью".into());
    }
    match super::network::atomic_write(target, bytes) {
        Ok(()) => Ok(()),
        Err(original) => {
            use base64::Engine;
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            let temporary =
                target.with_file_name(format!(".rustlm-hosts-{}.tmp", rand::random::<u64>()));
            let script = format!(
                r#"$ErrorActionPreference='Stop'; $target={target}; $bytes=[IO.File]::ReadAllBytes($target); $hash=([BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($bytes))).Replace('-','').ToLowerInvariant(); if($hash -ne {expected}){{throw 'hosts изменился перед записью'}}; $temp={temp}; try{{[IO.File]::WriteAllBytes($temp,[Convert]::FromBase64String({data}));[IO.File]::Replace($temp,$target,$null); & ipconfig.exe /flushdns | Out-Null}}finally{{if(Test-Path -LiteralPath $temp){{Remove-Item -LiteralPath $temp}}}}"#,
                target = super::repair::literal(&target.to_string_lossy()),
                expected = super::repair::literal(expected_hash),
                temp = super::repair::literal(&temporary.to_string_lossy()),
                data = super::repair::literal(&data)
            );
            super::repair::elevated(&script)
                .map(|_| ())
                .map_err(|e| format!("{original}. Windows: {e}"))
        }
    }
}
fn owned(domain: &str) -> bool {
    [
        "riotgames.com",
        "riotcdn.net",
        "leagueoflegends.com",
        "pvp.net",
        "rgpub.io",
    ]
    .iter()
    .any(|root| domain == *root || domain.ends_with(&format!(".{root}")))
}
pub fn candidates(report: &Report) -> Vec<Pin> {
    let mut candidates = BTreeSet::new();
    if report.context.connection == "vpn" || report.cancelled {
        return vec![];
    }
    for probe in &report.probes {
        if owned(&probe.domain)
            && probe.tls
            && probe.ip.is_ipv4()
            && super::network::public_ip(probe.ip)
            && probe.port == 443
            && probe.http.is_some_and(|s| (200..300).contains(&s))
            && probe.classification == "http_success"
        {
            candidates.insert((probe.domain.clone(), probe.ip));
        }
    }
    candidates
        .into_iter()
        .map(|(domain, ip)| Pin { domain, ip })
        .collect()
}
pub fn plan(report: &Report, current: &[u8], pins: Vec<Pin>) -> Result<HostsPlan, String> {
    if pins.is_empty() || pins.len() > 100 {
        return Err("Выберите от 1 до 100 проверенных адресов".into());
    }
    let text = std::str::from_utf8(current)
        .map_err(|_| "hosts не в UTF-8; автоматическая перекодировка отключена")?;
    if text.contains(BEGIN) || text.contains(END) {
        return Err(
            "Блок RustLM уже существует. Сначала откатите предыдущую настройку hosts.".into(),
        );
    }
    let eligible: BTreeSet<_> = candidates(report)
        .into_iter()
        .map(|p| (p.domain, p.ip))
        .collect();
    let mut entries = BTreeMap::new();
    for pin in &pins {
        if !eligible.contains(&(pin.domain.clone(), pin.ip)) {
            return Err(format!(
                "{}: нужен успешный HTTPS-ответ без VPN на выбранном IP",
                pin.domain
            ));
        }
        if entries.insert(pin.domain.clone(), pin.ip).is_some() {
            return Err("Для домена можно выбрать только один IP".into());
        }
    }
    for line in text.lines() {
        let names: Vec<_> = line
            .split('#')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .skip(1)
            .collect();
        if names
            .iter()
            .any(|name| entries.contains_key(&name.to_ascii_lowercase()))
        {
            return Err(
                "В hosts уже есть запись для выбранного домена. Существующая запись сохранена."
                    .into(),
            );
        }
    }
    let mut after = format!(
        "{text}\r\n{BEGIN}\r\n# DNS snapshot; undo if routing or CDN addresses change.\r\n"
    );
    for (domain, ip) in entries {
        after.push_str(&format!("{ip} {domain}\r\n"));
    }
    after.push_str(&format!("{END}\r\n"));
    Ok(HostsPlan {
        report_id: report.id.clone(),
        before_hash: hash(current),
        after,
        pins,
    })
}
pub fn preview(report: &Report, pins: Vec<Pin>) -> Result<HostsPlan, String> {
    plan(
        report,
        &fs::read(hosts_path()?).map_err(|e| e.to_string())?,
        pins,
    )
}
pub fn apply(report: &Report, supplied: HostsPlan, backups: &Path) -> Result<String, String> {
    let _guard = LOCK.lock().map_err(|e| e.to_string())?;
    let target = hosts_path()?;
    let before = fs::read(&target).map_err(|e| e.to_string())?;
    let expected = plan(report, &before, supplied.pins)?;
    if expected.before_hash != supplied.before_hash
        || expected.after != supplied.after
        || report.id != supplied.report_id
    {
        return Err("План или hosts изменились. Обновите предварительный просмотр.".into());
    }
    fs::create_dir_all(backups).map_err(|e| e.to_string())?;
    let id = chrono::Utc::now().format("%Y%m%dT%H%M%S%3f").to_string();
    let backup = HostsBackup {
        before,
        after_hash: hash(expected.after.as_bytes()),
    };
    let backup_path = backups.join(format!("{id}.json"));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(backup_path)
        .map_err(|e| e.to_string())?;
    use std::io::Write;
    file.write_all(&serde_json::to_vec(&backup).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    write_hosts(&target, &expected.before_hash, expected.after.as_bytes())
        .map_err(|e| format!("Не удалось изменить hosts: {e}. Резервная копия: {id}"))?;
    flush_dns();
    Ok(id)
}
pub fn restore(id: &str, backups: &Path) -> Result<(), String> {
    let _guard = LOCK.lock().map_err(|e| e.to_string())?;
    if id.len() != 18 || !id.bytes().all(|c| c.is_ascii_digit() || c == b'T') {
        return Err("Некорректный ID".into());
    }
    let backup: HostsBackup = serde_json::from_slice(
        &fs::read(backups.join(format!("{id}.json"))).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let target = hosts_path()?;
    let current = fs::read(&target).map_err(|e| e.to_string())?;
    if hash(&current) != backup.after_hash {
        return Err(
            "hosts изменён после настройки. Автооткат остановлен для сохранения ваших изменений."
                .into(),
        );
    }
    write_hosts(&target, &backup.after_hash, &backup.before)?;
    flush_dns();
    Ok(())
}
fn flush_dns() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("ipconfig.exe")
            .arg("/flushdns")
            .creation_flags(0x08000000)
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report() -> Report {
        serde_json::from_value(serde_json::json!({"id":"test","created_at":"now","context":{"connection":"direct","provider":"home","region":"EUW","strategy":null,"strategy_hash":null},"dns":[],"probes":[{"domain":"clientconfig.rpg.riotgames.com","ip":"104.18.156.37","port":443,"tcp":true,"tls":true,"http":200,"classification":"http_success","server":null,"ray":null,"title":null,"error":null,"elapsed_ms":1,"request_path":"/"}],"cancelled":false,"path":"test.json","discovery_limited":false})).unwrap()
    }
    #[test]
    fn preserves_unrelated_hosts_and_rejects_existing_entries() {
        let report = report();
        let pins = candidates(&report);
        let existing = b"127.0.0.1 localhost\r\n192.168.1.2 host.docker.internal\r\n";
        let result = plan(&report, existing, pins.clone()).unwrap();
        assert!(result
            .after
            .starts_with(std::str::from_utf8(existing).unwrap()));
        assert!(result
            .after
            .contains("104.18.156.37 clientconfig.rpg.riotgames.com"));
        assert!(plan(&report, b"1.2.3.4 clientconfig.rpg.riotgames.com", pins).is_err());
    }
    #[test]
    fn refuses_vpn_forbidden_and_unverified_addresses() {
        let mut report = report();
        let pins = candidates(&report);
        report.probes[0].classification = "cloudflare_block".into();
        report.probes[0].http = Some(403);
        assert!(plan(&report, b"", pins.clone()).is_err());
        report.probes[0].classification = "http_success".into();
        report.probes[0].http = Some(200);
        report.context.connection = "vpn".into();
        assert!(plan(&report, b"", pins).is_err());
        report.context.connection = "direct".into();
        assert!(plan(
            &report,
            b"",
            vec![Pin {
                domain: "auth.riotgames.com".into(),
                ip: "1.2.3.4".parse().unwrap()
            }]
        )
        .is_err());
    }
}
