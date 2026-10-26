use super::network::{self, Installation};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(super) fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
pub(super) fn elevated(script: &str) -> Result<String, String> {
    use std::io::Write;
    let dir = super::repair_paths::root()?.join("helpers");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("helper-{}.ps1", rand::random::<u64>()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    file.write_all(script.as_bytes())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    let cancel_path = path.with_extension("cancel");
    // Verify the exact bytes before execution; timed-out helpers cannot apply changes later.
    let wrapper=format!("$ErrorActionPreference='Stop';$script:RustLMCancelPath={cancel};if(Test-Path -LiteralPath $script:RustLMCancelPath){{throw 'OPERATION_CANCELLED'}};$b=[IO.File]::ReadAllBytes({path});$h=([BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($b))).Replace('-','').ToLowerInvariant();if($h -ne {hash}){{throw 'Содержимое помощника изменилось'}};& ([ScriptBlock]::Create([Text.Encoding]::UTF8.GetString($b)))",cancel=literal(&cancel_path.to_string_lossy()),path=literal(&path.to_string_lossy()),hash=literal(&network::hash(script.as_bytes())));

    let inner = STANDARD.encode(
        wrapper
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    let result = powershell(&helper_command(&inner, true));
    if result.as_ref().err().is_some_and(|e| e.contains("TIMEOUT")) {
        let _ = fs::write(&cancel_path, b"cancelled");
    }

    let _ = fs::remove_file(path);
    result
}
fn helper_command(inner: &str, elevated: bool) -> String {
    let verb = if elevated { "-Verb RunAs" } else { "" };
    // Wait for the helper only; Start-Process -Wait would also wait for winws forever.
    format!("$ErrorActionPreference='Stop';try {{$p=Start-Process -FilePath powershell.exe {verb} -WindowStyle Hidden -ArgumentList @('-NoProfile','-NonInteractive','-EncodedCommand',{}) -PassThru}}catch{{if($_.Exception.NativeErrorCode -eq 1223){{throw 'UAC_CANCELLED: запрос прав отменён'}};throw}};$helperHandle=$p.Handle;if(-not $p.WaitForExit(45000)){{throw 'ELEVATION_TIMEOUT: помощник Windows не завершился за 45 секунд'}};$p.Refresh();if($p.ExitCode -ne 0){{throw 'Помощник Windows завершился с ошибкой'}};'OK'",literal(inner))
}
fn integrate_service_manager(root: &str, store: &Path) -> Result<(), String> {
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let path = root.join("service.bat");
    if !path.exists() {
        return Ok(());
    }
    if !fs::canonicalize(&path)
        .map_err(|e| e.to_string())?
        .starts_with(&root)
    {
        return Err("service.bat вне выбранной папки".into());
    }
    let before = fs::read(&path).map_err(|e| e.to_string())?;
    let source =
        String::from_utf8(before.clone()).map_err(|_| "Неизвестная кодировка service.bat")?;
    let after = service_manager_source(&source)?;
    if after.as_bytes() == before {
        return Ok(());
    }
    let dir = network::backup_directory(&root, store).join("integration");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let backup = dir.join(format!("service-{}.bat", network::hash(&before)));
    if !backup.exists() {
        network::atomic_write(&backup, &before)?;
    }
    if fs::read(&path).map_err(|e| e.to_string())? != before {
        return Err("service.bat изменён во время подготовки. Файл сохранён без изменений.".into());
    }
    network::atomic_write(&path, after.as_bytes())
}
fn service_manager_source(source: &str) -> Result<String, String> {
    if source.contains(":: RUSTLM LIVE STRATEGY") {
        return Ok(source.into());
    }
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let start = source
        .find(&format!("{newline}:get_strategy_name{newline}"))
        .ok_or("Формат меню service.bat не распознан")?;
    let end = start
        + source[start..]
            .find(&format!("{newline}exit /b"))
            .ok_or("Конец функции меню service.bat не найден")?;
    let block = r#":: RUSTLM LIVE STRATEGY
set "RUSTLM_ZAPRET_ROOT=%~dp0"
for /f "usebackq delims=" %%S in (`powershell.exe -NoProfile -NonInteractive -Command "$ErrorActionPreference='SilentlyContinue';$x=Get-ItemProperty 'HKCU:\Software\RustLM\Zapret';if($x.Root -eq $env:RUSTLM_ZAPRET_ROOT.TrimEnd('\')){$p=Get-CimInstance Win32_Process -Filter ('ProcessId='+$x.Pid);if($p.Name -eq 'winws.exe' -and $p.CreationDate -and $p.CreationDate.ToUniversalTime().Ticks.ToString() -eq $x.Created){$x.Strategy}}" 2^>nul`) do set "CurrentStrategy=Strategy: %%S (RustLM)"
set "RUSTLM_ZAPRET_ROOT=""#;
    Ok(format!(
        "{}{newline}{}{}",
        &source[..end],
        block.replace('\n', newline),
        &source[end..]
    ))
}
fn powershell(script: &str) -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script = format!(
            "$ProgressPreference='SilentlyContinue';$OutputEncoding=[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);try {{{script}}} catch {{[Console]::Error.WriteLine($_.Exception.Message);exit 1}}"
        );
        let encoded = STANDARD.encode(
            script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let mut child = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-OutputFormat",
                "Text",
                "-EncodedCommand",
                &encoded,
            ])
            .creation_flags(0x08000000)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        let start = Instant::now();
        let stdout = child.stdout.take().ok_or("Нет stdout")?;
        let stderr = child.stderr.take().ok_or("Нет stderr")?;
        let out_reader = std::thread::spawn(move || {
            let mut data = Vec::new();
            stdout
                .take(2 * 1024 * 1024)
                .read_to_end(&mut data)
                .map(|_| data)
        });
        let err_reader = std::thread::spawn(move || {
            let mut data = Vec::new();
            stderr
                .take(2 * 1024 * 1024)
                .read_to_end(&mut data)
                .map(|_| data)
        });
        loop {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                break;
            }
            if start.elapsed() > Duration::from_secs(90) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    "POWERSHELL_TIMEOUT: команда Windows не завершилась за 90 секунд".into(),
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        let output = out_reader
            .join()
            .map_err(|_| "Ошибка чтения stdout")?
            .map_err(|e| e.to_string())?;
        let errors = err_reader
            .join()
            .map_err(|_| "Ошибка чтения stderr")?
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(String::from_utf8_lossy(&errors)
                .chars()
                .take(2000)
                .collect());
        }
        Ok(String::from_utf8_lossy(&output)
            .trim()
            .trim_start_matches('\u{feff}')
            .into())
    }
    #[cfg(not(windows))]
    {
        let _ = script;
        Err("Repair Tools доступны в Windows".into())
    }
}
#[derive(Serialize)]
pub struct ActiveZapret {
    pub installation: Installation,
    pub strategy: Option<String>,
    pub fingerprint: String,
}
#[derive(Serialize)]
pub struct Readiness {
    pub running: bool,
    pub strategy_matches: bool,
    pub observed_strategy: Option<String>,
    pub vpn_signals: Vec<String>,
    pub details_unavailable: bool,
}
impl Readiness {
    pub fn ensure_ready(&self) -> Result<(), String> {
        if !self.vpn_signals.is_empty() {
            return Err(format!("VPN_DETECTED: {}", self.vpn_signals.join("; ")));
        }
        if !self.running {
            return Err("ZAPRET_NOT_RUNNING".into());
        }
        if !self.strategy_matches {
            if self.details_unavailable {
                return Err("ZAPRET_DETAILS_UNAVAILABLE: включите выбранную стратегию через RustLM для подтверждения параметров".into());
            }
            return Err(format!(
                "STRATEGY_MISMATCH: {}",
                self.observed_strategy
                    .as_deref()
                    .unwrap_or("параметры не распознаны")
            ));
        }
        Ok(())
    }
}
pub fn readiness(root: &str, strategy: &str) -> Result<Readiness, String> {
    let json = powershell(
        r#"$ErrorActionPreference='Stop';$signals=@();$vpn='(?i)wireguard|wintun|openvpn|\btap\b|tailscale|zerotier|amnezia|protonvpn|nordvpn|outline|fortinet|anyconnect|windscribe';Get-NetAdapter | Where-Object {$_.Status -eq 'Up' -and ($_.Name -match $vpn -or $_.InterfaceDescription -match $vpn)} | ForEach-Object {$signals+=('Сетевой адаптер: '+$_.Name)};Get-VpnConnection -ErrorAction SilentlyContinue | Where-Object {$_.ConnectionStatus -eq 'Connected'} | ForEach-Object {$signals+=('VPN Windows: '+$_.Name)};$proxy=Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings' -ErrorAction SilentlyContinue;if($proxy.ProxyEnable -eq 1){$signals+='Включён системный прокси'};if($proxy.AutoConfigURL){$signals+='Настроен автоматический прокси (PAC)'};ConvertTo-Json -Compress -InputObject @($signals)"#,
    )?;
    let vpn_signals =
        serde_json::from_str(&json).map_err(|e| format!("Не удалось проверить VPN: {e}"))?;
    let processes = winws_processes()?;
    let running = !processes.is_empty();
    let detected = detect_from(&processes);
    let details_unavailable = detected
        .as_ref()
        .err()
        .is_some_and(|e| e.starts_with("ZAPRET_DETAILS_UNAVAILABLE"));
    let active = match detected {
        Ok(active) => active,
        Err(_) if details_unavailable => None,
        Err(error) => return Err(error),
    };
    let expected = if root.is_empty() || strategy.is_empty() {
        None
    } else {
        Some(network::hash(
            normalize(&arguments(root, strategy)?.join(" ")).as_bytes(),
        ))
    };
    Ok(Readiness {
        running,
        strategy_matches: active
            .as_ref()
            .is_some_and(|found| expected.as_ref() == Some(&found.fingerprint)),
        observed_strategy: active.and_then(|found| found.strategy),
        vpn_signals,
        details_unavailable,
    })
}
pub fn network_identity() -> Result<String, String> {
    let value=powershell("$ErrorActionPreference='Stop'; ConvertTo-Json -Compress -InputObject @(Get-NetIPConfiguration | Sort-Object InterfaceIndex | Select-Object InterfaceIndex,InterfaceAlias,@{Name='Gateway';Expression={$_.IPv4DefaultGateway.NextHop}},@{Name='DNS';Expression={$_.DNSServer.ServerAddresses}})")?;
    Ok(network::hash(value.as_bytes()))
}
pub fn detect() -> Result<Option<ActiveZapret>, String> {
    detect_from(&winws_processes()?)
}
fn winws_processes() -> Result<Vec<serde_json::Value>, String> {
    let json=powershell("$ErrorActionPreference='Stop'; ConvertTo-Json -Compress -InputObject @(Get-CimInstance Win32_Process -Filter \"name='winws.exe'\" | Select-Object ProcessId,ExecutablePath,CommandLine,@{Name='Created';Expression={if($_.CreationDate){$_.CreationDate.ToUniversalTime().Ticks.ToString()}else{''}}})")?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}
