#![allow(dead_code)]

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x00000008;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use crate::error::AppError;
use crate::models::league_client::{ClientConnectivityStatus, LockfileInfo};

fn identity_login(value: &serde_json::Value) -> Result<Option<String>, String> {
    let decoded = match value.get("userInfo").or_else(|| value.get("userinfo")) {
        Some(serde_json::Value::String(raw)) => Some(
            serde_json::from_str::<serde_json::Value>(raw)
                .map_err(|_| "Malformed identity response")?,
        ),
        Some(v) if v.is_object() => Some(v.clone()),
        _ => None,
    };
    let mut found: Option<String> = None;
    for root in std::iter::once(value).chain(decoded.as_ref()) {
        for node in std::iter::once(root).chain(
            [
                "authorization",
                "session",
                "sessionCredentials",
                "credentials",
                "account",
                "user",
                "login",
            ]
            .iter()
            .filter_map(|key| root.get(*key)),
        ) {
            for key in ["username", "loginUsername", "accountName", "userName"] {
                if let Some(raw) = node.get(key).and_then(serde_json::Value::as_str) {
                    let login = raw.trim();
                    if login.is_empty() {
                        continue;
                    }
                    if login.len() > 320
                        || found
                            .as_ref()
                            .is_some_and(|old| !old.eq_ignore_ascii_case(login))
                    {
                        return Err("Identity responses disagree or are invalid".into());
                    }
                    found = Some(login.to_owned());
                }
            }
        }
    }
    Ok(found)
}

