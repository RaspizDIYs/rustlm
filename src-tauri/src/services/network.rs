use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    net::{IpAddr, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

pub const CATALOG: &str = include_str!("../../resources/riot-domains.txt");
const MARKER: &str = ":: RUSTLM RIOT PROFILE";
const HOSTLIST: &str = "rustlm-riot-";
static PATCH_LOCK: Mutex<()> = Mutex::new(());
fn list_name(strategy: &str) -> String {
    format!("{HOSTLIST}{}.txt", &hash(strategy.as_bytes())[..12])
}
fn legacy_list_name(strategy: &str) -> String {
    list_name(strategy).replace(".txt", "-legacy.txt")
}

#[derive(Default)]
pub struct NetworkJobs(pub Mutex<Option<Arc<AtomicBool>>>);

#[derive(Clone, Serialize, Deserialize)]
pub struct RunContext {
    pub connection: String,
    pub provider: String,
    pub region: String,
    pub strategy: Option<String>,
    pub strategy_hash: Option<String>,
    #[serde(default)]
    pub observed_strategy: Option<String>,
    #[serde(default)]
    pub observed_command_hash: Option<String>,
    #[serde(default)]
    pub network_fingerprint: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct DnsResult {
    pub domain: String,
    pub resolver: String,
    pub addresses: Vec<IpAddr>,
    pub aliases: Vec<String>,
    pub error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Probe {
    pub domain: String,
    pub ip: IpAddr,
    pub port: u16,
    pub tcp: bool,
    pub tls: bool,
    pub http: Option<u16>,
    pub classification: String,
    pub server: Option<String>,
    pub ray: Option<String>,
    pub title: Option<String>,
    pub error: Option<String>,
    pub elapsed_ms: u128,
    pub request_path: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Report {
    pub id: String,
    pub created_at: String,
    pub context: RunContext,
    pub dns: Vec<DnsResult>,
    pub probes: Vec<Probe>,
    pub cancelled: bool,
    pub path: String,
    #[serde(default)]
    pub discovery_limited: bool,
}
#[derive(Clone, Serialize)]
pub struct Progress {
    pub phase: String,
    pub completed: usize,
    pub total: usize,
    pub current: String,
}

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read_small(path: &Path) -> Result<Vec<u8>, String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 2_000_000 {
        return Err("Файл больше 2 МБ".into());
    }
    fs::read(path).map_err(|e| e.to_string())
}
fn domains() -> BTreeSet<String> {
    CATALOG
        .lines()
        .map(str::trim)
        .filter(|s| valid_domain(s))
        .map(str::to_owned)
        .collect()
}
fn valid_domain(s: &str) -> bool {
    s.len() <= 253
        && s.contains('.')
        && s.split('.').all(|p| {
            !p.is_empty()
                && p.len() <= 63
                && !p.starts_with('-')
                && !p.ends_with('-')
                && p.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}

async fn dns_query(client: reqwest::Client, domain: String, resolver: String) -> DnsResult {
    let mut result = DnsResult {
        domain: domain.clone(),
        resolver: resolver.clone(),
        addresses: vec![],
        aliases: vec![],
        error: None,
    };
    if resolver == "system" {
        match tokio::time::timeout(
            Duration::from_secs(5),
            tokio::net::lookup_host((domain.as_str(), 443)),
        )
        .await
        {
            Ok(Ok(ips)) => result.addresses = ips.map(|s| s.ip()).collect(),
            Ok(Err(e)) => result.error = Some(e.to_string()),
            Err(_) => result.error = Some("DNS timeout".into()),
        }
    } else {
        for kind in ["A", "AAAA"] {
            let url = if resolver == "google" {
                "https://dns.google/resolve"
            } else {
                "https://cloudflare-dns.com/dns-query"
            };
            let answer = async {
                let response = client
                    .get(url)
                    .query(&[("name", domain.as_str()), ("type", kind)])
                    .header("accept", "application/dns-json")
                    .send()
                    .await?
                    .error_for_status()?;
                response.json::<serde_json::Value>().await
            }
            .await;
            match answer {
                Ok(json) => {
                    if json["Status"].as_u64().unwrap_or(2) != 0 {
                        result.error = Some(format!("DNS status {}", json["Status"]));
                    }
                    if let Some(answers) = json["Answer"].as_array() {
                        for item in answers {
                            if let Some(value) = item["data"].as_str() {
                                if let Ok(ip) = value.parse::<IpAddr>() {
                                    result.addresses.push(ip);
                                } else if item["type"] == 5 {
                                    let name = value.trim_end_matches('.').to_lowercase();
                                    if valid_domain(&name) {
                                        result.aliases.push(name);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => result.error = Some(e.to_string()),
            }
        }
    }
    result.addresses.sort();
    result.addresses.dedup();
    result.aliases.sort();
    result.aliases.dedup();
    result
}

pub fn classify(status: u16, body: &str) -> String {
    let lower = body.to_ascii_lowercase();
    if lower.contains("sorry, you have been blocked") || lower.contains("cf-error-details") {
        "cloudflare_block"
    } else if lower.contains("cf-chl-") || lower.contains("just a moment") {
        "challenge"
    } else if status == 401 {
        "authentication_required"
    } else if status == 403 {
        "http_forbidden"
    } else if (200..300).contains(&status) {
        "http_success"
    } else {
        "http_response"
    }
    .into()
}
fn title(body: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?is)<title[^>]*>(.*?)</title>").ok()?;
    re.captures(body)
        .map(|c| c[1].chars().filter(|c| !c.is_control()).take(200).collect())
}
fn request_path(domain: &str, region: &str) -> String {
    match domain {
        "auth.riotgames.com" => "/.well-known/openid-configuration".into(),
        "clientconfig.rpg.riotgames.com" => {
            "/api/v1/config/public?namespace=keystone.products.league_of_legends.patchlines.live"
                .into()
        }
        "lol.secure.dyn.riotcdn.net" => format!(
            "/channels/public/live-{}-win.json",
            match region {
                "RU" => "ru",
                "EUNE" => "eune",
                _ => "euw",
            }
        ),
        "ddragon.leagueoflegends.com" => format!(
            "/realms/{}.json",
            match region {
                "RU" => "ru",
                "EUNE" => "eune",
                _ => "euw",
            }
        ),
        _ => "/".into(),
    }
}
async fn probe(domain: String, ip: IpAddr, port: u16, region: String) -> Probe {
    let start = Instant::now();
    let mut result = Probe {
        domain: domain.clone(),
        ip,
        port,
        tcp: false,
        tls: false,
        http: None,
        classification: "tcp_error".into(),
        server: None,
        ray: None,
        title: None,
        error: None,
        elapsed_ms: 0,
        request_path: request_path(&domain, &region),
    };
    let addr = SocketAddr::new(ip, port);
    let transport = tokio::task::spawn_blocking(move || -> Result<(), (bool, String)> {
        let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
            .map_err(|e| (false, e.to_string()))?;
        tcp.set_read_timeout(Some(Duration::from_secs(4)))
            .map_err(|e| (true, e.to_string()))?;
        tcp.set_write_timeout(Some(Duration::from_secs(4)))
            .map_err(|e| (true, e.to_string()))?;
        native_tls::TlsConnector::new()
            .map_err(|e| (true, e.to_string()))?
            .connect(&domain, tcp)
            .map_err(|e| (true, e.to_string()))?;
        Ok(())
    })
    .await;
    match transport {
        Ok(Ok(())) => {
            result.tcp = true;
            result.tls = true;
            result.classification = "tls_success".into();
        }
        Ok(Err((tcp, e))) => {
            result.tcp = tcp;
            result.classification = if tcp { "tls_error" } else { "tcp_error" }.into();
            result.error = Some(e);
        }
        Err(e) => result.error = Some(e.to_string()),
    }
    if result.tls && port == 443 {
        // A second, pinned connection gets the HTTP evidence. Certificate validation stays on.
        let http = async {
            let client = reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(6))
                .resolve_to_addrs(&result.domain, &[addr])
                .build()?;
            let mut response = client
                .get(format!("https://{}{}", result.domain, result.request_path))
                .header("user-agent", "RustLM-Network-Diagnostics/1.0")
                .send()
                .await?;
            result.http = Some(response.status().as_u16());
            result.server = response
                .headers()
                .get("server")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            result.ray = response
                .headers()
                .get("cf-ray")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let mut body = Vec::new();
            while body.len() < 16384 {
                match response.chunk().await? {
                    Some(chunk) => {
                        let remaining = 16384 - body.len();
                        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
                    }
                    None => break,
                }
            }
            let body = String::from_utf8_lossy(&body);
            result.title = title(&body);
            result.classification = classify(result.http.unwrap_or(0), &body);
            Ok::<(), reqwest::Error>(())
        }
        .await;
        if let Err(e) = http {
            result.classification = "http_error".into();
            result.error = Some(e.to_string());
        }
    }
    result.elapsed_ms = start.elapsed().as_millis();
    result
}

pub async fn diagnose<F: Fn(Progress) + Send + Sync>(
    context: RunContext,
    root: PathBuf,
    cancel: Arc<AtomicBool>,
    progress: F,
) -> Result<Report, String> {
    if !["direct", "vpn", "zapret"].contains(&context.connection.as_str())
        || !["RU", "EUW", "EUNE", "ALL"].contains(&context.region.as_str())
        || context.provider.chars().count() > 160
    {
        return Err("Некорректный режим подключения".into());
    }
    let id = chrono::Utc::now().format("%Y%m%dT%H%M%S%9f").to_string();
    let path = root.join(format!("{id}.json"));
    let mut report = Report {
        id,
        created_at: chrono::Utc::now().to_rfc3339(),
        context,
        dns: vec![],
        probes: vec![],
        cancelled: false,
        path: path.to_string_lossy().into(),
        discovery_limited: false,
    };
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    let mut pending = domains();
    let mut visited = BTreeSet::new();
    // Bound CNAME expansion even when an upstream resolver returns changing/cyclic aliases.
    for _ in 0..8 {
        if pending.is_empty() || cancel.load(Ordering::Relaxed) {
            break;
        }
        visited.extend(pending.clone());
        let work: Vec<_> = pending
            .iter()
            .flat_map(|d| ["system", "google", "cloudflare"].map(|r| (d.clone(), r.to_owned())))
            .collect();
        let total = work.len();
        let mut stream = stream::iter(work)
            .map(|(d, r)| dns_query(client.clone(), d, r))
            .buffer_unordered(8);
        let mut done = 0;
        pending.clear();
        while let Some(answer) = stream.next().await {
            done += 1;
            progress(Progress {
                phase: "dns".into(),
                completed: done,
                total,
                current: answer.domain.clone(),
            });
            for alias in &answer.aliases {
                if !visited.contains(alias) {
                    if visited.len() + pending.len() < 1000 {
                        pending.insert(alias.clone());
                    } else {
                        report.discovery_limited = true;
                    }
                }
            }
            report.dns.push(answer);
            if cancel.load(Ordering::Relaxed) {
                break;
            }
        }
    }
    if !pending.is_empty() {
        report.discovery_limited = true;
    }
    let mut endpoints = BTreeSet::new();
    for dns in &report.dns {
        for &ip in &dns.addresses {
            if !public_ip(ip) {
                continue;
            }
            endpoints.insert((dns.domain.clone(), ip, 443));
            if dns.domain.ends_with(".chat.si.riotgames.com") {
                endpoints.insert((dns.domain.clone(), ip, 5223));
            }
            if dns.domain.starts_with("feapp.") && dns.domain.ends_with(".lol.pvp.net") {
                endpoints.insert((dns.domain.clone(), ip, 2099));
            }
        }
    }
    let total = endpoints.len();
    let region = report.context.region.clone();
    let mut stream = stream::iter(endpoints)
        .map(|(d, ip, port)| probe(d, ip, port, region.clone()))
        .buffer_unordered(8);
    if !cancel.load(Ordering::Relaxed) {
        while let Some(answer) = stream.next().await {
            progress(Progress {
                phase: "network".into(),
                completed: report.probes.len() + 1,
                total,
                current: format!("{}:{}", answer.domain, answer.port),
            });
            report.probes.push(answer);
            if cancel.load(Ordering::Relaxed) {
                break;
            }
        }
    }
    report.cancelled = cancel.load(Ordering::Relaxed);
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    fs::write(
        &path,
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(report)
}
pub(super) fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_documentation()
                && ip.octets()[0] != 0
                && ip.octets()[0] < 224
                && !(ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1]))
        }
        IpAddr::V6(ip) => {
            !ip.is_loopback()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && ip.segments()[0] & 0xfe00 != 0xfc00
                && ip.segments()[0] & 0xffc0 != 0xfe80
                && ip.to_ipv4_mapped().is_none()
        }
    }
}

#[derive(Serialize)]
pub struct Installation {
    pub root: String,
    pub strategies: Vec<String>,
    pub backups: Vec<String>,
    pub backup_directory: String,
    pub profiles: Vec<ManagedProfile>,
}
pub fn backup_directory(root: &Path, store: &Path) -> PathBuf {
    store.join(hash(
        root.to_string_lossy()
            .trim_start_matches("\\\\?\\")
            .to_lowercase()
            .as_bytes(),
    ))
}
pub fn migrate_backups(path: &str, store: &Path) -> Result<(), String> {
    let _guard = PATCH_LOCK.lock().map_err(|e| e.to_string())?;
    let root = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let legacy = root.join(".rustlm-backups");
    if !legacy.exists() {
        return Ok(());
    }
    if !fs::canonicalize(&legacy)
        .map_err(|e| e.to_string())?
        .starts_with(&root)
    {
        return Err("Папка старых копий находится вне Zapret".into());
    }
    let target = backup_directory(&root, store);
    fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(&legacy).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let id = entry.file_name().to_string_lossy().to_string();
        if ![18, 24].contains(&id.len())
            || !id.bytes().all(|b| b.is_ascii_digit() || b == b'T')
            || !entry.file_type().map_err(|e| e.to_string())?.is_dir()
        {
            continue;
        }
        let source = entry.path();
        let files = fs::read_dir(&source)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if files.iter().any(|f| {
            !["backup.json", "hosts.original", "restored"]
                .contains(&f.file_name().to_string_lossy().as_ref())
                || !f.file_type().is_ok_and(|t| t.is_file())
        }) {
            continue;
        }
        let destination = target.join(&id);
        let manifest = if source.join("backup.json").exists() {
            source.join("backup.json")
        } else {
            destination.join("backup.json")
        };
        let _: Backup =
            serde_json::from_slice(&read_small(&manifest)?).map_err(|e| e.to_string())?;
        fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
        for file in &files {
            let bytes = fs::read(file.path()).map_err(|e| e.to_string())?;
            let dest = destination.join(file.file_name());
            if dest.exists() {
                if fs::read(&dest).map_err(|e| e.to_string())? != bytes {
                    return Err(
                        "Конфликт при переносе старой резервной копии. Оригинал сохранён.".into(),
                    );
                }
            } else {
                atomic_write(&dest, &bytes)?;
            }
            if fs::read(&dest).map_err(|e| e.to_string())? != bytes {
                return Err("Не удалось проверить перенос резервной копии".into());
            }
        }
        for file in files {
            fs::remove_file(file.path()).map_err(|e| e.to_string())?;
        }
        fs::remove_dir(source).map_err(|e| e.to_string())?;
    }
    let _ = fs::remove_dir(legacy);
    Ok(())
}
pub fn installation(path: &str) -> Result<Installation, String> {
    let root = fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !root.join("bin/winws.exe").is_file() || !root.join("lists").is_dir() {
        return Err("Выберите папку Zapret с bin/winws.exe и lists. Zapret2 и другие форматы пока не поддерживаются.".into());
    }
    let mut strategies = Vec::new();
    for file in fs::read_dir(&root).map_err(|e| e.to_string())? {
        let file = file.map_err(|e| e.to_string())?;
        let name = file.file_name().to_string_lossy().to_string();
        if name.to_lowercase().starts_with("general") && name.to_lowercase().ends_with(".bat") {
            strategies.push(name);
        }
    }
    strategies.sort();
    let backup_root = backup_directory(&root, &super::repair_paths::root()?.join("backups"));
    let mut backups = Vec::new();
    for base in [&backup_root, &root.join(".rustlm-backups")] {
        if base.starts_with(&root) && fs::canonicalize(base).is_ok_and(|p| !p.starts_with(&root)) {
            continue;
        }
        if let Ok(entries) = fs::read_dir(base) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if [18, 24].contains(&name.len())
                    && name.bytes().all(|b| b.is_ascii_digit() || b == b'T')
                    && entry.path().join("backup.json").is_file()
                    && !entry.path().join("restored").exists()
                {
                    backups.push(name);
                }
            }
        }
    }
    backups.sort();
    backups.dedup();
    Ok(Installation {
        root: root.to_string_lossy().into(),
        strategies,
        backups,
        backup_directory: backup_root.to_string_lossy().into(),
        profiles: managed_profiles(&root, &super::repair_paths::root()?.join("backups"))?,
    })
}
fn child(root: &Path, name: &str) -> Result<PathBuf, String> {
    let relative = Path::new(name);
    if relative
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("Недопустимый путь".into());
    }
    let path = root.join(relative);
    let check = if path.exists() {
        path.clone()
    } else {
        path.parent().ok_or("Нет родительской папки")?.into()
    };
    if !fs::canonicalize(check)
        .map_err(|e| e.to_string())?
        .starts_with(root)
    {
        return Err("Путь выходит за папку Zapret".into());
    }
    Ok(path)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Patch {
    pub root: String,
    pub strategy: String,
    pub before_hash: String,
    pub before: String,
    pub after: String,
    pub domains: String,
    pub variant: u8,
    pub source_report: Option<String>,
    pub legacy_ips: Option<String>,
}
pub fn preview(path: &str, strategy: &str) -> Result<Patch, String> {
    let info = installation(path)?;
    if !info.strategies.iter().any(|s| s == strategy) {
        return Err("Стратегия не найдена".into());
    }
    let root = Path::new(&info.root);
    let hostlist = list_name(strategy);
    let bytes = read_small(&child(root, strategy)?)?;
    let text = String::from_utf8(bytes.clone())
        .map_err(|_| "BAT должен быть в UTF-8: автоматическая перекодировка отключена")?;
    if text.contains(MARKER) {
        return Err("Профиль RustLM уже применён. Сначала восстановите резервную копию.".into());
    }
    let normalized = text.replace("\r\n", "\n");
    let lines: Vec<_> = normalized.lines().collect();
    let index = lines.iter().position(|l| l.starts_with("--filter-tcp=80,443 ") && l.contains("--hostlist=")).ok_or("Формат стратегии не распознан: нужен отдельный TCP-профиль 80,443 с --hostlist. Файл не изменён.")?;
    let token = regex::Regex::new(r#"--(?:hostlist|hostlist-exclude|ipset|ipset-exclude)(?:-domains|-ip)?=(?:"[^"]*"|\S+)\s*"#).map_err(|e| e.to_string())?;
    let template = token
        .replace_all(lines[index], "")
        .trim()
        .trim_end_matches('^')
        .trim()
        .to_string();
    if !template.ends_with("--new") {
        return Err(
            "Многострочный или завершающий профиль не поддерживается; файл не изменён.".into(),
        );
    }
    let riot = template
        .replacen(
            "--filter-tcp=80,443",
            "--filter-tcp=80,443,2099,5222,5223",
            1,
        )
        .replacen(
            " --new",
            &format!(" --hostlist=\"%LISTS%{hostlist}\" --new ^"),
            1,
        );
    let udp = lines.iter().find(|l| l.starts_with("--filter-udp=443 ") && l.contains("--hostlist="))
        .map(|line| token.replace_all(line, "").trim().trim_end_matches('^').trim().to_string())
        .filter(|line| line.ends_with("--new"))
        .ok_or("Не найден отдельный UDP 443 профиль. Автоматическая настройка этой сборки не поддерживается.")?
        .replacen(" --new", &format!(" --hostlist=\"%LISTS%{hostlist}\" --new ^"), 1);
    let anchor = lines
        .iter()
        .position(|l| l.starts_with("--filter-"))
        .ok_or("Не найдены профили winws")?;
    if anchor == 0 || !lines[anchor - 1].trim_end().ends_with('^') {
        return Err("Не найден непрерывный вызов winws; файл не изменён.".into());
    }
    let wf = regex::Regex::new(r"--wf-tcp=([0-9,%A-Za-z_-]+)").map_err(|e| e.to_string())?;
    if wf.captures_iter(&normalized).count() != 1 {
        return Err("Нужен единственный --wf-tcp".into());
    }
    let mut output = lines.clone();
    output.insert(anchor, &udp);
    output.insert(anchor, &riot);
    let joined = output.join("\n");
    let after = wf
        .replace(&joined, "--wf-tcp=$1,2099,5222,5223")
        .to_string();
    // Put the marker before the command, never inside cmd.exe's continued argument list.
    let start = after.find("start ").ok_or("Не найден start winws")?;
    let after = format!("{}{}\n{}", &after[..start], MARKER, &after[start..]);
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut after = after.replace('\n', newline);
    if text.ends_with('\n') {
        after.push_str(newline);
    }
    Ok(Patch {
        root: info.root,
        strategy: strategy.into(),
        before_hash: hash(&bytes),
        before: text,
        after,
        domains: format!(
            "# RustLM Riot catalog, snapshot 2026-10-26; subdomains match automatically.\r\n{}\r\n",
            domains().into_iter().collect::<Vec<_>>().join("\r\n")
        ),
        variant: 0,
        source_report: None,
        legacy_ips: None,
    })
}

pub fn preview_variant(
    path: &str,
    strategy: &str,
    variant: u8,
    report: Option<&Report>,
) -> Result<Patch, String> {
    if variant > 2 {
        return Err("Допустимы только три проверяемых варианта".into());
    }
    let mut patch = preview(path, strategy)?;
    patch.variant = variant;
    if variant > 0 {
        let hostlist = list_name(strategy);
        let options = if variant == 1 {
            "--dpi-desync=multisplit --dpi-desync-split-pos=1,midsld"
        } else {
            "--dpi-desync=multidisorder --dpi-desync-split-pos=1,midsld"
        };
        let replacement = format!(
            "--filter-tcp=80,443,2099,5222,5223 --hostlist=\"%LISTS%{hostlist}\" {options} --new ^"
        );
        let newline = if patch.after.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let mut lines: Vec<String> = patch.after.lines().map(str::to_owned).collect();
        let line = lines
            .iter_mut()
            .find(|l| l.starts_with("--filter-tcp=") && l.contains(&hostlist))
            .ok_or("Профиль Riot не найден")?;
        *line = replacement;
        patch.after = lines.join(newline) + newline;
    }
    if let Some(report) = report {
        if report.cancelled || report.discovery_limited {
            return Err("Нужен завершённый замер без ограничения DNS-обхода".into());
        }
        let mut names = domains();
        for answer in &report.dns {
            if valid_domain(&answer.domain) {
                names.insert(answer.domain.clone());
            }
            for alias in &answer.aliases {
                if valid_domain(alias) {
                    names.insert(alias.clone());
                }
            }
        }
        patch.domains = format!(
            "# RustLM Riot catalog plus locally discovered DNS chains\r\n{}\r\n",
            names.into_iter().collect::<Vec<_>>().join("\r\n")
        );
        patch.source_report = Some(report.id.clone());
        let mut legacy = BTreeSet::new();
        for answer in &report.dns {
            if answer.domain.ends_with(".chat.si.riotgames.com")
                || (answer.domain.starts_with("feapp.") && answer.domain.ends_with(".lol.pvp.net"))
            {
                legacy.extend(answer.addresses.iter().copied().filter(|ip| public_ip(*ip)));
            }
        }
        if !legacy.is_empty() {
            let name = legacy_list_name(strategy);
            let hostlist = list_name(strategy);
            let newline = if patch.after.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            let mut lines: Vec<String> = patch.after.lines().map(str::to_owned).collect();
            let index = lines
                .iter()
                .position(|l| l.starts_with("--filter-tcp=") && l.contains(&hostlist))
                .ok_or("TCP профиль не найден")?;
            let fallback = lines[index]
                .replace(
                    "--filter-tcp=80,443,2099,5222,5223",
                    "--filter-tcp=2099,5223",
                )
                .replace(
                    &format!("--hostlist=\"%LISTS%{hostlist}\""),
                    &format!("--ipset=\"%LISTS%{name}\" --dpi-desync-skip-nosni=0"),
                );
            lines.insert(index + 1, fallback);
            patch.after = lines.join(newline) + newline;
            patch.legacy_ips = Some(
                legacy
                    .into_iter()
                    .map(|ip| ip.to_string())
                    .collect::<Vec<_>>()
                    .join("\r\n")
                    + "\r\n",
            );
        }
    }
    Ok(patch)
}

pub fn snapshot(path: &str, strategy: &str, store: &Path) -> Result<String, String> {
    let _guard = PATCH_LOCK.lock().map_err(|e| e.to_string())?;
    let patch = preview(path, strategy)?;
    let root = Path::new(&patch.root);
    let name = format!("lists/{}", list_name(strategy));
    let list = child(root, &name)?;
    let list_bytes = if list.exists() {
        Some(read_small(&list)?)
    } else {
        None
    };
    let files = vec![
        SavedFile {
            name: strategy.into(),
            before: Some(patch.before.into_bytes()),
            after_hash: patch.before_hash,
            after: None,
        },
        SavedFile {
            name,
            after_hash: list_bytes.as_ref().map(|v| hash(v)).unwrap_or_default(),
            before: list_bytes,
            after: None,
        },
    ];
    let base = backup_directory(root, store);
    fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let id = chrono::Utc::now().format("%Y%m%dT%H%M%S%9f").to_string();
    let dir = base.join(&id);
    fs::create_dir(&dir).map_err(|e| e.to_string())?;
    fs::write(
        dir.join("backup.json"),
        serde_json::to_vec_pretty(&Backup { files }).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

#[derive(Clone, Serialize, Deserialize)]
struct SavedFile {
    name: String,
    before: Option<Vec<u8>>,
    after_hash: String,
    #[serde(default)]
    after: Option<Vec<u8>>,
}
#[derive(Serialize, Deserialize)]
struct Backup {
    files: Vec<SavedFile>,
}
pub fn apply(patch: Patch, report: Option<&Report>, store: &Path) -> Result<String, String> {
    let _guard = PATCH_LOCK.lock().map_err(|e| e.to_string())?;
    // Recompute the entire plan: the UI cannot supply arbitrary file contents or paths.
    let expected = preview_variant(&patch.root, &patch.strategy, patch.variant, report)?;
    if patch.before_hash != expected.before_hash
        || patch.after != expected.after
        || patch.domains != expected.domains
        || patch.source_report != expected.source_report
        || patch.legacy_ips != expected.legacy_ips
    {
        return Err("Файлы или план изменились. Постройте предварительный просмотр заново.".into());
    }
    let root = Path::new(&expected.root);
    let mut writes = vec![
        (expected.strategy.clone(), expected.after.into_bytes()),
        (
            format!("lists/{}", list_name(&expected.strategy)),
            expected.domains.into_bytes(),
        ),
    ];
    if let Some(ips) = expected.legacy_ips {
        writes.push((
            format!("lists/{}", legacy_list_name(&expected.strategy)),
            ips.into_bytes(),
        ));
    }
    let mut files = Vec::new();
    for (name, data) in &writes {
        let path = child(root, name)?;
        let before = if path.exists() {
            Some(read_small(&path)?)
        } else {
            None
        };
        files.push(SavedFile {
            name: name.clone(),
            before,
            after_hash: hash(data),
            after: Some(data.clone()),
        });
    }
    let backup_root = backup_directory(root, store);
    fs::create_dir_all(&backup_root).map_err(|e| e.to_string())?;
    let id = chrono::Utc::now().format("%Y%m%dT%H%M%S%9f").to_string();
    let dir = backup_root.join(&id);
    fs::create_dir(&dir).map_err(|e| e.to_string())?;
    fs::write(
        dir.join("backup.json"),
        serde_json::to_vec_pretty(&Backup {
            files: files.clone(),
        })
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let profile_dir = backup_root.join("profiles");
    fs::create_dir_all(&profile_dir).map_err(|e| e.to_string())?;
    atomic_write(
        &profile_dir.join(format!("{}.json", hash(expected.strategy.as_bytes()))),
        &serde_json::to_vec_pretty(&ProfileData {
            strategy: expected.strategy.clone(),
            variant: expected.variant,
            files,
        })
        .map_err(|e| e.to_string())?,
    )?;
    // Write the list first so a failed list write cannot leave a launcher with a missing dependency.
    for (name, data) in writes.iter().rev() {
        if let Err(error) = atomic_write(&child(root, name)?, data) {
            let rollback = restore_inner(&expected.root, &id, false, store);
            return Err(format!(
                "Запись не завершена: {error}. Откат: {rollback:?}. Резервная копия: {id}"
            ));
        }
    }
    Ok(id)
}
#[derive(Serialize, Deserialize)]
struct ProfileData {
    strategy: String,
    variant: u8,
    files: Vec<SavedFile>,
}
#[derive(Serialize)]
pub struct ManagedProfile {
    pub strategy: String,
    pub variant: u8,
    pub state: String,
}
fn managed_profiles(root: &Path, store: &Path) -> Result<Vec<ManagedProfile>, String> {
    let dir = backup_directory(root, store).join("profiles");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut profiles = vec![];
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())?.take(100) {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let profile: ProfileData =
            serde_json::from_slice(&read_small(&path)?).map_err(|e| e.to_string())?;
        let mut original = true;
        let mut configured = true;
        for file in &profile.files {
            let path = child(root, &file.name)?;
            let current = if path.exists() {
                Some(read_small(&path)?)
            } else {
                None
            };
            original &= current == file.before;
            configured &= current == file.after;
        }
        profiles.push(ManagedProfile {
            strategy: profile.strategy,
            variant: profile.variant,
            state: if configured {
                "configured"
            } else if original {
                "original"
            } else {
                "modified"
            }
            .into(),
        });
    }
    profiles.sort_by(|a, b| a.strategy.cmp(&b.strategy));
    Ok(profiles)
}
pub fn select_profile(
    path: &str,
    strategy: &str,
    configured: bool,
    store: &Path,
) -> Result<String, String> {
    let _guard = PATCH_LOCK.lock().map_err(|e| e.to_string())?;
    let root = PathBuf::from(installation(path)?.root);
    let base = backup_directory(&root, store);
    let profile: ProfileData = serde_json::from_slice(&read_small(
        &base
            .join("profiles")
            .join(format!("{}.json", hash(strategy.as_bytes()))),
    )?)
    .map_err(|e| e.to_string())?;
    if profile.strategy != strategy || !(2..=3).contains(&profile.files.len()) {
        return Err("Некорректный профиль стратегии".into());
    }
    let mut files = vec![];
    for file in &profile.files {
        if file.name != strategy
            && file.name != format!("lists/{}", list_name(strategy))
            && file.name != format!("lists/{}", legacy_list_name(strategy))
        {
            return Err("Недопустимый файл профиля".into());
        }
        let target = child(&root, &file.name)?;
        let current = if target.exists() {
            Some(read_small(&target)?)
        } else {
            None
        };
        if current != file.before && current != file.after {
            return Err(format!(
                "{} изменён вручную. Переключение остановлено, чтобы сохранить изменения.",
                file.name
            ));
        }
        let chosen = if configured {
            file.after.clone()
        } else {
            file.before.clone()
        };
        files.push(SavedFile {
            name: file.name.clone(),
            before: current,
            after_hash: chosen.as_ref().map(|v| hash(v)).unwrap_or_default(),
            after: chosen,
        });
    }
    let id = chrono::Utc::now().format("%Y%m%dT%H%M%S%9f").to_string();
    let dir = base.join(&id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(
        dir.join("backup.json"),
        serde_json::to_vec_pretty(&Backup {
            files: files.clone(),
        })
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    for file in files.iter().rev() {
        let target = child(&root, &file.name)?;
        let result = if let Some(bytes) = &file.after {
            atomic_write(&target, bytes)
        } else if target.exists() {
            fs::remove_file(target).map_err(|e| e.to_string())
        } else {
            Ok(())
        };
        if let Err(error) = result {
            let rollback = restore_inner(path, &id, false, store);
            return Err(format!(
                "Не удалось переключить профиль: {error}. Откат: {rollback:?}. Копия: {id}"
            ));
        }
    }
    Ok(id)
}
pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_file_name(format!(
        ".rustlm-{}-{}.tmp",
        std::process::id(),
        rand::random::<u64>()
    ));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&temporary);
        return Err(e.to_string());
    }
    Ok(())
}
pub fn restore(path: &str, id: &str, check_changes: bool, store: &Path) -> Result<(), String> {
    let _guard = PATCH_LOCK.lock().map_err(|e| e.to_string())?;
    restore_inner(path, id, check_changes, store)
}
fn restore_inner(path: &str, id: &str, check_changes: bool, store: &Path) -> Result<(), String> {
    let root = PathBuf::from(installation(path)?.root);
    if !id.bytes().all(|c| c.is_ascii_digit() || c == b'T') || ![18, 24].contains(&id.len()) {
        return Err("Некорректный ID резервной копии".into());
    }
    let current = backup_directory(&root, store).join(id).join("backup.json");
    let manifest = if current.exists() {
        current
    } else {
        child(&root, &format!(".rustlm-backups/{id}/backup.json"))?
    };
    let backup: Backup =
        serde_json::from_slice(&read_small(&manifest)?).map_err(|e| e.to_string())?;
    if !(2..=3).contains(&backup.files.len()) {
        return Err("Некорректный состав резервной копии".into());
    }
    for file in &backup.files {
        let is_list = file.name.starts_with(&format!("lists/{HOSTLIST}"))
            && file.name.ends_with(".txt")
            && Path::new(&file.name).components().count() == 2;
        if !is_list
            && !(file.name.to_ascii_lowercase().starts_with("general")
                && file.name.to_ascii_lowercase().ends_with(".bat")
                && Path::new(&file.name).components().count() == 1)
        {
            return Err("Недопустимый файл в резервной копии".into());
        }
        let path = child(&root, &file.name)?;
        let already_restored = match &file.before {
            Some(bytes) => path.exists() && hash(&read_small(&path)?) == hash(bytes),
            None => !path.exists(),
        };
        let expected_after = if file.after_hash.is_empty() {
            !path.exists()
        } else {
            path.exists() && hash(&read_small(&path)?) == file.after_hash
        };
        if check_changes && !already_restored && !expected_after {
            return Err(format!(
                "{} изменён после настройки. Автооткат остановлен, чтобы сохранить ваши изменения.",
                file.name
            ));
        }
    }
    for file in &backup.files {
        let path = child(&root, &file.name)?;
        let already_restored = match &file.before {
            Some(bytes) => path.exists() && hash(&read_small(&path)?) == hash(bytes),
            None => !path.exists(),
        };
        if already_restored {
            continue;
        }
        match &file.before {
            Some(bytes) => atomic_write(&path, bytes)?,
            None => {
                if path.exists() {
                    fs::remove_file(&path).map_err(|e| e.to_string())?;
                }
            }
        }
    }
    fs::write(manifest.with_file_name("restored"), b"restored").map_err(|e| e.to_string())?;
    Ok(())
}

pub fn history(root: &Path) -> Result<Vec<Report>, String> {
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut paths: Vec<_> = fs::read_dir(root)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    paths.reverse();
    paths
        .into_iter()
        .take(20)
        .map(|p| {
            serde_json::from_slice(&fs::read(p).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_backups_profiles_and_legacy_migration_preserve_original_files() {
        let dir = fixture();
        let root = dir.to_str().unwrap();
        let store = std::env::temp_dir().join(format!("rustlm-store-{}", rand::random::<u64>()));
        let original = fs::read(dir.join("general (ALT11).bat")).unwrap();
        let id = apply(preview(root, "general (ALT11).bat").unwrap(), None, &store).unwrap();
        assert!(!dir.join(".rustlm-backups").exists());
        let configured = fs::read(dir.join("general (ALT11).bat")).unwrap();
        let switched = select_profile(root, "general (ALT11).bat", false, &store).unwrap();
        assert_eq!(fs::read(dir.join("general (ALT11).bat")).unwrap(), original);
        restore(root, &switched, true, &store).unwrap();
        assert_eq!(
            fs::read(dir.join("general (ALT11).bat")).unwrap(),
            configured
        );
        select_profile(root, "general (ALT11).bat", false, &store).unwrap();
        select_profile(root, "general (ALT11).bat", true, &store).unwrap();
        assert_eq!(
            fs::read(dir.join("general (ALT11).bat")).unwrap(),
            configured
        );
        fs::write(dir.join("general (ALT11).bat"), "user changes").unwrap();
        assert!(select_profile(root, "general (ALT11).bat", false, &store).is_err());
        fs::write(dir.join("general (ALT11).bat"), configured).unwrap();
        let central = backup_directory(&fs::canonicalize(&dir).unwrap(), &store);
        fs::create_dir(dir.join(".rustlm-backups")).unwrap();
        fs::rename(central.join(&id), dir.join(".rustlm-backups").join(&id)).unwrap();
        migrate_backups(root, &store).unwrap();
        assert!(!dir.join(".rustlm-backups").exists());
        assert!(central.join(&id).join("backup.json").exists());
        restore(root, &id, true, &store).unwrap();
        assert_eq!(fs::read(dir.join("general (ALT11).bat")).unwrap(), original);
        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(store).unwrap();
    }
    #[test]
    fn forbidden_response_never_means_success() {
        assert_eq!(
            classify(403, "<h1>Sorry, you have been blocked</h1>"),
            "cloudflare_block"
        );
        assert_eq!(classify(403, "Forbidden"), "http_forbidden");
        assert_eq!(classify(401, ""), "authentication_required");
        assert_eq!(
            classify(200, "<title>Just a moment...</title>"),
            "challenge"
        );
    }
    #[test]
    fn rejects_private_dns_answers() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "192.168.1.2",
            "169.254.169.254",
            "::1",
            "fd00::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(public_ip("104.16.119.50".parse().unwrap()));
    }
    fn fixture() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rustlm-network-{}", rand::random::<u64>()));
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::create_dir(dir.join("lists")).unwrap();
        fs::write(dir.join("bin/winws.exe"), []).unwrap();
        fs::write(dir.join("general (ALT11).bat"), "@echo off\r\nset \"LISTS=%~dp0lists\\\"\r\nstart \"zapret\" winws.exe --wf-tcp=80,443,%GameFilterTCP% ^\r\n--filter-tcp=80,443 --hostlist=\"%LISTS%general.txt\" --hostlist-exclude=\"%LISTS%exclude.txt\" --dpi-desync=fake --new ^\r\n--filter-udp=443 --hostlist=\"%LISTS%general.txt\" --dpi-desync=fake --new ^\r\n--filter-udp=12 --dpi-desync=fake\r\n").unwrap();
        dir
    }
    #[test]
    fn patch_keeps_original_rules_and_restores_original_bytes() {
        let dir = fixture();
        let root = dir.to_str().unwrap();
        let patch = preview(root, "general (ALT11).bat").unwrap();
        let original = patch.before.clone();
        assert!(patch.after.find(HOSTLIST).unwrap() < patch.after.find("general.txt").unwrap());
        assert!(patch
            .after
            .contains("--hostlist-exclude=\"%LISTS%exclude.txt\""));
        let id = apply(patch, None, &dir.join("test-store")).unwrap();
        restore(root, &id, true, &dir.join("test-store")).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("general (ALT11).bat")).unwrap(),
            original
        );
        assert!(!dir
            .join(format!("lists/{}", list_name("general (ALT11).bat")))
            .exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn refuses_changed_plan_and_conflicting_restore() {
        let dir = fixture();
        let root = dir.to_str().unwrap();
        let mut patch = preview(root, "general (ALT11).bat").unwrap();
        patch.after.push_str("malicious");
        assert!(apply(patch, None, &dir.join("test-store")).is_err());
        let id = apply(
            preview(root, "general (ALT11).bat").unwrap(),
            None,
            &dir.join("test-store"),
        )
        .unwrap();
        fs::write(dir.join("general (ALT11).bat"), "user edits").unwrap();
        assert!(restore(root, &id, true, &dir.join("test-store")).is_err());
        assert_eq!(
            fs::read_to_string(dir.join("general (ALT11).bat")).unwrap(),
            "user edits"
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn rejects_traversal_and_unsupported_batches() {
        let dir = fixture();
        let root = fs::canonicalize(&dir).unwrap();
        assert!(child(&root, "../other.bat").is_err());
        assert!(preview(root.to_str().unwrap(), "../general.bat").is_err());
        fs::write(dir.join("general (ALT11).bat"), "unrecognized script").unwrap();
        assert!(preview(root.to_str().unwrap(), "general (ALT11).bat").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn restoring_one_strategy_keeps_the_other_strategy_and_its_list() {
        let dir = fixture();
        let root = dir.to_str().unwrap();
        fs::copy(
            dir.join("general (ALT11).bat"),
            dir.join("general (ALT13).bat"),
        )
        .unwrap();
        let first = apply(
            preview(root, "general (ALT11).bat").unwrap(),
            None,
            &dir.join("test-store"),
        )
        .unwrap();
        let second = apply(
            preview(root, "general (ALT13).bat").unwrap(),
            None,
            &dir.join("test-store"),
        )
        .unwrap();
        let current = fs::read(dir.join("general (ALT13).bat")).unwrap();
        restore(root, &first, true, &dir.join("test-store")).unwrap();
        assert_eq!(fs::read(dir.join("general (ALT13).bat")).unwrap(), current);
        assert!(dir
            .join(format!("lists/{}", list_name("general (ALT13).bat")))
            .exists());
        restore(root, &second, true, &dir.join("test-store")).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn rollback_resumes_after_a_partially_restored_transaction() {
        let dir = fixture();
        let root = dir.to_str().unwrap();
        let plan = preview(root, "general (ALT11).bat").unwrap();
        let original = plan.before.clone();
        let id = apply(plan, None, &dir.join("test-store")).unwrap();
        fs::write(dir.join("general (ALT11).bat"), original).unwrap();
        restore(root, &id, true, &dir.join("test-store")).unwrap();
        restore(root, &id, true, &dir.join("test-store")).unwrap();
        assert!(!dir
            .join(format!("lists/{}", list_name("general (ALT11).bat")))
            .exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn report_adds_discovered_aliases_and_public_legacy_ips_with_full_rollback() {
        let dir = fixture();
        let root = dir.to_str().unwrap();
        let report:Report=serde_json::from_value(serde_json::json!({"id":"test","created_at":"now","context":{"connection":"zapret","provider":"test","region":"EUW","strategy":null,"strategy_hash":null},"dns":[{"domain":"euw1.chat.si.riotgames.com","resolver":"google","addresses":["172.65.252.238","127.0.0.1"],"aliases":["new-riot-cdn.example.net"],"error":null}],"probes":[],"cancelled":false,"path":"test.json","discovery_limited":false})).unwrap();
        let plan = preview_variant(root, "general (ALT11).bat", 1, Some(&report)).unwrap();
        assert!(plan.domains.contains("new-riot-cdn.example.net"));
        assert!(plan.after.contains("--dpi-desync-skip-nosni=0"));
        assert!(plan.legacy_ips.as_ref().unwrap().contains("172.65.252.238"));
        assert!(!plan.legacy_ips.as_ref().unwrap().contains("127.0.0.1"));
        let id = apply(plan, Some(&report), &dir.join("test-store")).unwrap();
        restore(root, &id, true, &dir.join("test-store")).unwrap();
        assert!(!dir
            .join(format!("lists/{}", legacy_list_name("general (ALT11).bat")))
            .exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    #[ignore = "Live HTTPS smoke; uses the current network without changing system settings"]
    async fn live_public_https_smoke() {
        let domain = "ddragon.leagueoflegends.com";
        let ip = tokio::net::lookup_host((domain, 443))
            .await
            .unwrap()
            .map(|a| a.ip())
            .find(IpAddr::is_ipv4)
            .expect("IPv4 DNS answer");
        let result = probe(domain.into(), ip, 443, "EUW".into()).await;
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
        assert!(result.tcp, "TCP: {:?}", result.error);
        assert!(result.tls, "TLS certificate validation: {:?}", result.error);
        assert_eq!(
            result.http,
            Some(200),
            "Public DDragon endpoint: {:?}",
            result.error
        );
    }
}