#[derive(Serialize, Deserialize)]
struct ActiveReceipt {
    pid: u32,
    created: String,
    root: String,
    strategy: String,
    fingerprint: String,
}
fn receipt_matches(receipt: &ActiveReceipt, process: &serde_json::Value) -> bool {
    !receipt.created.is_empty()
        && process["ProcessId"].as_u64() == Some(receipt.pid as u64)
        && process["Created"].as_str() == Some(receipt.created.as_str())
}
fn detect_from(processes: &[serde_json::Value]) -> Result<Option<ActiveZapret>, String> {
    if processes.is_empty() {
        return Ok(None);
    }
    if processes.len() != 1 {
        return Err(
            "Найдено несколько winws. Оставьте один Zapret и повторите определение.".into(),
        );
    }
    let exe = match processes[0]["ExecutablePath"]
        .as_str()
        .filter(|s| !s.is_empty())
    {
        Some(exe) => exe,
        None => {
            let receipt =
                fs::read_to_string(super::repair_paths::root()?.join("active-zapret.json"))
                    .ok()
                    .and_then(|b| {
                        serde_json::from_str::<ActiveReceipt>(b.trim_start_matches('\u{feff}')).ok()
                    });
            if let Some(receipt) = receipt.filter(|r| receipt_matches(r, &processes[0])) {
                return Ok(Some(ActiveZapret {
                    installation: network::installation(&receipt.root)?,
                    strategy: Some(receipt.strategy),
                    fingerprint: receipt.fingerprint,
                }));
            }
            return Err("ZAPRET_DETAILS_UNAVAILABLE: Zapret запущен с повышенными правами; параметры недоступны обычному процессу".into());
        }
    };
    let root = Path::new(exe)
        .parent()
        .and_then(Path::parent)
        .ok_or("Папка winws не определена")?;
    let info = network::installation(&root.to_string_lossy())?;
    let active = processes[0]["CommandLine"].as_str().unwrap_or("");
    let active = active
        .find("--wf-")
        .map(|i| normalize(&active[i..]))
        .unwrap_or_default();
    let strategy = info
        .strategies
        .iter()
        .find(|s| {
            arguments(&info.root, s)
                .map(|args| normalize(&args.join(" ")) == active)
                .unwrap_or(false)
        })
        .cloned();
    Ok(Some(ActiveZapret {
        installation: info,
        strategy,
        fingerprint: network::hash(active.as_bytes()),
    }))
}
fn normalize(value: &str) -> String {
    value
        .replace("\\\\?\\", "")
        .replace('"', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
fn split_arguments(value: &str) -> Result<Vec<String>, String> {
    let mut result = vec![];
    let mut current = String::new();
    let mut quoted = false;
    for c in value.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if c.is_whitespace() && !quoted {
            if !current.is_empty() {
                result.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if quoted {
        return Err("Незакрытая кавычка в BAT".into());
    }
    if !current.is_empty() {
        result.push(current);
    }
    if result.is_empty()
        || result
            .iter()
            .any(|s| !s.starts_with("--") || s.contains('%') || s.contains('\0'))
    {
        return Err("Команда winws не распознана. Сторонние команды не выполнялись.".into());
    }
    Ok(result)
}
fn arguments(root: &str, strategy: &str) -> Result<Vec<String>, String> {
    let info = network::installation(root)?;
    if !info.strategies.iter().any(|s| s == strategy) {
        return Err("BAT не найден".into());
    }
    let file = fs::canonicalize(Path::new(&info.root).join(strategy)).map_err(|e| e.to_string())?;
    if !file.starts_with(&info.root) {
        return Err("BAT вне выбранной папки".into());
    }
    let source = fs::read_to_string(file).map_err(|e| e.to_string())?;
    let start = source.find("--wf-").ok_or("Не найдены параметры winws")?;
    let root = info.root.trim_start_matches("\\\\?\\");
    let source = source[start..]
        .replace("^\r\n", " ")
        .replace("^\n", " ")
        .replace("%BIN%", &format!("{root}\\bin\\"))
        .replace("%LISTS%", &format!("{root}\\lists\\"));
    let game = Path::new(root).join("utils/game_filter.enabled");
    let mode = if game.exists() {
        fs::read_to_string(game).unwrap_or_default()
    } else {
        String::new()
    };
    let values: BTreeMap<_, _> = mode
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();
    let mode =
        values
            .get("mode")
            .copied()
            .unwrap_or(if mode.is_empty() { "disabled" } else { "all" });
    let tcp = if ["all", "tcp"].contains(&mode) {
        values.get("tcp").copied().unwrap_or("1024-65535")
    } else {
        "12"
    };
    let udp = if ["all", "udp"].contains(&mode) {
        values.get("udp").copied().unwrap_or("1024-65535")
    } else {
        "12"
    };
    split_arguments(
        &source
            .replace("%GameFilterTCP%", tcp)
            .replace("%GameFilterUDP%", udp),
    )
}
pub fn restart(root: &str, strategy: &str, logs: &Path) -> Result<String, String> {
    let info = network::installation(root)?;
    let args = arguments(&info.root, strategy)?;
    let fingerprint = network::hash(normalize(&args.join(" ")).as_bytes());
    let receipt_path = super::repair_paths::root()?.join("active-zapret.json");
    let logs = logs.join(format!(
        "zapret-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%3f")
    ));
    fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    let exe =
        fs::canonicalize(Path::new(&info.root).join("bin/winws.exe")).map_err(|e| e.to_string())?;
    if !exe.starts_with(&info.root) {
        return Err("winws вне выбранной папки".into());
    }
    let exe = exe
        .to_string_lossy()
        .trim_start_matches("\\\\?\\")
        .to_owned();
    let args = args
        .iter()
        .map(|a| format!("\"{a}\""))
        .collect::<Vec<_>>()
        .join(" ");
    let output = logs.join("winws.stdout.log");
    let error = logs.join("winws.stderr.log");
    let dry_output = logs.join("validation.stdout.log");
    let dry_error = logs.join("validation.stderr.log");
    let helper_error = logs.join("helper.error.log");
    let script = format!(
        r#"
$ErrorActionPreference='Stop'
$stopped=$false
try{{
$exe={exe}
$running=@(Get-CimInstance Win32_Process -Filter "name='winws.exe'" | Where-Object {{$_.ExecutablePath -eq $exe}})
$ids=@($running | ForEach-Object ProcessId)
$service=@(Get-CimInstance Win32_Service | Where-Object {{$ids -contains $_.ProcessId -and $_.ProcessId -ne 0}})
if($service.Count){{throw 'SERVICE_MODE: Zapret работает как служба. Автоперезапуск службы пока не поддерживается.'}}
$dry=Start-Process -FilePath $exe -ArgumentList ({args}+' --dry-run') -WorkingDirectory {cwd} -RedirectStandardOutput {dry_out} -RedirectStandardError {dry_err} -WindowStyle Hidden -PassThru
$dryHandle=$dry.Handle
if(-not $dry.WaitForExit(15000)){{$dry.Kill();throw 'VALIDATION_FAILED: проверка параметров превысила 15 секунд'}}
$dry.Refresh()
$validation=(Get-Content -LiteralPath {dry_out},{dry_err} -Raw -ErrorAction SilentlyContinue | Out-String)
if($dry.ExitCode -ne 0 -and $validation -notmatch '(?m)^command line parameters verified\s*$' -and $validation -notmatch 'already running with the same filter'){{throw ('VALIDATION_FAILED: '+$validation)}}
if($script:RustLMCancelPath -and (Test-Path -LiteralPath $script:RustLMCancelPath)){{throw 'OPERATION_CANCELLED'}}
$stopped=$true
foreach($p in $running){{Stop-Process -Id $p.ProcessId -Force -ErrorAction Stop}}
Start-Sleep -Milliseconds 500
if($script:RustLMCancelPath -and (Test-Path -LiteralPath $script:RustLMCancelPath)){{throw 'OPERATION_CANCELLED'}}
$p=Start-Process -FilePath $exe -ArgumentList {args} -WorkingDirectory {cwd} -RedirectStandardOutput {out} -RedirectStandardError {err} -WindowStyle Hidden -PassThru
Start-Sleep -Seconds 2
if($p.HasExited){{throw ('winws завершился: '+(Get-Content -LiteralPath {err} -Tail 8 -ErrorAction SilentlyContinue | Out-String))}}
$actual=Get-CimInstance Win32_Process -Filter ('ProcessId='+$p.Id)
[pscustomobject]@{{pid=$p.Id;created=$actual.CreationDate.ToUniversalTime().Ticks.ToString();root={root};strategy={strategy};fingerprint={fingerprint}}}|ConvertTo-Json -Compress|Set-Content -LiteralPath {receipt} -Encoding UTF8
$key='HKCU:\Software\RustLM\Zapret'
New-Item -Path $key -Force|Out-Null
Set-ItemProperty -Path $key -Name Root -Value {manager_root}
Set-ItemProperty -Path $key -Name Pid -Value $p.Id
Set-ItemProperty -Path $key -Name Created -Value $actual.CreationDate.ToUniversalTime().Ticks.ToString()
Set-ItemProperty -Path $key -Name Strategy -Value {manager_strategy}
'winws запущен; журнал записывается в файл'
}}catch{{$prefix=if($stopped){{'RESTART_FAILED_AFTER_STOP: '}}else{{'RESTART_NOT_STARTED: '}};($prefix+($_|Out-String))|Set-Content -LiteralPath {helper_error} -Encoding UTF8;exit 1}}
"#,
        exe = literal(&exe),
        args = literal(&args),
        cwd = literal(info.root.trim_start_matches("\\\\?\\")),
        out = literal(&output.to_string_lossy()),
        err = literal(&error.to_string_lossy()),
        dry_out = literal(&dry_output.to_string_lossy()),
        dry_err = literal(&dry_error.to_string_lossy()),
        helper_error = literal(&helper_error.to_string_lossy()),
        root = literal(&info.root),
        strategy = literal(strategy),
        fingerprint = literal(&fingerprint),
        receipt = literal(&receipt_path.to_string_lossy()),
        manager_root = literal(info.root.trim_start_matches("\\\\?\\")),
        manager_strategy = literal(
            Path::new(strategy)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref()
        )
    );
    let result = elevated(&script).map_err(|error| {
        format!(
            "{error}. {}",
            fs::read_to_string(helper_error)
                .unwrap_or_else(
                    |_| "RESTART_STATE_UNKNOWN: помощник не записал результат операции".into()
                )
                .trim_start_matches('\u{feff}')
                .to_owned()
        ) + &format!("\nЖурналы: {}", logs.display())
    })?;
    match integrate_service_manager(&info.root, &super::repair_paths::root()?.join("backups")) {
        Ok(()) => Ok(result),
        Err(warning) => Ok(format!(
            "Zapret включён. Не удалось обновить отображение service.bat: {warning}"
        )),
    }
}
pub fn stop(root: &str) -> Result<(), String> {
    let info = network::installation(root)?;
    let exe =
        fs::canonicalize(Path::new(&info.root).join("bin/winws.exe")).map_err(|e| e.to_string())?;
    if !exe.starts_with(&info.root) {
        return Err("winws вне выбранной папки".into());
    }
    let logs = super::repair_paths::root()?.join("logs");
    fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    let error = logs.join(format!("stop-{}.log", rand::random::<u64>()));
    let script=format!("$ErrorActionPreference='Stop';try {{$p=@(Get-CimInstance Win32_Process -Filter \"name='winws.exe'\"|Where-Object {{$_.ExecutablePath -eq {exe}}});$ids=@($p|ForEach-Object ProcessId);if(@(Get-CimInstance Win32_Service|Where-Object {{$ids -contains $_.ProcessId -and $_.ProcessId -ne 0}}).Count){{throw 'SERVICE_MODE'}};foreach($item in $p){{Stop-Process -Id $item.ProcessId -Force -ErrorAction Stop}}}}catch{{$_.Exception.Message|Set-Content -LiteralPath {error} -Encoding UTF8;exit 1}}",exe=literal(exe.to_string_lossy().trim_start_matches("\\\\?\\")),error=literal(&error.to_string_lossy()));
    elevated(&script)
        .map_err(|e| format!("{e}\n{}", fs::read_to_string(&error).unwrap_or_default()))?;
    Ok(())
}
pub fn vpn_application() -> Result<Option<String>, String> {
    let value=powershell("$names=@('AmneziaVPN','ProtonVPN','v2rayN','v2rayTun','Hiddify','nekoray','nekobox','Clash Verge','Outline','WireGuard','NordVPN','Windscribe','warp');$p=Get-Process -ErrorAction SilentlyContinue|Where-Object {$names -contains $_.ProcessName -and $_.Path}|Sort-Object @{Expression={$_.MainWindowHandle -ne 0};Descending=$true}|Select-Object -First 1;if($p){$p.Path}")?;
    Ok(if value.trim().is_empty() {
        None
    } else {
        Some(value.trim().into())
    })
}

#[derive(Clone, Serialize)]
pub struct Evidence {
    pub source: String,
    pub category: String,
    pub count: usize,
}
#[derive(Serialize)]
pub struct EvidenceReport {
    pub created_at: String,
    pub errors: Vec<Evidence>,
    pub unavailable: Vec<String>,
    pub path: String,
}
#[derive(Serialize, Deserialize)]
pub struct Journal {
    pub id: String,
    pub revision: u64,
    pub step: u8,
    pub provider: String,
    pub region: String,
    pub strategy: String,
    pub events: Vec<String>,
    pub report_ids: Vec<String>,
    pub client_status: Option<String>,
    pub outcome: Option<String>,
    pub operation_failed: bool,
}
static JOURNAL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub fn save_journal(root: &Path, journal: Journal) -> Result<(), String> {
    let _guard = JOURNAL_LOCK.lock().map_err(|e| e.to_string())?;
    if journal.id.is_empty()
        || journal.id.len() > 80
        || !journal
            .id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        || !(1..=6).contains(&journal.step)
        || journal.provider.chars().count() > 160
        || journal.strategy.chars().count() > 255
        || journal.events.len() > 500
        || journal.events.iter().any(|s| s.len() > 8000)
        || journal.report_ids.len() > 20
    {
        return Err("Некорректный журнал мастера".into());
    }
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let path = root.join(format!("{}.json", journal.id));
    if let Ok(bytes) = fs::read(&path) {
        if serde_json::from_slice::<Journal>(&bytes)
            .is_ok_and(|old| old.revision >= journal.revision)
        {
            return Ok(());
        }
    }
    super::network::atomic_write(
        &path,
        &serde_json::to_vec_pretty(&journal).map_err(|e| e.to_string())?,
    )
}
fn categories(text: &str) -> Vec<(&'static str, usize)> {
    [
        (
            "DNS",
            r"(?i)could not resolve|dns.*(?:fail|error)|host not found",
        ),
        (
            "TLS",
            r"(?i)tls.*(?:fail|error)|ssl.*(?:fail|error)|certificate.*(?:fail|invalid|expired)",
        ),
        ("TIMEOUT", r"(?i)timed?\s*out|timeout"),
        ("HTTP_403", r"(?i)\b403\b|sorry, you have been blocked"),
        (
            "CONNECTION_RESET",
            r"(?i)connection.*(?:reset|refused)|network.*unreachable",
        ),
        (
            "WINWS",
            r"(?i)winws.*(?:fail|error)|windivert.*(?:fail|error)|access.*denied",
        ),
    ]
    .into_iter()
    .filter_map(|(name, pattern)| {
        let count = regex::Regex::new(pattern).ok()?.find_iter(text).count();
        (count > 0).then_some((name, count))
    })
    .collect()
}
fn scan_logs(root: &Path, depth: usize, files: &mut Vec<std::path::PathBuf>) {
    if depth == 0 || files.len() >= 80 {
        return;
    }
    if let Ok(entries) = fs::read_dir(root) {
        let mut entries: Vec<_> = entries.flatten().collect();
        entries
            .sort_by_key(|item| std::cmp::Reverse(item.metadata().and_then(|m| m.modified()).ok()));
        for item in entries {
            if files.len() >= 80 {
                break;
            }
            let path = item.path();
            if item.file_type().map(|t| t.is_symlink()).unwrap_or(true) {
                continue;
            }
            if path.is_dir() {
                scan_logs(&path, depth - 1, files);
            } else if path.extension().is_some_and(|e| e == "log" || e == "txt")
                && item
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|time| time.elapsed().ok())
                    .is_some_and(|age| age < Duration::from_secs(1800))
            {
                files.push(path);
            }
        }
    }
}
pub fn collect(logs: &Path) -> Result<EvidenceReport, String> {
    let mut errors = vec![];
    let mut unavailable = vec![];
    let mut files = vec![];
    scan_logs(logs, 2, &mut files);
    if let Some(local) = dirs::data_local_dir() {
        let app_log = local.join("LolManager/debug.log");
        if app_log.is_file() {
            files.push(app_log);
        }
        scan_logs(&local.join("Riot Games"), 4, &mut files);
    }
    if let Some(game) = super::riot_client::RiotClientService::find_lol_install_dir() {
        scan_logs(&game.join("Logs"), 4, &mut files);
    }
    for root in [
        r"C:\Riot Games\League of Legends\Logs",
        r"D:\Riot Games\League of Legends\Logs",
    ] {
        scan_logs(Path::new(root), 4, &mut files);
    }
    files.sort();
    files.dedup();
    if files.is_empty() {
        unavailable.push("Логи Riot/League в стандартных папках не найдены".into());
    }
    for path in files {
        let mut file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => {
                unavailable.push(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into(),
                );
                continue;
            }
        };
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        let _ = file.seek(SeekFrom::Start(size.saturating_sub(256 * 1024)));
        let mut bytes = vec![];
        if file.take(256 * 1024).read_to_end(&mut bytes).is_err() {
            continue;
        }
        // Store derived categories only: raw client logs can contain account IDs and tokens.
        for (category, count) in categories(&String::from_utf8_lossy(&bytes)) {
            errors.push(Evidence {
                source: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                category: category.into(),
                count,
            });
        }
    }
    let windows = powershell("$ErrorActionPreference='Stop'; ConvertTo-Json -Compress -InputObject @(Get-WinEvent -FilterHashtable @{LogName='System';StartTime=(Get-Date).AddMinutes(-15)} -MaxEvents 200 -ErrorAction Stop | Where-Object {$_.ProviderName -match 'Schannel|DNS|Tcpip'} | Group-Object ProviderName,Id | ForEach-Object {[pscustomobject]@{source='Windows System';category=$_.Name;count=$_.Count}})");
    match windows {
        Ok(json) => match serde_json::from_str::<Vec<serde_json::Value>>(&json) {
            Ok(items) => {
                for item in items {
                    errors.push(Evidence {
                        source: "Windows System".into(),
                        category: item["category"].as_str().unwrap_or("System event").into(),
                        count: item["count"].as_u64().unwrap_or(0) as usize,
                    });
                }
            }
            Err(_) => {
                unavailable.push("Нет подходящих событий Windows или ответ журнала пуст".into())
            }
        },
        Err(_) => unavailable.push("Журнал Windows недоступен".into()),
    }
    fs::create_dir_all(logs).map_err(|e| e.to_string())?;
    let path = logs.join(format!(
        "evidence-{}.json",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%3f")
    ));
    let report = EvidenceReport {
        created_at: chrono::Utc::now().to_rfc3339(),
        errors,
        unavailable,
        path: path.to_string_lossy().into(),
    };
    fs::write(
        path,
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn helper_finishes_while_its_long_running_child_remains_alive() {
        fn encoded(script: &str) -> String {
            STANDARD.encode(
                script
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
        }
        let dir =
            std::env::temp_dir().join(format!("rustlm-helper-test-{}", rand::random::<u64>()));
        fs::create_dir(&dir).unwrap();
        let child = encoded("Start-Sleep -Seconds 30");
        let parent=format!("$c=Start-Process powershell.exe -WindowStyle Hidden -ArgumentList @('-NoProfile','-NonInteractive','-EncodedCommand',{}) -RedirectStandardOutput {} -RedirectStandardError {} -PassThru;[pscustomobject]@{{pid=$c.Id;created=$c.StartTime.ToUniversalTime().Ticks.ToString()}}|ConvertTo-Json -Compress|Set-Content -LiteralPath {} -Encoding UTF8",literal(&child),literal(&dir.join("child.out").to_string_lossy()),literal(&dir.join("child.err").to_string_lossy()),literal(&dir.join("child.json").to_string_lossy()));
        let started = Instant::now();
        let result = powershell(&helper_command(&encoded(&parent), false));
        let elapsed = started.elapsed();
        let text = fs::read_to_string(dir.join("child.json")).unwrap();
        let record: serde_json::Value =
            serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap();
        let cleanup=powershell(&format!("$p=Get-Process -Id {} -ErrorAction SilentlyContinue;if($p){{Stop-Process -Id $p.Id -Force;Wait-Process -Id $p.Id -Timeout 10;'alive'}}else{{'gone'}}",record["pid"].as_u64().unwrap())).unwrap();
        assert_eq!(result.unwrap(), "OK");
        assert_eq!(
            cleanup, "alive",
            "The helper must not wait for its child to exit"
        );
        assert!(elapsed < Duration::from_secs(15), "Helper took {elapsed:?}");
        // Windows can keep redirected output locked briefly after the child exits.
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn service_manager_integration_is_backed_up_and_idempotent() {
        let dir =
            std::env::temp_dir().join(format!("rustlm-service-test-{}", rand::random::<u64>()));
        let root = dir.join("zapret");
        let store = dir.join("data");
        fs::create_dir_all(&root).unwrap();
        let before="@echo off\r\n:menu\r\necho menu\r\n:get_strategy_name\r\nset \"CurrentStrategy=\"\r\nrem existing HKLM service lookup\r\nexit /b\r\n:next\r\necho untouched\r\n";
        fs::write(root.join("service.bat"), before).unwrap();
        integrate_service_manager(root.to_str().unwrap(), &store).unwrap();
        let after = fs::read_to_string(root.join("service.bat")).unwrap();
        assert!(after.contains("rem existing HKLM service lookup"));
        assert!(after.contains("CreationDate.ToUniversalTime().Ticks"));
        assert!(after.ends_with(":next\r\necho untouched\r\n"));
        integrate_service_manager(root.to_str().unwrap(), &store).unwrap();
        assert_eq!(fs::read_to_string(root.join("service.bat")).unwrap(), after);
        let backup = network::backup_directory(&fs::canonicalize(&root).unwrap(), &store)
            .join("integration")
            .join(format!("service-{}.bat", network::hash(before.as_bytes())));
        assert_eq!(fs::read(backup).unwrap(), before.as_bytes());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn elevated_launch_receipt_does_not_match_a_reused_process_id() {
        let receipt = ActiveReceipt {
            pid: 42,
            created: "100".into(),
            root: "test".into(),
            strategy: "test".into(),
            fingerprint: "hash".into(),
        };
        assert!(receipt_matches(
            &receipt,
            &serde_json::json!({"ProcessId":42,"Created":"100"})
        ));
        assert!(!receipt_matches(
            &receipt,
            &serde_json::json!({"ProcessId":42,"Created":"200"})
        ));
        assert!(!receipt_matches(
            &receipt,
            &serde_json::json!({"ProcessId":43,"Created":"100"})
        ));
    }
    #[test]
    fn preflight_blocks_vpn_stopped_zapret_and_wrong_strategy() {
        let mut state = Readiness {
            details_unavailable: false,
            running: false,
            strategy_matches: false,
            observed_strategy: None,
            vpn_signals: vec!["VPN adapter".into()],
        };
        assert!(state
            .ensure_ready()
            .unwrap_err()
            .starts_with("VPN_DETECTED"));
        state.vpn_signals.clear();
        assert_eq!(state.ensure_ready().unwrap_err(), "ZAPRET_NOT_RUNNING");
        state.running = true;
        assert!(state
            .ensure_ready()
            .unwrap_err()
            .starts_with("STRATEGY_MISMATCH"));
        state.strategy_matches = true;
        assert!(state.ensure_ready().is_ok());
    }
    #[cfg(windows)]
    #[test]
    fn powershell_errors_are_plain_text_without_clixml_or_script_source() {
        let error = powershell("throw 'VALIDATION_FAILED: readable reason'").unwrap_err();
        assert_eq!(error.trim(), "VALIDATION_FAILED: readable reason");
        assert!(!error.contains("CLIXML"));
        assert!(!error.contains("At line:"));
    }
    #[test]
    fn journal_keeps_latest_revision_and_rejects_path_traversal() {
        let root = std::env::temp_dir().join(format!("rustlm-journal-{}", rand::random::<u64>()));
        let mut journal = Journal {
            id: "test-session".into(),
            revision: 2,
            step: 3,
            provider: "current".into(),
            region: "RU".into(),
            strategy: "ALT11.bat".into(),
            events: vec!["DNS completed".into()],
            report_ids: vec![],
            client_status: None,
            outcome: None,
            operation_failed: false,
        };
        save_journal(&root, journal).unwrap();
        journal =
            serde_json::from_slice(&fs::read(root.join("test-session.json")).unwrap()).unwrap();
        journal.revision = 1;
        journal.events.clear();
        save_journal(&root, journal).unwrap();
        let saved: Journal =
            serde_json::from_slice(&fs::read(root.join("test-session.json")).unwrap()).unwrap();
        assert_eq!(saved.revision, 2);
        assert_eq!(saved.events, vec!["DNS completed"]);
        let mut invalid = saved;
        invalid.id = "../outside".into();
        assert!(save_journal(&root, invalid).is_err());
        fs::remove_file(root.join("test-session.json")).unwrap();
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn splits_only_winws_arguments_without_executing_batch_commands() {
        assert_eq!(
            split_arguments("--wf-tcp=443 --hostlist=\"C:\\my folder\\list.txt\"").unwrap(),
            vec!["--wf-tcp=443", "--hostlist=C:\\my folder\\list.txt"]
        );
        assert!(split_arguments("--wf-tcp=443 & del x").is_err());
        assert!(split_arguments("--hostlist=\"unclosed").is_err());
    }
    #[test]
    fn evidence_does_not_copy_tokens_or_user_data() {
        let text =
            "Authorization: Bearer secret-jwt account=private-user TLS handshake failed HTTP 403";
        let result = categories(text);
        assert!(result.iter().any(|(name, _)| *name == "TLS"));
        assert!(!format!("{result:?}").contains("secret-jwt"));
        assert!(!format!("{result:?}").contains("private-user"));
    }
    #[test]
    fn quotes_paths_as_powershell_literals() {
        assert_eq!(literal("D:\\O'Brien\\zapret"), "'D:\\O''Brien\\zapret'");
    }
    #[cfg(windows)]
    #[test]
    fn powershell_preserves_utf8_arrays_and_drains_large_output() {
        let output=powershell("ConvertTo-Json -Compress -InputObject @([pscustomobject]@{value='проверка';body=('x'*100000)})").unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["value"], "проверка");
        assert_eq!(parsed[0]["body"].as_str().unwrap().len(), 100000);
    }
}