fn authorization_state(value: &serde_json::Value) -> Option<bool> {
    let object = value.as_object()?;
    if ["errorCode", "error"].iter().any(|key| {
        object
            .get(*key)
            .is_some_and(|v| !v.is_null() && !v.as_str().is_some_and(|s| s.trim().is_empty()))
    }) {
        return None;
    }
    let mut signals = Vec::new();
    for key in ["isAuthorized", "authorized"] {
        if let Some(b) = value[key].as_bool() {
            signals.push(b);
        }
    }
    if value["subject"].as_str().is_some_and(|s| !s.is_empty())
        || value["currentAccountId"].as_u64().is_some_and(|n| n > 0)
        || value["accessToken"].as_str().is_some_and(|s| !s.is_empty())
        || value["authorization"]["accessToken"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    {
        signals.push(true);
    }
    match value["type"].as_str() {
        Some("authenticated") => signals.push(true),
        Some("needs_credentials" | "needs_password" | "needs_multifactor_verification") => {
            signals.push(false)
        }
        _ => {}
    }
    let first = *signals.first()?;
    signals.iter().all(|s| *s == first).then_some(first)
}

pub struct RiotClientService {
    cached_lcu_auth: Mutex<Option<(u16, String)>>,
    connection_epoch: AtomicU64,
    request_policy: super::lcu_request_policy::RequestPolicy,
    http_client: reqwest::Client,
    read_slots: tokio::sync::Semaphore,
    critical_read_slots: tokio::sync::Semaphore,
    write_slots: tokio::sync::Semaphore,
    pub recorder: super::session_recording::SessionRecorder,
    pub diagnostics: std::sync::Arc<super::client_diagnostics::ClientDiagnostics>,
}

impl RiotClientService {
    pub fn new() -> Self {
        // Create HTTP client that ignores self-signed certs (LCU uses self-signed)
        let http_client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .http1_only()
            .build()
            .unwrap_or_default();

        Self {
            cached_lcu_auth: Mutex::new(None),
            connection_epoch: AtomicU64::new(0),
            request_policy: super::lcu_request_policy::RequestPolicy::default(),
            http_client,
            read_slots: tokio::sync::Semaphore::new(8),
            critical_read_slots: tokio::sync::Semaphore::new(2),
            write_slots: tokio::sync::Semaphore::new(2),
            recorder: super::session_recording::SessionRecorder::default(),
            diagnostics: std::sync::Arc::new(
                super::client_diagnostics::ClientDiagnostics::default(),
            ),
        }
    }

    // --- Riot Client Install Detection ---

    /// RiotClientInstalls.json lives in %PROGRAMDATA%\Riot Games\, NOT %LOCALAPPDATA%
    fn find_installs_json() -> Option<PathBuf> {
        // Primary: %PROGRAMDATA% (C:\ProgramData)
        if let Ok(program_data) = env::var("PROGRAMDATA") {
            let path = PathBuf::from(&program_data)
                .join("Riot Games")
                .join("RiotClientInstalls.json");
            if path.exists() {
                return Some(path);
            }
        }
        // Fallback: %LOCALAPPDATA% (some old installs)
        if let Some(local_app_data) = dirs::data_local_dir() {
            let path = local_app_data
                .join("Riot Games")
                .join("RiotClientInstalls.json");
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    // --- Lockfile Parsing ---

    pub fn find_rc_lockfile() -> Option<LockfileInfo> {
        let local_app_data = dirs::data_local_dir()?;
        let lockfile_path = local_app_data
            .join("Riot Games")
            .join("Riot Client")
            .join("Config")
            .join("lockfile");

        Self::parse_lockfile(&lockfile_path)
    }

    pub fn find_lcu_lockfile() -> Option<LockfileInfo> {
        let candidates = Self::enumerate_lcu_lockfile_candidates();

        for path in candidates {
            if let Some(info) = Self::parse_lockfile(&path) {
                // Guard against stale lockfiles left by force-killed League (/F taskkill).
                // The file persists on disk but the process is dead — same check as RC lockfile.
                if Self::is_pid_alive(info.pid) {
                    return Some(info);
                }
            }
        }
        None
    }

    fn enumerate_lcu_lockfile_candidates() -> Vec<PathBuf> {
        let mut candidates = Vec::new();

        // Try RiotClientInstalls.json first (from %PROGRAMDATA%)
        if let Some(installs_json) = Self::find_installs_json() {
            if let Ok(content) = fs::read_to_string(&installs_json) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(lol) = parsed.get("associated_client") {
                        if let Some(obj) = lol.as_object() {
                            for (path, _) in obj {
                                let lockfile = PathBuf::from(path).join("lockfile");
                                candidates.push(lockfile);
                            }
                        }
                    }
                }
            }
        }

        // Standard install paths
        let standard_paths = [
            r"C:\Riot Games\League of Legends",
            r"D:\Riot Games\League of Legends",
            r"C:\Games\League of Legends",
            r"D:\Games\League of Legends",
        ];

        for path in &standard_paths {
            candidates.push(PathBuf::from(path).join("lockfile"));
        }

        // Try finding from running process
        #[cfg(windows)]
        {
            if let Some(path) = Self::find_league_lockfile_from_process() {
                candidates.insert(0, path);
            }
        }

        candidates
    }

    fn parse_lockfile(path: &PathBuf) -> Option<LockfileInfo> {
        // Read with shared access (file may be locked by the client)
        #[cfg(windows)]
        {
            use std::io::Read;
            use std::os::windows::fs::OpenOptionsExt;
            let file = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0x00000001 | 0x00000002 | 0x00000004) // FILE_SHARE_READ | WRITE | DELETE
                .open(path)
                .ok()?;
            let mut content = String::new();
            std::io::BufReader::new(file)
                .read_to_string(&mut content)
                .ok()?;
            Self::parse_lockfile_content(&content)
        }
        #[cfg(not(windows))]
        {
            let content = fs::read_to_string(path).ok()?;
            Self::parse_lockfile_content(&content)
        }
    }

    fn parse_lockfile_content(content: &str) -> Option<LockfileInfo> {
        let parts: Vec<&str> = content.trim().split(':').collect();
        if parts.len() < 5 {
            return None;
        }
        Some(LockfileInfo {
            name: parts[0].to_string(),
            pid: parts[1].parse().ok()?,
            port: parts[2].parse().ok()?,
            password: parts[3].to_string(),
            protocol: parts[4].to_string(),
        })
    }

    // --- Process Management ---

    #[cfg(windows)]
    fn find_league_lockfile_from_process() -> Option<PathBuf> {
        Self::find_process_pids("LeagueClientUx.exe")
            .into_iter()
            .find_map(|pid| {
                let path = Self::process_path(pid)?;
                if !path
                    .file_name()?
                    .to_string_lossy()
                    .eq_ignore_ascii_case("LeagueClientUx.exe")
                {
                    return None;
                }
                Some(path.parent()?.join("lockfile"))
            })
    }
    #[cfg(windows)]
    fn process_path(pid: u32) -> Option<PathBuf> {
        use std::os::windows::ffi::OsStringExt;
        use windows::core::PWSTR;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION,
        };
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = vec![0u16; 32768];
            let mut length = buffer.len() as u32;
            let result = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut length,
            );
            let _ = CloseHandle(handle);
            result.ok()?;
            Some(PathBuf::from(std::ffi::OsString::from_wide(
                &buffer[..length as usize],
            )))
        }
    }

    /// Check if a process is running by name using Windows ToolHelp API.
    /// Handles names with spaces (e.g. "Riot Client.exe") without shell quoting issues.
    fn is_process_running(name: &str) -> bool {
        #[cfg(windows)]
        {
            Self::find_process_pids(name).len() > 0
        }
        #[cfg(not(windows))]
        {
            let _ = name;
            false
        }
    }

    /// Find all PIDs of a process by name using CreateToolhelp32Snapshot.
    #[cfg(windows)]
    fn find_process_pids(name: &str) -> Vec<u32> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Diagnostics::ToolHelp::*;

        let mut pids = Vec::new();
        unsafe {
            let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
                Ok(h) => h,
                Err(_) => return pids,
            };

            let mut entry = PROCESSENTRY32W::default();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    let exe_name = String::from_utf16_lossy(
                        &entry.szExeFile[..entry
                            .szExeFile
                            .iter()
                            .position(|&c| c == 0)
                            .unwrap_or(entry.szExeFile.len())],
                    );
                    if exe_name.eq_ignore_ascii_case(name) {
                        pids.push(entry.th32ProcessID);
                    }
                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snapshot);
        }
        pids
    }

    /// Check if any Riot Client process is running.
    /// Uses lockfile PID verification + process name checks.
    pub fn is_riot_client_running() -> bool {
        // Check lockfile — but verify the PID is actually alive
        if let Some(lockfile) = Self::find_rc_lockfile() {
            if Self::is_pid_alive(lockfile.pid) {
                return true;
            }
            // Lockfile exists but PID is dead — stale lockfile, ignore it
        }
        Self::is_process_running("RiotClientServices.exe")
            || Self::is_process_running("Riot Client.exe")
            || Self::is_process_running("RiotClientUx.exe")
    }

    /// Check if a process with the given PID is still alive.
    fn is_pid_alive(pid: u32) -> bool {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::CloseHandle;
            use windows::Win32::System::Threading::{
                OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            };
            unsafe {
                match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                    Ok(handle) => {
                        let _ = CloseHandle(handle);
                        true
                    }
                    Err(_) => false,
                }
            }
        }
        #[cfg(not(windows))]
        {
            let _ = pid;
            false
        }
    }

    /// Check specifically if the Riot Client UI process is running.
    pub fn is_riot_client_ui_running() -> bool {
        Self::is_process_running("Riot Client.exe") || Self::is_process_running("RiotClientUx.exe")
    }

    pub fn is_riot_client_services_running() -> bool {
        Self::is_process_running("RiotClientServices.exe")
    }

    pub fn is_league_running() -> bool {
        Self::is_process_running("LeagueClientUx.exe")
    }

    pub fn unglitch_process_state() -> (bool, bool, Vec<u32>) {
        #[cfg(windows)]
        return (
            Self::is_process_running("LeagueClient.exe"),
            Self::is_league_running(),
            Self::find_process_pids("LeagueClientUxRender.exe"),
        );
        #[cfg(not(windows))]
        (false, false, vec![])
    }

    pub fn unglitch_probe() -> crate::models::unglitch::UxProbe {
        use crate::models::unglitch::UxProbe;
        #[cfg(not(windows))]
        return UxProbe::default();
        #[cfg(windows)]
        {
            use windows::core::BOOL;
            use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{
                EnumWindows, GetWindowThreadProcessId, IsHungAppWindow, IsWindowVisible,
                SendMessageTimeoutW, SMTO_ABORTIFHUNG, SMTO_ERRORONEXIT, WM_NULL,
            };
            struct WindowProbe {
                pid: u32,
                responsive: Option<bool>,
            }
            unsafe extern "system" fn inspect(hwnd: HWND, param: LPARAM) -> BOOL {
                let data = &mut *(param.0 as *mut WindowProbe);
                let mut pid = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid != data.pid || !IsWindowVisible(hwnd).as_bool() {
                    return BOOL(1);
                }
                let result = SendMessageTimeoutW(
                    hwnd,
                    WM_NULL,
                    WPARAM(0),
                    LPARAM(0),
                    SMTO_ABORTIFHUNG | SMTO_ERRORONEXIT,
                    500,
                    None,
                );
                // A denied probe is not evidence of a freeze. Require Windows' hung-window signal too.
                data.responsive = if result.0 != 0 {
                    Some(true)
                } else if IsHungAppWindow(hwnd).as_bool() {
                    Some(false)
                } else {
                    None
                };
                BOOL(0)
            }
            let backends = Self::find_process_pids("LeagueClient.exe");
            let interfaces = Self::find_process_pids("LeagueClientUx.exe");
            let backend_pid = (backends.len() == 1).then(|| backends[0]);
            let ui_pid = (interfaces.len() == 1).then(|| interfaces[0]);
            let root = backend_pid
                .and_then(Self::process_path)
                .and_then(|path| path.parent().map(|p| p.to_path_buf()));
            let same_install = root.as_ref().is_some_and(|root| {
                ui_pid
                    .and_then(Self::process_path)
                    .and_then(|path| path.parent().map(|p| p.to_path_buf()))
                    .is_some_and(|parent| {
                        parent
                            .to_string_lossy()
                            .eq_ignore_ascii_case(&root.to_string_lossy())
                    })
            });
            let session_matches = Self::find_lcu_lockfile()
                .is_some_and(|lock| Some(lock.pid) == backend_pid || Some(lock.pid) == ui_pid);
            let mut window = WindowProbe {
                pid: ui_pid.unwrap_or(0),
                responsive: None,
            };
            if ui_pid.is_some() && same_install {
                unsafe {
                    let _ = EnumWindows(
                        Some(inspect),
                        LPARAM(&mut window as *mut WindowProbe as isize),
                    );
                }
            }
            let renderer_pids = Self::find_process_pids("LeagueClientUxRender.exe");
            let renderer_count = root
                .as_ref()
                .map(|root| {
                    renderer_pids
                        .iter()
                        .copied()
                        .filter(|pid| Self::unglitch_renderer_matches(*pid, root))
                        .count()
                })
                .unwrap_or(0);
            // An unreadable or differently located renderer is unknown, not proof that it disappeared.
            let render_scope_known = renderer_pids.is_empty() || renderer_count > 0;
            UxProbe {
                backend_running: !backends.is_empty(),
                ui_running: !interfaces.is_empty(),
                backend_pid,
                ui_pid,
                renderer_count,
                window_responsive: window.responsive,
                game_running: Self::is_process_running("League of Legends.exe"),
                scope_verified: same_install && session_matches && render_scope_known,
            }
        }
    }

    pub fn unglitch_renderer_matches(pid: u32, root: &std::path::Path) -> bool {
        #[cfg(windows)]
        return Self::process_path(pid)
            .is_some_and(|path| crate::models::unglitch::renderer_in_install(&path, root));
        #[cfg(not(windows))]
        {
            let _ = (pid, root);
            false
        }
    }

    /// Return path to the League of Legends install directory (the folder that
    /// contains `lockfile`, `Config\`, etc.). Reuses the same candidate list as
    /// LCU lockfile detection — first hit wins.
    pub fn find_lol_install_dir() -> Option<PathBuf> {
        for lockfile_candidate in Self::enumerate_lcu_lockfile_candidates() {
            if let Some(dir) = lockfile_candidate.parent() {
                if dir.is_dir() {
                    return Some(dir.to_path_buf());
                }
            }
        }
        None
    }

    pub fn kill_league(include_riot_client: bool) {
        #[cfg(debug_assertions)]
        if env::var_os("RUSTLM_TEST_LCU").is_some() {
            return;
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let _ = Command::new("taskkill")
                .args(["/F", "/IM", "LeagueClientUx.exe"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
            let _ = Command::new("taskkill")
                .args(["/F", "/IM", "LeagueClient.exe"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
            if include_riot_client {
                // Kill the entire process tree for each RC process
                for name in &[
                    "Riot Client.exe",
                    "RiotClientUx.exe",
                    "RiotClientServices.exe",
                    "RiotClientCrashHandler.exe",
                    "RiotClientUxRender.exe",
                ] {
                    let _ = Command::new("taskkill")
                        .args(["/F", "/T", "/IM", name])
                        .creation_flags(CREATE_NO_WINDOW)
                        .output();
                }
            }
        }
    }

    pub fn start_riot_client() -> Result<(), AppError> {
        #[cfg(debug_assertions)]
        if env::var_os("RUSTLM_TEST_LCU").is_some() {
            return Err(AppError::Custom(
                "Реальные процессы отключены в тестовом LCU".into(),
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;

            // Try RiotClientInstalls.json from %PROGRAMDATA% (primary) or %LOCALAPPDATA% (fallback)
            if let Some(installs_json) = Self::find_installs_json() {
                let content = fs::read_to_string(&installs_json)?;
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(rc_path) = parsed
                        .get("rc_default")
                        .or_else(|| parsed.get("rc_live"))
                        .and_then(|v| v.as_str())
                    {
                        let path = PathBuf::from(rc_path);
                        if path.exists() {
                            Command::new(&path)
                                .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
                                .spawn()
                                .map_err(|e| {
                                    AppError::Custom(format!("Failed to start RC: {}", e))
                                })?;
                            return Ok(());
                        }
                    }
                }
            }

            // Fallback: try common install paths
            let fallback_paths = [
                r"C:\Riot Games\Riot Client\RiotClientServices.exe",
                r"D:\Riot Games\Riot Client\RiotClientServices.exe",
            ];
            for path_str in &fallback_paths {
                let path = PathBuf::from(path_str);
                if path.exists() {
                    Command::new(&path)
                        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
                        .spawn()
                        .map_err(|e| AppError::Custom(format!("Failed to start RC: {}", e)))?;
                    return Ok(());
                }
            }

            Err(AppError::Custom("Riot Client not found".to_string()))
        }
        #[cfg(not(windows))]
        Err(AppError::Custom(
            "Not supported on this platform".to_string(),
        ))
    }

    // --- LCU HTTP API ---

    pub fn get_lcu_auth(&self) -> Option<(u16, String)> {
        #[cfg(debug_assertions)]
        if let Some(path) = env::var_os("RUSTLM_TEST_LCU") {
            // Explicit debug override: a missing/invalid simulator must never fall back to real League.
            let data = fs::read(path).ok()?;
            let config: serde_json::Value = serde_json::from_slice(&data).ok()?;
            let port = u16::try_from(config["port"].as_u64()?)
                .ok()
                .filter(|p| *p > 0)?;
            let password = config["password"].as_str()?.to_owned();
            return (!password.is_empty()).then_some((port, password));
        }
        let generation = self.connection_generation();
        // Check cache first
        {
            let cache = self.cached_lcu_auth.lock().unwrap();
            if cache.is_some() {
                return cache.clone();
            }
        }

        // Try to find from lockfile
        if let Some(info) = Self::find_lcu_lockfile() {
            let auth = (info.port, info.password.clone());
            return self.cache_discovered_auth(generation, auth);
        }

        None
    }

    fn cache_discovered_auth(&self, generation: u64, auth: (u16, String)) -> Option<(u16, String)> {
        let mut cache = self.cached_lcu_auth.lock().unwrap();
        if self.connection_generation() == generation && cache.is_none() {
            *cache = Some(auth);
        }
        cache.clone()
    }

    pub fn invalidate_cache(&self) {
        let mut cache = self.cached_lcu_auth.lock().unwrap();
        *cache = None;
        self.connection_epoch.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn connection_generation(&self) -> u64 {
        self.connection_epoch.load(Ordering::SeqCst)
    }

    pub(crate) fn invalidate_if_generation(&self, epoch: u64) -> bool {
        let mut cache = self.cached_lcu_auth.lock().unwrap();
        if self.connection_epoch.load(Ordering::SeqCst) != epoch {
            return false;
        }
        *cache = None;
        self.connection_epoch.fetch_add(1, Ordering::SeqCst);
        true
    }

    fn invalidate_auth_if_current(&self, port: u16, password: &str, epoch: u64) {
        let mut cache = self.cached_lcu_auth.lock().unwrap();
        if self.connection_epoch.load(Ordering::SeqCst) == epoch
            && cache
                .as_ref()
                .is_some_and(|(p, token)| *p == port && token == password)
        {
            *cache = None;
            self.connection_epoch.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn make_auth_header(password: &str) -> String {
        use base64::Engine;
        let credentials = format!("riot:{}", password);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes());
        format!("Basic {}", encoded)
    }

    pub fn lcu_websocket_url(&self, port: u16) -> String {
        #[cfg(debug_assertions)]
        if env::var_os("RUSTLM_TEST_LCU").is_some() {
            return format!("ws://127.0.0.1:{}/", port);
        }
        format!("wss://127.0.0.1:{}/", port)
    }

    async fn lcu_request(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        body: Option<&str>,
    ) -> Result<String, AppError> {
        self.lcu_request_if(method, endpoint, body, || true).await
    }

    async fn lcu_request_if(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        body: Option<&str>,
        is_current: impl Fn() -> bool + Send + Sync,
    ) -> Result<String, AppError> {
        self.lcu_request_checked(method, endpoint, body, || std::future::ready(is_current()))
            .await
    }

    async fn lcu_request_checked<F: std::future::Future<Output = bool> + Send>(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        body: Option<&str>,
        is_current: impl FnOnce() -> F + Send,
    ) -> Result<String, AppError> {
        if !endpoint.starts_with('/') || endpoint.starts_with("//") {
            return Err(AppError::Custom("Invalid LCU endpoint".into()));
        }
        let epoch = self.connection_epoch.load(Ordering::SeqCst);
        self.check_cooldown(epoch, &method, endpoint)
            .map_err(|e| AppError::LcuNotDispatched(e.to_string()))?;
        // Writes have their own capacity: match history must not starve ready-check or picks.
        let slots = if method != reqwest::Method::GET {
            &self.write_slots
        } else if super::session_recording::allowed_route(endpoint) {
            &self.critical_read_slots
        } else {
            &self.read_slots
        };
        let _permit = tokio::time::timeout(Duration::from_secs(3), slots.acquire())
            .await
            .map_err(|_| AppError::LcuNotDispatched("LCU request queue timed out".into()))?
            .map_err(|_| AppError::LcuNotDispatched("LCU request queue closed".into()))?;
        self.check_cooldown(epoch, &method, endpoint)
            .map_err(|e| AppError::LcuNotDispatched(e.to_string()))?;
        let (port, password) = self
            .get_lcu_auth()
            .ok_or_else(|| AppError::LcuNotDispatched("LCU not connected".into()))?;
        let scheme = "https";
        #[cfg(debug_assertions)]
        let scheme = if env::var_os("RUSTLM_TEST_LCU").is_some() {
            "http"
        } else {
            scheme
        };
        let url = format!("{}://127.0.0.1:{}{}", scheme, port, endpoint);
        let mut request = self
            .http_client
            .request(method.clone(), url)
            .header(AUTHORIZATION, Self::make_auth_header(&password))
            .header(CONTENT_TYPE, "application/json");
        if let Some(body) = body {
            request = request.body(body.to_owned());
        }
        let started = std::time::Instant::now();
        if self.connection_epoch.load(Ordering::SeqCst) != epoch
            || !is_current().await
            || self.connection_epoch.load(Ordering::SeqCst) != epoch
        {
            return Err(AppError::LcuNotDispatched(
                "LCU operation cancelled before dispatch".into(),
            ));
        }
        // A concurrent response may establish a cooldown while the async guard is reading state.
        self.check_cooldown(epoch, &method, endpoint)
            .map_err(|e| AppError::LcuNotDispatched(e.to_string()))?;
        let response = request.send().await.map_err(|_| {
            self.diagnostics
                .request(method.as_str(), endpoint, None, started.elapsed());
            self.invalidate_auth_if_current(port, &password, epoch);
            self.recorder.record(
                "http",
                method.as_str(),
                endpoint,
                Some(599),
                body,
                &serde_json::Value::Null,
            );
            AppError::Custom(format!(
                "LCU {} {}: connection failed or timed out",
                method, endpoint
            ))
        })?;
        let status = response.status();
        if !status.is_success() {
            if status.as_u16() == 429 {
                let delay = super::lcu_request_policy::retry_after(
                    response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|value| value.to_str().ok()),
                    chrono::Utc::now(),
                );
                self.request_policy.rate_limited(
                    epoch,
                    &method,
                    endpoint,
                    delay,
                    std::time::Instant::now(),
                );
            }
            self.recorder.record(
                "http",
                method.as_str(),
                endpoint,
                Some(status.as_u16()),
                body,
                &serde_json::Value::Null,
            );
            self.diagnostics.request(
                method.as_str(),
                endpoint,
                Some(status.as_u16()),
                started.elapsed(),
            );
            if matches!(status.as_u16(), 401 | 403) {
                self.invalidate_auth_if_current(port, &password, epoch);
            }
            if status.as_u16() == 429 {
                self.check_cooldown(epoch, &method, endpoint)?;
            }
            // Server response bodies can contain account data; log only method, route and status.
            return Err(AppError::Custom(format!(
                "LCU {} {} returned {}",
                method, endpoint, status
            )));
        }
        let text = Self::read_lcu_response(response, 16 * 1024 * 1024)
            .await
            .map_err(|error| {
                self.diagnostics
                    .request(method.as_str(), endpoint, None, started.elapsed());
                error
            })?;
        if self.connection_epoch.load(Ordering::SeqCst) != epoch {
            self.diagnostics
                .request(method.as_str(), endpoint, None, started.elapsed());
            // An in-flight write may have completed; its caller must reconcile, not blindly retry.
            return Err(AppError::Custom(
                "LCU connection changed during request; result is unknown".into(),
            ));
        }
        self.diagnostics.request(
            method.as_str(),
            endpoint,
            Some(status.as_u16()),
            started.elapsed(),
        );
        if self.recorder.active() && super::session_recording::allowed_route(endpoint) {
            self.recorder.record(
                "http",
                method.as_str(),
                endpoint,
                Some(status.as_u16()),
                body,
                &serde_json::from_str(&text).unwrap_or(serde_json::Value::Null),
            );
        }
        Ok(text)
    }

    async fn read_lcu_response(
        mut response: reqwest::Response,
        limit: usize,
    ) -> Result<String, AppError> {
        if response
            .content_length()
            .is_some_and(|size| size > limit as u64)
        {
            return Err(AppError::Custom("LCU response exceeds size limit".into()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| AppError::Custom("LCU response read failed".into()))?
        {
            if bytes.len().saturating_add(chunk.len()) > limit {
                return Err(AppError::Custom("LCU response exceeds size limit".into()));
            }
            bytes.extend_from_slice(&chunk);
        }
        String::from_utf8(bytes)
            .map_err(|_| AppError::Custom("LCU response is not valid UTF-8".into()))
    }

    fn check_cooldown(
        &self,
        epoch: u64,
        method: &reqwest::Method,
        endpoint: &str,
    ) -> Result<(), AppError> {
        if let Some(remaining) =
            self.request_policy
                .cooldown(epoch, method, endpoint, std::time::Instant::now())
        {
            return Err(AppError::Custom(format!(
                "LCU {} {} returned 429 Too Many Requests; retry after {} ms",
                method,
                endpoint,
                remaining.as_millis()
            )));
        }
        Ok(())
    }

    pub async fn lcu_get(&self, endpoint: &str) -> Result<String, AppError> {
        use super::lcu_request_policy::Read;
        let epoch = self.connection_epoch.load(Ordering::SeqCst);
        let current = || self.connection_epoch.load(Ordering::SeqCst) == epoch;
        let result = match self
            .request_policy
            .begin_read(epoch, endpoint)
            .map_err(AppError::Custom)?
        {
            Read::Lead(leader) => {
                let result = self
                    .lcu_request_if(reqwest::Method::GET, endpoint, None, current)
                    .await;
                leader.complete(
                    result
                        .as_ref()
                        .map(Clone::clone)
                        .map_err(ToString::to_string),
                );
                result
            }
            Read::Wait(flight) => flight.wait().await.map_err(AppError::Custom),
            Read::Unshared => {
                self.lcu_request_if(reqwest::Method::GET, endpoint, None, current)
                    .await
            }
        };
        if !current() {
            return Err(AppError::Custom(
                "LCU connection changed during shared read".into(),
            ));
        }
        result
    }
    pub async fn lcu_post(&self, endpoint: &str, body: &str) -> Result<String, AppError> {
        self.lcu_request(reqwest::Method::POST, endpoint, Some(body))
            .await
    }

    pub async fn lcu_post_if(
        &self,
        endpoint: &str,
        body: &str,
        is_current: impl Fn() -> bool + Send + Sync,
    ) -> Result<String, AppError> {
        self.lcu_request_if(reqwest::Method::POST, endpoint, Some(body), is_current)
            .await
    }
    pub(crate) async fn lcu_post_checked<F: std::future::Future<Output = bool> + Send>(
        &self,
        endpoint: &str,
        body: &str,
        check: impl FnOnce() -> F + Send,
    ) -> Result<String, AppError> {
        self.lcu_request_checked(reqwest::Method::POST, endpoint, Some(body), check)
            .await
    }
    pub async fn lcu_put(&self, endpoint: &str, body: &str) -> Result<String, AppError> {
        self.lcu_put_if(endpoint, body, || true).await
    }
    pub async fn lcu_put_if(
        &self,
        endpoint: &str,
        body: &str,
        is_current: impl Fn() -> bool + Send + Sync,
    ) -> Result<String, AppError> {
        self.lcu_request_if(reqwest::Method::PUT, endpoint, Some(body), is_current)
            .await
    }

    pub(crate) async fn lcu_put_checked<F: std::future::Future<Output = bool> + Send>(
        &self,
        endpoint: &str,
        body: &str,
        check: impl FnOnce() -> F + Send,
    ) -> Result<String, AppError> {
        self.lcu_request_checked(reqwest::Method::PUT, endpoint, Some(body), check)
            .await
    }

    #[cfg(test)]
    pub(crate) async fn hold_test_writes(&self) -> tokio::sync::SemaphorePermit<'_> {
        self.write_slots.acquire_many(2).await.unwrap()
    }
    pub async fn lcu_delete(&self, endpoint: &str) -> Result<String, AppError> {
        self.lcu_request(reqwest::Method::DELETE, endpoint, None)
            .await
    }
    pub(crate) async fn lcu_delete_checked<F: std::future::Future<Output = bool> + Send>(
        &self,
        endpoint: &str,
        check: impl FnOnce() -> F + Send,
    ) -> Result<String, AppError> {
        self.lcu_request_checked(reqwest::Method::DELETE, endpoint, None, check)
            .await
    }
    pub async fn lcu_patch(&self, endpoint: &str, body: &str) -> Result<String, AppError> {
        self.lcu_patch_if(endpoint, body, || true).await
    }

    pub async fn lcu_patch_if(
        &self,
        endpoint: &str,
        body: &str,
        is_current: impl Fn() -> bool + Send + Sync,
    ) -> Result<String, AppError> {
        serde_json::from_str::<serde_json::Value>(body)
            .map_err(|_| AppError::Custom("Invalid JSON body".into()))?;
        self.lcu_request_if(reqwest::Method::PATCH, endpoint, Some(body), is_current)
            .await
    }

    // --- Account Info ---

    pub async fn get_account_info(&self) -> Result<Option<AccountInfo>, AppError> {
        let summoner_json = self.lcu_get("/lol-summoner/v1/current-summoner").await?;

        let summoner: serde_json::Value = serde_json::from_str(&summoner_json)
            .map_err(|_| AppError::Custom("Failed to parse summoner".to_string()))?;

        if summoner.get("errorCode").is_some() {
            return Ok(None);
        }

        let display_name = summoner
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let profile_icon_id = summoner
            .get("profileIconId")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let summoner_level = summoner
            .get("summonerLevel")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let mut puuid = summoner
            .get("puuid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if !crate::models::account::is_valid_puuid(&puuid) {
            // current-summoner can be only partially populated while LCU finishes
            // logging in. Chat exposes the same account identity independently.
            if let Ok(chat_json) = self.lcu_get("/lol-chat/v1/me").await {
                if let Ok(chat) = serde_json::from_str::<serde_json::Value>(&chat_json) {
                    let chat_puuid = chat.get("puuid").and_then(|v| v.as_str()).unwrap_or("");
                    if crate::models::account::is_valid_puuid(chat_puuid) {
                        puuid = chat_puuid.to_string();
                    }
                }
            }
        }

        // Get ranked info
        let ranked_json = self
            .lcu_get("/lol-ranked/v1/current-ranked-stats")
            .await
            .unwrap_or_default();
        let ranked: serde_json::Value = serde_json::from_str(&ranked_json).unwrap_or_default();

        let mut rank = String::from("Unranked");
        let mut rank_display = String::new();

        if let Some(queues) = ranked.get("queues").and_then(|v| v.as_array()) {
            let preferred_queues = ["RANKED_SOLO_5x5", "RANKED_FLEX_SR", "RANKED_TFT"];
            for queue_type in &preferred_queues {
                if let Some(queue) = queues
                    .iter()
                    .find(|q| q.get("queueType").and_then(|v| v.as_str()) == Some(queue_type))
                {
                    let tier = queue.get("tier").and_then(|v| v.as_str()).unwrap_or("");
                    let division = queue.get("division").and_then(|v| v.as_str()).unwrap_or("");

                    if !tier.is_empty() && tier != "NONE" && tier != "NA" {
                        rank = format!("{} {}", tier, division);
                        let lp = queue
                            .get("leaguePoints")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        rank_display = format!("{} {} {}LP", tier, division, lp);
                        break;
                    }
                }
            }
        }

        // Get avatar URL from CommunityDragon (DDragon is missing many newer icon IDs)
        let avatar_url = format!(
            "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/profile-icons/{}.jpg",
            profile_icon_id
        );

        // Riot ID: current-summoner sometimes has gameName+tagLine before chat is ready
        let mut riot_id = Self::riot_id_from_summoner_json(&summoner);
        if riot_id.is_empty() {
            riot_id = self.get_riot_id().await.unwrap_or_default();
        }
        if riot_id.is_empty() {
            for _ in 0..6 {
                tokio::time::sleep(Duration::from_millis(500)).await;
                riot_id = self.get_riot_id().await.unwrap_or_default();
                if !riot_id.is_empty() {
                    break;
                }
            }
        }

        // Detect server/region
        let server = self.detect_server().await.unwrap_or_default();

        Ok(Some(AccountInfo {
            summoner_name: display_name,
            avatar_url,
            rank,
            rank_display,
            riot_id,
            puuid,
            summoner_level: summoner_level as i32,
            server,
        }))
    }

    pub async fn detect_server(&self) -> Result<String, AppError> {
        // Try /riotclient/region-locale first (works via RC API)
        if let Ok(resp) = self
            .rc_request(reqwest::Method::GET, "/riotclient/region-locale", None)
            .await
        {
            let val: serde_json::Value = serde_json::from_str(&resp).unwrap_or_default();
            if let Some(region) = val.get("region").and_then(|v| v.as_str()) {
                if !region.is_empty() {
                    return Ok(platform_to_server(region));
                }
            }
        }

        // Fallback: LCU shard-data
        if let Ok(resp) = self.lcu_get("/lol-service-status/v1/shard-data").await {
            let val: serde_json::Value = serde_json::from_str(&resp).unwrap_or_default();
            // Try slug first (e.g. "euw1"), then name
            for field in &["slug", "name"] {
                if let Some(v) = val.get(field).and_then(|v| v.as_str()) {
                    if !v.is_empty() {
                        return Ok(platform_to_server(v));
                    }
                }
            }
        }

        Ok(String::new())
    }

    /// Riot login username for the current RC/LCU session (email / login name), if exposed by APIs.
    pub async fn get_authorized_riot_login_username(&self) -> Result<String, AppError> {
        let generation = self.connection_generation();
        let read = |route| async move {
            tokio::time::timeout(
                Duration::from_secs(2),
                self.rc_request_silent(reqwest::Method::GET, route, None),
            )
            .await
            .ok()
            .and_then(Result::ok)
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        };
        if let Some(value) = read("/rso-auth/v1/authorization").await {
            if let Some(login) = identity_login(&value).map_err(AppError::Custom)? {
                if self.connection_generation() != generation {
                    return Err(AppError::Custom("Identity connection changed".into()));
                }
                return Ok(login);
            }
        }
        let replies = futures_util::future::join_all(
            [
                "/riot-client-auth/v1/userinfo",
                "/rso-auth/v1/authorization/userinfo",
                "/rso-auth/v1/session",
            ]
            .into_iter()
            .map(read),
        )
        .await;
        let mut found: Option<String> = None;
        for value in replies.into_iter().flatten() {
            if let Some(login) = identity_login(&value).map_err(AppError::Custom)? {
                if found
                    .as_ref()
                    .is_some_and(|old| !old.eq_ignore_ascii_case(&login))
                {
                    return Err(AppError::Custom("Identity responses disagree".into()));
                }
                found = Some(login);
            }
        }
        if found.is_none() {
            if let Ok(Ok(raw)) = tokio::time::timeout(
                Duration::from_secs(2),
                self.lcu_get("/lol-login/v1/session"),
            )
            .await
            {
                if let Ok(value) = serde_json::from_str(&raw) {
                    found = identity_login(&value).map_err(AppError::Custom)?;
                }
            }
        }
        if self.connection_generation() != generation {
            return Err(AppError::Custom("Identity connection changed".into()));
        }
        Ok(found.unwrap_or_default())
    }

    pub(crate) async fn verify_authorized_login(&self, expected: &str) -> Result<(), String> {
        let actual = self
            .get_authorized_riot_login_username()
            .await
            .map_err(|_| "Не удалось проверить аккаунт Riot")?;
        if expected.trim().is_empty() || actual.trim().is_empty() {
            return Err(
                "Riot не подтвердил выбранный аккаунт. Проверьте вход в Riot Client".into(),
            );
        }
        if !actual.trim().eq_ignore_ascii_case(expected.trim()) {
            return Err(
                "В Riot Client открыт другой аккаунт. Проверьте выбранную учётную запись".into(),
            );
        }
        Ok(())
    }

    fn riot_id_from_summoner_json(summoner: &serde_json::Value) -> String {
        let game_name = summoner
            .get("gameName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if game_name.is_empty() {
            return String::new();
        }
        let tag_line = summoner
            .get("tagLine")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        format!("{}#{}", game_name, tag_line)
    }

    async fn get_riot_id(&self) -> Result<String, AppError> {
        let resp = self.lcu_get("/lol-chat/v1/me").await?;
        let me: serde_json::Value = serde_json::from_str(&resp).unwrap_or_default();
        let game_name = me.get("gameName").and_then(|v| v.as_str()).unwrap_or("");
        let tag_line = me.get("gameTag").and_then(|v| v.as_str()).unwrap_or("");
        if game_name.is_empty() {
            Ok(String::new())
        } else {
            Ok(format!("{}#{}", game_name, tag_line))
        }
    }

    // --- RC HTTP API (Riot Client, NOT LCU) ---

    /// Make an HTTP request to Riot Client via its lockfile (port + password).
    /// This is separate from LCU — RC lockfile is in %LOCALAPPDATA%/Riot Games/Riot Client/Config/lockfile.
    async fn rc_request(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        body: Option<&str>,
    ) -> Result<String, AppError> {
        self.rc_request_impl(method, endpoint, body, true).await
    }

    /// Silent variant — no logging of non-2xx responses.
    /// Use for polling loops (is_rso_authorized, wait_for_rc_api_ready) to avoid log spam.
    async fn rc_request_silent(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        body: Option<&str>,
    ) -> Result<String, AppError> {
        self.rc_request_impl(method, endpoint, body, false).await
    }

    async fn rc_request_impl(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        body: Option<&str>,
        log_errors: bool,
    ) -> Result<String, AppError> {
        if !endpoint.starts_with('/') || endpoint.starts_with("//") {
            return Err(AppError::Custom("Invalid RC endpoint".into()));
        }
        #[cfg(debug_assertions)]
        let simulated = env::var_os("RUSTLM_TEST_LCU").is_some();
        #[cfg(not(debug_assertions))]
        let simulated = false;
        let (port, password, scheme) = if simulated {
            let (port, password) = self
                .get_lcu_auth()
                .ok_or_else(|| AppError::Custom("RC simulator not connected".into()))?;
            (port, password, "http")
        } else {
            let lockfile = Self::find_rc_lockfile()
                .ok_or_else(|| AppError::Custom("RC lockfile not found".into()))?;
            (lockfile.port, lockfile.password, "https")
        };
        let url = format!("{scheme}://127.0.0.1:{port}{endpoint}");
        let auth = Self::make_auth_header(&password);

        let mut req = self
            .http_client
            .request(method, &url)
            .header(AUTHORIZATION, &auth)
            .header(CONTENT_TYPE, "application/json");

        if let Some(b) = body {
            req = req.body(b.to_string());
        }

        let response = req
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("RC request failed: {}", e)))?;

        let status = response.status();
        let text = Self::read_lcu_response(response, 16 * 1024 * 1024)
            .await
            .map_err(|_| AppError::Custom("RC response could not be read safely".into()))?;

        if !status.is_success() {
            // RC error bodies may contain account or authorization data.
            let eula = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .is_some_and(|body| {
                    body["errorCode"] == "eula_not_accepted" || body["error"] == "eula_not_accepted"
                });
            let message = format!(
                "RC {}: HTTP {}{}",
                endpoint,
                status.as_u16(),
                if eula { " eula_not_accepted" } else { "" }
            );
            if log_errors {
                log::warn!("{}", message);
            }
            return Err(AppError::Custom(message));
        }

        Ok(text)
    }

    /// Logout current account via RC API.
    pub async fn logout_via_rc(&self) -> Result<(), AppError> {
        let _ = self
            .rc_request_silent(reqwest::Method::DELETE, "/rso-auth/v2/authorizations", None)
            .await;
        let _ = self
            .rc_request_silent(reqwest::Method::DELETE, "/rso-auth/v1/authorization", None)
            .await;
        let _ = self
            .rc_request_silent(
                reqwest::Method::PUT,
                "/rso-auth/v1/session/credentials",
                Some(r#"{"username":"","password":"","persistLogin":false}"#),
            )
            .await;
        Ok(())
    }

    /// Check if a user is currently authorized via RSO (Riot Sign-On).
    pub async fn is_rso_authorized(&self) -> bool {
        self.rso_authorization_state().await == Some(true)
    }

    pub async fn rso_authorization_state(&self) -> Option<bool> {
        let mut observed = None;
        for route in [
            "/rso-auth/v1/authorization",
            "/riot-client-auth/v1/authorization",
            "/rso-auth/v1/session",
        ] {
            if let Ok(Ok(raw)) = tokio::time::timeout(
                Duration::from_secs(2),
                self.rc_request_silent(reqwest::Method::GET, route, None),
            )
            .await
            {
                if let Ok(value) = serde_json::from_str(&raw) {
                    if let Some(state) = authorization_state(&value) {
                        if state {
                            return Some(true);
                        }
                        observed = Some(false);
                    }
                }
            }
        }
        observed
    }

    /// Initialize RSO session — required before login_via_rc can work.
    pub async fn init_rso_session(&self) -> Result<(), AppError> {
        let body = r#"{"clientId":"riot-client","trustLevels":["always_trusted"]}"#;
        for ep in ["/rso-auth/v2/authorizations", "/rso-auth/v1/authorization"] {
            match self
                .rc_request_silent(reqwest::Method::POST, ep, Some(body))
                .await
            {
                Ok(resp) => {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&resp) {
                        if json.get("errorCode").is_none() {
                            return Ok(());
                        }
                    } else {
                        return Ok(());
                    }
                }
                Err(_) => continue,
            }
        }
        Err(AppError::Custom(
            "RSO session init failed on all endpoints".to_string(),
        ))
    }

    /// Login via RC API using credentials (no UIA needed).
    /// Requires init_rso_session() to be called first.
    /// PUT /rso-auth/v1/session/credentials
    pub async fn login_via_rc(&self, username: &str, password: &str) -> Result<(), AppError> {
        let body = serde_json::json!({
            "username": username,
            "password": password,
            "persistLogin": false
        });

        let resp = self
            .rc_request(
                reqwest::Method::PUT,
                "/rso-auth/v1/session/credentials",
                Some(&body.to_string()),
            )
            .await?;

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&resp) {
            let resp_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");

            // Success
            if resp_type == "authenticated" || resp_type == "success" {
                return Ok(());
            }

            // 2FA required
            if resp_type == "multifactor" {
                let method = json
                    .get("multifactor")
                    .and_then(|m| m.get("method"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                return Err(AppError::Custom(format!(
                    "Login requires 2FA (method: {})",
                    method
                )));
            }

            // Error
            if let Some(error) = json.get("error").and_then(|v| v.as_str()) {
                return Err(AppError::Custom(format!(
                    "Login failed: {} (type: {})",
                    error, resp_type
                )));
            }
            if let Some(error_code) = json.get("errorCode").and_then(|v| v.as_str()) {
                let message = json
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or(error_code);
                return Err(AppError::Custom(format!("Login failed: {}", message)));
            }

            // No error fields — success
            return Ok(());
        }

        Ok(())
    }

    /// Launch League of Legends via RC product launcher API.
    /// Tries multiple endpoints since Riot changes them between versions.
    pub async fn launch_league_via_rc(&self) -> Result<(), AppError> {
        let endpoints = [
            "/product-launcher/v1/products/league_of_legends/patchlines/live",
            "/product-launcher/v1/products/league_of_legends/patchlines/live/launch",
            "/product-launcher/v1/launch",
        ];

        let body = r#"{"productId":"league_of_legends","patchlineId":"live"}"#;

        for endpoint in &endpoints {
            match self
                .rc_request(reqwest::Method::POST, endpoint, Some(body))
                .await
            {
                Ok(raw) => {
                    let body = serde_json::from_str::<serde_json::Value>(&raw).ok();
                    if body
                        .as_ref()
                        .is_some_and(|b| b.get("errorCode").is_some() || b.get("error").is_some())
                    {
                        return Err(AppError::Custom(
                            "Riot отклонил запуск League. Проверьте Riot Client перед повтором"
                                .into(),
                        ));
                    }
                    return Ok(());
                }
                Err(e)
                    if e.to_string().ends_with("HTTP 404")
                        || e.to_string().ends_with("HTTP 405") =>
                {
                    continue
                }
                Err(e)
                    if e.to_string().starts_with("RC request failed:")
                        || e.to_string().contains("response could not be read safely") =>
                {
                    // A lost launch response may already have started League; never launch twice.
                    if self
                        .lcu_get("/lol-summoner/v1/current-summoner")
                        .await
                        .is_ok()
                    {
                        return Ok(());
                    }
                    return Err(e);
                }
                Err(e) => return Err(e),
            }
        }

        // Last resort: start League directly via process
        Self::start_league_directly()
    }

    /// Start League Client directly via RiotClientServices.exe with launch args.
    fn start_league_directly() -> Result<(), AppError> {
        Self::launch_product(super::riot_game::RiotGame::League)
    }

    pub async fn open_tft_lobby(&self) -> Result<(), String> {
        let queues: serde_json::Value = serde_json::from_str(
            &self
                .lcu_get("/lol-game-queues/v1/queues")
                .await
                .map_err(|e| e.to_string())?,
        )
        .map_err(|_| "Не удалось проверить режимы TFT")?;
        if !queues.as_array().is_some_and(|q| {
            q.iter().any(|q| {
                q["id"] == 1090 && q["gameMode"] == "TFT" && q["queueAvailability"] == "Available"
            })
        }) {
            return Err("Обычный режим TFT сейчас недоступен. League открыт — выберите доступный режим вручную".into());
        }
        let phase: String = serde_json::from_str(
            &self
                .lcu_get("/lol-gameflow/v1/gameflow-phase")
                .await
                .map_err(|e| e.to_string())?,
        )
        .map_err(|_| "Не удалось проверить состояние League")?;
        if !matches!(phase.as_str(), "None" | "Lobby") {
            return Err("Нельзя открыть TFT во время поиска, выбора чемпионов или игры".into());
        }
        self.lcu_post("/lol-lobby/v2/lobby", r#"{"queueId":1090}"#)
            .await
            .map_err(|e| e.to_string())?;
        let lobby: serde_json::Value = serde_json::from_str(
            &self
                .lcu_get("/lol-lobby/v2/lobby")
                .await
                .map_err(|e| e.to_string())?,
        )
        .map_err(|_| "Не удалось подтвердить лобби TFT")?;
        if lobby["gameConfig"]["queueId"] != 1090 {
            return Err("Клиент не подтвердил лобби TFT".into());
        }
        Ok(())
    }

    pub fn is_valorant_running() -> bool {
        Self::is_process_running("VALORANT.exe")
            || Self::is_process_running("VALORANT-Win64-Shipping.exe")
    }

    pub fn launch_product(game: super::riot_game::RiotGame) -> Result<(), AppError> {
        #[cfg(debug_assertions)]
        if env::var_os("RUSTLM_TEST_LCU").is_some() {
            return Err(AppError::Custom(
                "Реальные процессы отключены в тестовом LCU".into(),
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let installs = Self::find_installs_json()
                .ok_or_else(|| AppError::Custom("Riot Client не найден".into()))?;
            let parsed: serde_json::Value = serde_json::from_str(&fs::read_to_string(installs)?)
                .map_err(|_| {
                    AppError::Custom("Не удалось прочитать установку Riot Client".into())
                })?;
            let executable = parsed["rc_default"]
                .as_str()
                .or_else(|| parsed["rc_live"].as_str())
                .map(PathBuf::from)
                .filter(|p| p.is_file())
                .ok_or_else(|| AppError::Custom("RiotClientServices.exe не найден".into()))?;
            Command::new(executable)
                .args(game.launch_args())
                .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
                .spawn()
                .map_err(|_| AppError::Custom(format!("Не удалось запустить {}", game.name())))?;
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = game;
            Err(AppError::Custom("Запуск поддерживается в Windows".into()))
        }
    }

    // --- Wait Helpers ---

    /// Wait for a process by name to appear (async, polling every 200ms).
    pub async fn wait_for_process(name: &str, timeout: Duration) -> bool {
        let name = name.to_string();
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if Self::is_process_running(&name) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        false
    }

    /// Wait for RC lockfile to appear (async, polling every 200ms).
    pub async fn wait_for_rc_lockfile(timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if Self::find_rc_lockfile().is_some() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        false
    }

    /// Wait for RC API to be responsive.
    pub async fn wait_for_rc_api_ready(&self, timeout: Duration) -> bool {
        tokio::time::timeout(timeout, async {
            loop {
                match self
                    .rc_request_silent(reqwest::Method::GET, "/riotclient/app-name", None)
                    .await
                {
                    Ok(resp) if !resp.contains("RESOURCE_NOT_FOUND") && !resp.is_empty() => {
                        return true
                    }
                    _ => {}
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        })
        .await
        .unwrap_or(false)
    }

    /// Wait for RSO authorization state to match target (polling every 300ms).
    pub async fn wait_for_rso_state(&self, authorized: bool, timeout: Duration) -> bool {
        tokio::time::timeout(timeout, async {
            loop {
                if self.rso_authorization_state().await == Some(authorized) {
                    return true;
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        })
        .await
        .unwrap_or(false)
    }

    // --- Login Phase Detection ---

    /// Detect what phase the login system is currently in.
    /// This allows the algorithm to resume from any point after failures or cold starts.
    pub async fn detect_login_phase(&self) -> LoginPhase {
        // Check if League is already running with LCU available
        if Self::is_league_running() {
            if self.get_lcu_auth().is_some() {
                return LoginPhase::LeagueRunning;
            }
        }

        // Check if RC is running at all
        let rc_running = Self::is_riot_client_running();
        if !rc_running {
            return LoginPhase::Nothing;
        }

        // RC process exists — check lockfile
        let lockfile = Self::find_rc_lockfile();
        if lockfile.is_none() {
            return LoginPhase::RcStarting;
        }

        // Lockfile exists — check if API is responsive via lightweight endpoint
        match self
            .rc_request_silent(reqwest::Method::GET, "/riotclient/app-name", None)
            .await
        {
            Ok(resp) if !resp.contains("RESOURCE_NOT_FOUND") && !resp.is_empty() => {
                if self.is_rso_authorized().await {
                    return LoginPhase::Authenticated;
                }
                LoginPhase::RcReady
            }
            _ => LoginPhase::RcWaitingForApi,
        }
    }

    // --- Connectivity Probe ---

    pub async fn probe_connectivity(&self) -> ClientConnectivityStatus {
        let is_rc_running = Self::is_riot_client_running();
        let is_league_running = Self::is_league_running();
        let rc_lockfile = Self::find_rc_lockfile().is_some();

        let lcu_lockfile = Self::find_lcu_lockfile();
        let lcu_found = lcu_lockfile.is_some();
        let lcu_port = lcu_lockfile.as_ref().map(|l| l.port as i32);
        let lcu_lockfile_path = None; // Could be computed but not critical

        let lcu_http_ok = if lcu_found {
            self.lcu_get("/lol-service-status/v1/shard-data")
                .await
                .is_ok()
        } else {
            false
        };

        ClientConnectivityStatus {
            is_riot_client_running: is_rc_running,
            is_league_running,
            rc_lockfile_found: rc_lockfile,
            lcu_lockfile_found: lcu_found,
            lcu_port,
            lcu_http_ok,
            lcu_lockfile_path,
            league_install_path: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AccountInfo {
    pub summoner_name: String,
    pub avatar_url: String,
    pub rank: String,
    pub rank_display: String,
    pub riot_id: String,
    pub puuid: String,
    pub summoner_level: i32,
    pub server: String,
}

/// Describes the current phase of the Riot Client login lifecycle.
/// Used to determine what the login algorithm should do next.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum LoginPhase {
    /// Nothing is running — need to start RC from scratch
    Nothing,
    /// RC process is spawned but lockfile doesn't exist yet
    RcStarting,
    /// RC lockfile exists but API returns 404 (still initializing)
    RcWaitingForApi,
    /// RC API is responsive, ready for login
    RcReady,
    /// User is already RSO-authorized
    Authenticated,
    /// League Client is running with LCU available
    LeagueRunning,
}

pub(crate) fn platform_to_server(platform: &str) -> String {
    let upper = platform.to_uppercase();
    match upper.as_str() {
        "EUW1" | "EUW" => "EUW",
        "EUN1" | "EUNE" => "EUNE",
        "NA1" | "NA" => "NA",
        "KR" => "KR",
        "RU" => "RU",
        "TR1" | "TR" => "TR",
        "BR1" | "BR" => "BR",
        "JP1" | "JP" => "JP",
        "LA1" | "LAN" => "LAN",
        "LA2" | "LAS" => "LAS",
        "OC1" | "OCE" => "OCE",
        "PH2" | "PH" => "PH",
        "SG2" | "SG" => "SG",
        "TH2" | "TH" => "TH",
        "TW2" | "TW" => "TW",
        "VN2" | "VN" => "VN",
        _ => return upper,
    }
    .to_string()
}

#[cfg(all(test, windows))]
mod native_process_tests {
    #[test]
    fn queries_current_executable_without_starting_system_utilities() {
        assert_eq!(
            super::RiotClientService::process_path(std::process::id()).unwrap(),
            std::env::current_exe().unwrap()
        );
        assert!(super::RiotClientService::process_path(0).is_none());
    }
}

#[cfg(all(test, debug_assertions))]
mod rc_simulator_tests {
    #[test]
    fn identity_and_rso_parsers_reject_unknown_conflicting_or_private_lookalike_fields() {
        use serde_json::json;
        assert_eq!(
            identity_login(&json!({"userInfo":json!({"username":" AccountA "}).to_string()}))
                .unwrap(),
            Some("AccountA".into())
        );
        assert_eq!(
            identity_login(&json!({"username":"AccountA","session":{"loginUsername":"accounta"}}))
                .unwrap(),
            Some("accounta".into())
        );
        assert!(
            identity_login(&json!({"username":"AccountA","user":{"username":"AccountB"}})).is_err()
        );
        assert!(identity_login(&json!({"userInfo":"not JSON"})).is_err());
        assert_eq!(
            identity_login(&json!({"displayName":"AccountA","hint":{"username":"AccountA"}}))
                .unwrap(),
            None
        );
        for value in [
            json!({}),
            json!([]),
            json!({"errorCode":"UNKNOWN"}),
            json!({"authorized":false,"subject":"subject"}),
            json!({"type":"unknown"}),
        ] {
            assert_eq!(authorization_state(&value), None);
        }
        assert_eq!(
            authorization_state(&json!({"type":"authenticated"})),
            Some(true)
        );
        assert_eq!(
            authorization_state(&json!({"type":"authenticated","error":"","errorCode":null})),
            Some(true)
        );
        assert_eq!(
            authorization_state(&json!({"type":"needs_credentials"})),
            Some(false)
        );
        assert_eq!(
            authorization_state(&json!({"authorized":false})),
            Some(false)
        );
        assert_eq!(
            authorization_state(&json!({"authorization":{"accessToken":"synthetic"}})),
            Some(true)
        );
    }

    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_userinfo_fallback_and_rso_deadline_are_bounded() {
        let client = RiotClientService::new();
        let route = "/riot-client-auth/v1/userinfo";
        for login in ["AccountA", "AccountB"] {
            client.lcu_post("/__test/failure", &serde_json::json!({"route":route,"status":200,"response":{"userInfo":serde_json::json!({"username":login}).to_string()}}).to_string()).await.unwrap();
            assert_eq!(
                client.verify_authorized_login("AccountA").await.is_ok(),
                login == "AccountA"
            );
        }
        client
            .lcu_post(
                "/__test/failure",
                r#"{"route":"/rso-auth/v1/authorization","status":200,"delayMs":1000}"#,
            )
            .await
            .unwrap();
        assert!(!tokio::time::timeout(
            Duration::from_millis(500),
            client.wait_for_rso_state(false, Duration::from_millis(10))
        )
        .await
        .unwrap());
        client
            .lcu_post(
                "/__test/failure",
                r#"{"route":"/riotclient/app-name","status":200,"delayMs":1000}"#,
            )
            .await
            .unwrap();
        assert!(!tokio::time::timeout(
            Duration::from_millis(500),
            client.wait_for_rc_api_ready(Duration::from_millis(10))
        )
        .await
        .unwrap());
    }
    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_search_writes_are_cancelled_when_phase_changes_in_queue() {
        let client = RiotClientService::new();
        let control = RiotClientService::new();
        let route = "/lol-lobby/v2/lobby/matchmaking/search";
        for deleting in [false, true] {
            control
                .lcu_post("/__test/scenario", r#"{"name":"idle"}"#)
                .await
                .unwrap();
            let permits = client.hold_test_writes().await;
            let request = async {
                let check = || async {
                    client
                        .lcu_get("/lol-gameflow/v1/gameflow-phase")
                        .await
                        .is_ok_and(|v| v == "\"Lobby\"")
                };
                if deleting {
                    client.lcu_delete_checked(route, check).await
                } else {
                    client.lcu_post_checked(route, "{}", check).await
                }
            };
            tokio::pin!(request);
            assert!(futures_util::poll!(&mut request).is_pending());
            control
                .lcu_post("/__test/scenario", r#"{"name":"ready"}"#)
                .await
                .unwrap();
            let before: serde_json::Value =
                serde_json::from_str(&control.lcu_get("/__test/state").await.unwrap()).unwrap();
            drop(permits);
            assert!(request
                .await
                .unwrap_err()
                .to_string()
                .contains("cancelled before dispatch"));
            let after: serde_json::Value =
                serde_json::from_str(&control.lcu_get("/__test/state").await.unwrap()).unwrap();
            let key = format!("{} {route}", if deleting { "DELETE" } else { "POST" });
            assert_eq!(before["requestCounts"][&key], after["requestCounts"][&key]);
            assert_eq!(before["actions"], after["actions"]);
        }
    }
    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_guarded_reload_rechecks_phase_after_write_queue() {
        let client = RiotClientService::new();
        let control = RiotClientService::new();
        control
            .lcu_post("/__test/scenario", r#"{"name":"idle"}"#)
            .await
            .unwrap();
        control
            .lcu_post("/__test/ux-health", r#"{"window_responsive":false}"#)
            .await
            .unwrap();
        let permits = client.hold_test_writes().await;
        let request = client.lcu_post_checked("/riotclient/kill-and-restart-ux", "", || async {
            super::super::unglitch::status(&client)
                .await
                .is_ok_and(|sample| crate::models::unglitch::automatic_reload_allowed(&sample))
        });
        tokio::pin!(request);
        assert!(futures_util::poll!(&mut request).is_pending());
        control
            .lcu_post("/__test/scenario", r#"{"name":"classic"}"#)
            .await
            .unwrap();
        let before: serde_json::Value =
            serde_json::from_str(&control.lcu_get("/__test/state").await.unwrap()).unwrap();
        drop(permits);
        assert!(request
            .await
            .unwrap_err()
            .to_string()
            .contains("cancelled before dispatch"));
        let after: serde_json::Value =
            serde_json::from_str(&control.lcu_get("/__test/state").await.unwrap()).unwrap();
        assert_eq!(after["actions"], before["actions"]);
        assert_eq!(
            after["requestCounts"]["POST /riotclient/kill-and-restart-ux"],
            before["requestCounts"]["POST /riotclient/kill-and-restart-ux"]
        );
    }

    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_identity_checks_and_lost_launch_response_never_launch_twice() {
        let client = RiotClientService::new();
        client
            .lcu_post("/__test/scenario", r#"{"name":"idle"}"#)
            .await
            .unwrap();
        assert!(client.verify_authorized_login("AccountA").await.is_err());
        for (actual, valid) in [("AccountB", false), (" accounta ", true), ("", false)] {
            let fault = serde_json::json!({"route":"/rso-auth/v1/authorization","status":200,"response":{"username":actual}});
            client
                .lcu_post("/__test/failure", &fault.to_string())
                .await
                .unwrap();
            assert_eq!(
                client.verify_authorized_login("AccountA").await.is_ok(),
                valid
            );
        }
        let route = "/product-launcher/v1/products/league_of_legends/patchlines/live";
        let before: serde_json::Value =
            serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap()).unwrap();
        client
            .lcu_post(
                "/__test/failure",
                &serde_json::json!({"route":route,"status":204,"dropAfterApply":true}).to_string(),
            )
            .await
            .unwrap();
        assert!(client.launch_league_via_rc().await.is_err());
        let after: serde_json::Value =
            serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap()).unwrap();
        let key = format!("POST {route}");
        assert_eq!(
            after["requestCounts"][&key].as_u64().unwrap(),
            before["requestCounts"][&key].as_u64().unwrap_or(0) + 1
        );
        assert_eq!(
            after["requestCounts"][format!("POST {route}/launch")],
            before["requestCounts"][format!("POST {route}/launch")]
        );
        assert_eq!(
            after["requestCounts"]["POST /product-launcher/v1/launch"],
            before["requestCounts"]["POST /product-launcher/v1/launch"]
        );
        for status in [401, 403, 429, 503] {
            client
                .lcu_post(
                    "/__test/failure",
                    &serde_json::json!({"route":route,"status":status}).to_string(),
                )
                .await
                .unwrap();
            assert!(client.launch_league_via_rc().await.is_err());
        }
        assert!(RiotClientService::start_riot_client()
            .unwrap_err()
            .to_string()
            .contains("отключены"));
        assert!(
            RiotClientService::launch_product(crate::services::riot_game::RiotGame::League)
                .unwrap_err()
                .to_string()
                .contains("отключены")
        );

    }

    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_shared_reads_use_one_request_and_cancelled_leader_releases_followers() {
        let client = RiotClientService::new();
        client
            .lcu_post("/__test/scenario", r#"{"name":"ready"}"#)
            .await
            .unwrap();
        let before: serde_json::Value =
            serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap()).unwrap();
        let route = "/lol-gameflow/v1/session";
        client
            .lcu_post(
                "/__test/failure",
                r#"{"route":"/lol-gameflow/v1/session","status":200,"delayMs":100}"#,
            )
            .await
            .unwrap();
        let (a, b) = tokio::join!(client.lcu_get(route), client.lcu_get(route));
        assert_eq!(a.unwrap(), b.unwrap());
        let after: serde_json::Value =
            serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap()).unwrap();
        let count = |value: &serde_json::Value| {
            value["requestCounts"]["GET /lol-gameflow/v1/session"]
                .as_u64()
                .unwrap_or(0)
        };
        assert_eq!(count(&after), count(&before) + 1);
        let fresh: serde_json::Value =
            serde_json::from_str(&client.lcu_get(route).await.unwrap()).unwrap();
        assert_eq!(
            fresh["phase"], "ReadyCheck",
            "completed response must not become a cache"
        );
        let permits = client.critical_read_slots.acquire_many(2).await.unwrap();
        let mut leader = Box::pin(client.lcu_get(route));
        assert!(futures_util::poll!(&mut leader).is_pending());
        let mut follower = Box::pin(client.lcu_get(route));
        assert!(futures_util::poll!(&mut follower).is_pending());
        drop(leader);
        assert!(follower
            .await
            .unwrap_err()
            .to_string()
            .contains("cancelled"));
        drop(permits);
        assert!(client.lcu_get(route).await.is_ok());
    }

    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_rate_limit_is_rechecked_after_async_guard() {
        let client = RiotClientService::new();
        let control = RiotClientService::new();
        let route = "/__test/guard-rate-limit";
        let (release, wait) = tokio::sync::oneshot::channel::<()>();
        let request = client.lcu_post_checked(route, "{}", || async { wait.await.is_ok() });
        tokio::pin!(request);
        assert!(futures_util::poll!(&mut request).is_pending());
        control
            .lcu_post(
                "/__test/failure",
                &serde_json::json!({"route":route,"status":429,"retryAfter":60}).to_string(),
            )
            .await
            .unwrap();
        assert!(client
            .lcu_post(route, "{}")
            .await
            .unwrap_err()
            .to_string()
            .contains("429"));
        let before: serde_json::Value =
            serde_json::from_str(&control.lcu_get("/__test/state").await.unwrap()).unwrap();
        release.send(()).unwrap();
        let not_sent = request.await.unwrap_err();
        assert!(matches!(&not_sent, AppError::LcuNotDispatched(_)));
        assert!(not_sent.to_string().contains("429"));
        let after: serde_json::Value =
            serde_json::from_str(&control.lcu_get("/__test/state").await.unwrap()).unwrap();
        assert_eq!(
            before["requestCounts"]["POST /__test/guard-rate-limit"],
            after["requestCounts"]["POST /__test/guard-rate-limit"]
        );
    }
    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_rate_limit_suppresses_retries_but_not_other_routes_or_new_connection() {
        let client = RiotClientService::new();
        client
            .lcu_post("/__test/scenario", r#"{"name":"ready"}"#)
            .await
            .unwrap();
        client
            .lcu_post(
                "/__test/failure",
                r#"{"route":"/lol-gameflow/v1/session","status":429,"retryAfter":"2"}"#,
            )
            .await
            .unwrap();
        let before: serde_json::Value =
            serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap()).unwrap();
        assert!(client
            .lcu_get("/lol-gameflow/v1/session")
            .await
            .unwrap_err()
            .to_string()
            .contains("429"));
        assert!(client
            .lcu_get("/lol-gameflow/v1/session")
            .await
            .unwrap_err()
            .to_string()
            .contains("retry after"));
        assert!(client
            .lcu_get("/lol-matchmaking/v1/ready-check")
            .await
            .is_ok());
        let after: serde_json::Value =
            serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap()).unwrap();
        let count = |value: &serde_json::Value| {
            value["requestCounts"]["GET /lol-gameflow/v1/session"]
                .as_u64()
                .unwrap_or(0)
        };
        assert_eq!(count(&after), count(&before) + 1);
        client.invalidate_cache();
        assert!(client.lcu_get("/lol-gameflow/v1/session").await.is_ok());
        client.lcu_post("/__test/failure",r#"{"route":"/lol-matchmaking/v1/ready-check/accept","status":429,"retryAfter":"2"}"#).await.unwrap();
        assert!(client
            .lcu_post("/lol-matchmaking/v1/ready-check/accept", "{}")
            .await
            .is_err());
        assert!(client
            .lcu_post("/lol-matchmaking/v1/ready-check/accept", "{}")
            .await
            .is_err());
        let ready: serde_json::Value = serde_json::from_str(
            &client
                .lcu_get("/lol-matchmaking/v1/ready-check")
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(ready["playerResponse"], "None");
        let final_state: serde_json::Value =
            serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap()).unwrap();
        let writes = |value: &serde_json::Value| {
            value["requestCounts"]["POST /lol-matchmaking/v1/ready-check/accept"]
                .as_u64()
                .unwrap_or(0)
        };
        assert_eq!(
            writes(&final_state),
            writes(&after) + 1,
            "a rejected write is never silently retried"
        );
    }
    #[tokio::test]
    async fn response_reader_bounds_declared_and_chunked_bodies_and_checks_utf8() {
        use std::io::{Read, Write};
        let body = "x".repeat(32);
        let declared =
            format!("HTTP/1.1 200 OK\r\nContent-Length: 32\r\nConnection: close\r\n\r\n{body}")
                .into_bytes();
        let chunked=format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n20\r\n{body}\r\n0\r\n\r\n").into_bytes();
        let mut invalid =
            b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\n".to_vec();
        invalid.push(255);
        for (wire, limit, error) in [
            (declared.clone(), 8, Some("size limit")),
            (chunked, 8, Some("size limit")),
            (declared, 32, None),
            (invalid, 32, Some("UTF-8")),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let server = tokio::task::spawn_blocking(move || {
                let deadline = std::time::Instant::now() + Duration::from_secs(3);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                && std::time::Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(1))
                        }
                        Err(e) => panic!("loopback accept failed: {e}"),
                    }
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
                    let count = socket.read(&mut buffer).unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..count]);
                }
                let _ = socket.write_all(&wire);
            });
            let client = RiotClientService::new();
            let response = client
                .http_client
                .get(format!("http://{address}/"))
                .send()
                .await
                .unwrap();
            let result = RiotClientService::read_lcu_response(response, limit).await;
            if let Some(error) = error {
                assert!(result.unwrap_err().to_string().contains(error));
            } else {
                assert_eq!(result.unwrap(), body);
            }
            server.await.unwrap();
        }
    }
    use super::*;
    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_tft_requires_available_queue_idle_phase_and_confirmed_lobby() {
        let client = RiotClientService::new();
        client
            .lcu_post("/__test/scenario", r#"{"name":"ready"}"#)
            .await
            .unwrap();
        assert!(client.open_tft_lobby().await.is_err());
        client
            .lcu_post("/__test/scenario", r#"{"name":"idle"}"#)
            .await
            .unwrap();
        client.open_tft_lobby().await.unwrap();
        client
            .lcu_post(
                "/__test/failure",
                r#"{"route":"/lol-game-queues/v1/queues","status":503}"#,
            )
            .await
            .unwrap();
        assert!(client.open_tft_lobby().await.is_err());
        client
            .lcu_post(
                "/__test/failure",
                r#"{"route":"/lol-lobby/v2/lobby","status":503}"#,
            )
            .await
            .unwrap();
        assert!(client.open_tft_lobby().await.is_err());
    }

    #[tokio::test]
    #[ignore = "requires an explicit recorded-session replay config"]
    async fn replay_ready_check_uses_native_transport_and_matches_recorded_write() {
        assert!(std::env::var_os("RUSTLM_REPLAY_SMOKE").is_some());
        let client = RiotClientService::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            if let Ok(body) = client.lcu_get("/lol-matchmaking/v1/ready-check").await {
                let value: serde_json::Value = serde_json::from_str(&body).unwrap();
                assert_eq!(value["state"], "InProgress");
                assert_eq!(value["playerResponse"], "None");
                break;
            }
            assert!(tokio::time::Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        client
            .lcu_post("/lol-matchmaking/v1/ready-check/accept", "{}")
            .await
            .unwrap();
    }

    #[test]
    fn stale_lockfile_lookup_cannot_repopulate_or_replace_new_generation_auth() {
        let client = RiotClientService::new();
        let old = client.connection_generation();
        client.invalidate_cache();
        assert!(client
            .cache_discovered_auth(old, (1111, "old".into()))
            .is_none());
        let generation = client.connection_generation();
        assert_eq!(
            client.cache_discovered_auth(generation, (2222, "new".into())),
            Some((2222, "new".into()))
        );
        assert_eq!(
            client.cache_discovered_auth(old, (1111, "old".into())),
            Some((2222, "new".into()))
        );
        assert_eq!(
            client.cache_discovered_auth(generation, (3333, "late".into())),
            Some((2222, "new".into()))
        );
    }
    #[test]
    fn delayed_old_request_cannot_clear_new_connection_credentials() {
        let client = RiotClientService::new();
        *client.cached_lcu_auth.lock().unwrap() = Some((1234, "new".into()));
        client.invalidate_auth_if_current(1234, "old", 0);
        assert_eq!(
            client.cached_lcu_auth.lock().unwrap().as_ref().unwrap().1,
            "new"
        );
        client.invalidate_auth_if_current(1234, "new", 0);
        assert!(client.cached_lcu_auth.lock().unwrap().is_none());
        *client.cached_lcu_auth.lock().unwrap() = Some((1234, "new".into()));
        client.invalidate_auth_if_current(1234, "new", 0);
        assert!(
            client.cached_lcu_auth.lock().unwrap().is_some(),
            "old epoch cannot clear a reconnected identical credential"
        );
    }

    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_old_success_is_rejected_after_connection_invalidation() {
        let client = RiotClientService::new();
        client
            .lcu_post(
                "/__test/failure",
                r#"{"route":"/lol-gameflow/v1/session","status":200,"delayMs":200}"#,
            )
            .await
            .unwrap();
        let request = client.lcu_get("/lol-gameflow/v1/session");
        let invalidate = async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            client.invalidate_cache();
        };
        let (response, ()) = tokio::join!(request, invalidate);
        assert!(response
            .unwrap_err()
            .to_string()
            .contains("connection changed"));
        assert!(client.lcu_get("/lol-gameflow/v1/session").await.is_ok());
    }

    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_readonly_health_probe_preserves_connection_generation() {
        let client = RiotClientService::new();
        let epoch = client.connection_epoch.load(Ordering::SeqCst);
        let health = super::super::unglitch::status(&client).await.unwrap();
        assert!(health.api_available);
        assert_eq!(client.connection_epoch.load(Ordering::SeqCst), epoch);
    }

    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_cancelled_write_is_not_dispatched_after_queue_wait() {
        let client = RiotClientService::new();
        client
            .lcu_post("/__test/scenario", r#"{"name":"ready"}"#)
            .await
            .unwrap();
        let permit = client.write_slots.acquire_many(2).await.unwrap();
        let allowed = std::sync::atomic::AtomicBool::new(true);
        let dispatched = std::sync::atomic::AtomicBool::new(false);
        let request = client.lcu_post_if("/lol-matchmaking/v1/ready-check/accept", "{}", || {
            dispatched.store(true, Ordering::SeqCst);
            allowed.load(Ordering::SeqCst)
        });
        tokio::pin!(request);
        // Poll the actual future until pending behind the occupied write queue.
        assert!(futures_util::poll!(&mut request).is_pending());
        assert!(!dispatched.load(Ordering::SeqCst));
        allowed.store(false, Ordering::SeqCst);
        drop(permit);
        assert!(request
            .await
            .unwrap_err()
            .to_string()
            .contains("cancelled before dispatch"));
        let state: serde_json::Value =
            serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap()).unwrap();
        assert_eq!(state["ready"]["playerResponse"], "None");
    }

    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_write_capacity_is_independent_of_busy_readers() {
        let client = RiotClientService::new();
        let readers = client.read_slots.acquire_many(8).await.unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            client.lcu_post("/__test/scenario", r#"{"name":"ready"}"#),
        )
        .await;
        assert!(result.unwrap().is_ok());
        assert!(tokio::time::timeout(
            Duration::from_secs(1),
            client.lcu_get("/lol-matchmaking/v1/ready-check")
        )
        .await
        .unwrap()
        .is_ok());
        drop(readers);
        let folder = std::env::temp_dir().join(format!("rustlm-record-{}", rand::random::<u128>()));
        let path = client.recorder.start(folder.clone()).unwrap().path.unwrap();
        client
            .lcu_get("/lol-matchmaking/v1/ready-check")
            .await
            .unwrap();
        client
            .lcu_post("/lol-matchmaking/v1/ready-check/accept", "{}")
            .await
            .unwrap();
        assert_eq!(client.recorder.stop().unwrap().entries, 2);
        let trace = std::fs::read_to_string(&path).unwrap();
        if let Some(output) = std::env::var_os("RUSTLM_RECORD_OUTPUT") {
            std::fs::write(output, &trace).unwrap();
        }
        assert!(trace.contains("rustlm-lcu-session"));
        assert!(!trace.contains(&client.get_lcu_auth().unwrap().1));
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_rc_rejects_http_errors_and_does_not_confirm_unknown_logout() {
        assert!(std::env::var_os("RUSTLM_TEST_LCU").is_some());
        let client = RiotClientService::new();
        for status in [401, 403, 404, 429, 500, 503] {
            client
                .lcu_post(
                    "/__test/failure",
                    &format!(r#"{{"route":"/lol-gameflow/v1/session","status":{status},"retryAfter":"0"}}"#),
                )
                .await
                .unwrap();
            let error = client
                .rc_request_impl(reqwest::Method::GET, "/lol-gameflow/v1/session", None, true)
                .await
                .unwrap_err()
                .to_string();
            assert!(error.contains(&status.to_string()));
            assert!(!error.contains("synthetic private response"));
        }
        assert!(client
            .rc_request_impl(
                reqwest::Method::GET,
                "/lol-gameflow/v1/session",
                None,
                false
            )
            .await
            .is_ok());
        assert!(client
            .rc_request_impl(reqwest::Method::GET, "//other-host", None, false)
            .await
            .is_err());
        assert!(client.init_rso_session().await.is_err());
        assert!(!client.wait_for_rc_api_ready(Duration::from_millis(1)).await);
        assert_eq!(client.rso_authorization_state().await, None);
        assert!(
            !client
                .wait_for_rso_state(false, Duration::from_millis(1))
                .await
        );
    }
}
