use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use crate::error::AppError;
use crate::models::league_client::{ClientConnectivityStatus, LockfileInfo};

pub struct RiotClientService {
    cached_lcu_auth: Mutex<Option<(u16, String)>>,
    http_client: reqwest::Client,
}

impl RiotClientService {
    pub fn new() -> Self {
        // Create HTTP client that ignores self-signed certs (LCU uses self-signed)
        let http_client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            cached_lcu_auth: Mutex::new(None),
            http_client,
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
        // Try common League Client install paths
        let candidates = Self::enumerate_lcu_lockfile_candidates();

        for path in candidates {
            if let Some(info) = Self::parse_lockfile(&path) {
                return Some(info);
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
            std::io::BufReader::new(file).read_to_string(&mut content).ok()?;
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
        let output = Command::new("wmic")
            .args(["process", "where", "name='LeagueClientUx.exe'", "get", "ExecutablePath", "/VALUE"])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(path_str) = line.strip_prefix("ExecutablePath=") {
                let path = PathBuf::from(path_str.trim());
                if let Some(parent) = path.parent() {
                    return Some(parent.join("lockfile"));
                }
            }
        }
        None
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
        use windows::Win32::System::Diagnostics::ToolHelp::*;
        use windows::Win32::Foundation::CloseHandle;

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
                        &entry.szExeFile[..entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len())]
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
            use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
            use windows::Win32::Foundation::CloseHandle;
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
        Self::is_process_running("Riot Client.exe")
            || Self::is_process_running("RiotClientUx.exe")
    }

    pub fn is_riot_client_services_running() -> bool {
        Self::is_process_running("RiotClientServices.exe")
    }

    pub fn is_league_running() -> bool {
        Self::is_process_running("LeagueClientUx.exe")
    }

    pub fn kill_league(include_riot_client: bool) {
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/F", "/IM", "LeagueClientUx.exe"])
                .output();
            let _ = Command::new("taskkill")
                .args(["/F", "/IM", "LeagueClient.exe"])
                .output();
            if include_riot_client {
                let _ = Command::new("taskkill")
                    .args(["/F", "/IM", "Riot Client.exe"])
                    .output();
                let _ = Command::new("taskkill")
                    .args(["/F", "/IM", "RiotClientUx.exe"])
                    .output();
                let _ = Command::new("taskkill")
                    .args(["/F", "/IM", "RiotClientServices.exe"])
                    .output();
            }
        }
    }

    pub fn start_riot_client() -> Result<(), AppError> {
        #[cfg(windows)]
        {
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
                                .spawn()
                                .map_err(|e| AppError::Custom(format!("Failed to start RC: {}", e)))?;
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
                        .spawn()
                        .map_err(|e| AppError::Custom(format!("Failed to start RC: {}", e)))?;
                    return Ok(());
                }
            }

            Err(AppError::Custom("Riot Client not found".to_string()))
        }
        #[cfg(not(windows))]
        Err(AppError::Custom("Not supported on this platform".to_string()))
    }

    // --- LCU HTTP API ---

    pub fn get_lcu_auth(&self) -> Option<(u16, String)> {
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
            let mut cache = self.cached_lcu_auth.lock().unwrap();
            *cache = Some(auth.clone());
            return Some(auth);
        }

        None
    }

    pub fn invalidate_cache(&self) {
        let mut cache = self.cached_lcu_auth.lock().unwrap();
        *cache = None;
    }

    fn make_auth_header(password: &str) -> String {
        use base64::Engine;
        let credentials = format!("riot:{}", password);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes());
        format!("Basic {}", encoded)
    }

    pub async fn lcu_get(&self, endpoint: &str) -> Result<String, AppError> {
        let (port, password) = self
            .get_lcu_auth()
            .ok_or_else(|| AppError::Custom("LCU not connected".to_string()))?;

        let url = format!("https://127.0.0.1:{}{}", port, endpoint);
        let auth = Self::make_auth_header(&password);

        let response = self
            .http_client
            .get(&url)
            .header(AUTHORIZATION, &auth)
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("LCU GET failed: {}", e)))?;

        let text = response
            .text()
            .await
            .map_err(|e| AppError::Custom(format!("LCU response read failed: {}", e)))?;

        Ok(text)
    }

    pub async fn lcu_post(&self, endpoint: &str, body: &str) -> Result<String, AppError> {
        let (port, password) = self
            .get_lcu_auth()
            .ok_or_else(|| AppError::Custom("LCU not connected".to_string()))?;

        let url = format!("https://127.0.0.1:{}{}", port, endpoint);
        let auth = Self::make_auth_header(&password);

        let response = self
            .http_client
            .post(&url)
            .header(AUTHORIZATION, &auth)
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("LCU POST failed: {}", e)))?;

        let text = response
            .text()
            .await
            .map_err(|e| AppError::Custom(format!("LCU response read failed: {}", e)))?;

        Ok(text)
    }

    pub async fn lcu_put(&self, endpoint: &str, body: &str) -> Result<String, AppError> {
        let (port, password) = self
            .get_lcu_auth()
            .ok_or_else(|| AppError::Custom("LCU not connected".to_string()))?;

        let url = format!("https://127.0.0.1:{}{}", port, endpoint);
        let auth = Self::make_auth_header(&password);

        let response = self
            .http_client
            .put(&url)
            .header(AUTHORIZATION, &auth)
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("LCU PUT failed: {}", e)))?;

        let text = response
            .text()
            .await
            .map_err(|e| AppError::Custom(format!("LCU response read failed: {}", e)))?;

        Ok(text)
    }

    pub async fn lcu_delete(&self, endpoint: &str) -> Result<String, AppError> {
        let (port, password) = self
            .get_lcu_auth()
            .ok_or_else(|| AppError::Custom("LCU not connected".to_string()))?;

        let url = format!("https://127.0.0.1:{}{}", port, endpoint);
        let auth = Self::make_auth_header(&password);

        let response = self
            .http_client
            .delete(&url)
            .header(AUTHORIZATION, &auth)
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("LCU DELETE failed: {}", e)))?;

        let text = response
            .text()
            .await
            .map_err(|e| AppError::Custom(format!("LCU response read failed: {}", e)))?;

        Ok(text)
    }

    pub async fn lcu_patch(&self, endpoint: &str, body: &str) -> Result<String, AppError> {
        let (port, password) = self
            .get_lcu_auth()
            .ok_or_else(|| AppError::Custom("LCU not connected".to_string()))?;

        let url = format!("https://127.0.0.1:{}{}", port, endpoint);
        let auth = Self::make_auth_header(&password);

        let response = self
            .http_client
            .patch(&url)
            .header(AUTHORIZATION, &auth)
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("LCU PATCH failed: {}", e)))?;

        let text = response
            .text()
            .await
            .map_err(|e| AppError::Custom(format!("LCU response read failed: {}", e)))?;

        Ok(text)
    }

    // --- Account Info ---

    pub async fn get_account_info(
        &self,
    ) -> Result<Option<AccountInfo>, AppError> {
        let summoner_json = self
            .lcu_get("/lol-summoner/v1/current-summoner")
            .await?;

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
        let puuid = summoner
            .get("puuid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Get ranked info
        let ranked_json = self
            .lcu_get("/lol-ranked/v1/current-ranked-stats")
            .await
            .unwrap_or_default();
        let ranked: serde_json::Value =
            serde_json::from_str(&ranked_json).unwrap_or_default();

        let mut rank = String::from("Unranked");
        let mut rank_display = String::new();

        if let Some(queues) = ranked.get("queues").and_then(|v| v.as_array()) {
            let preferred_queues = [
                "RANKED_SOLO_5x5",
                "RANKED_FLEX_SR",
                "RANKED_TFT",
            ];
            for queue_type in &preferred_queues {
                if let Some(queue) = queues.iter().find(|q| {
                    q.get("queueType")
                        .and_then(|v| v.as_str())
                        == Some(queue_type)
                }) {
                    let tier = queue
                        .get("tier")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let division = queue
                        .get("division")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if !tier.is_empty()
                        && tier != "NONE"
                        && tier != "NA"
                    {
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

        // Get avatar URL
        let avatar_url = format!(
            "https://ddragon.leagueoflegends.com/cdn/14.1.1/img/profileicon/{}.png",
            profile_icon_id
        );

        // Get Riot ID
        let riot_id = self.get_riot_id().await.unwrap_or_default();

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
        if let Ok(resp) = self.rc_request(reqwest::Method::GET, "/riotclient/region-locale", None).await {
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

    async fn get_riot_id(&self) -> Result<String, AppError> {
        let resp = self
            .lcu_get("/lol-chat/v1/me")
            .await?;
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
        let lockfile = Self::find_rc_lockfile()
            .ok_or_else(|| AppError::Custom("RC lockfile not found".to_string()))?;

        let url = format!("https://127.0.0.1:{}{}", lockfile.port, endpoint);
        let auth = Self::make_auth_header(&lockfile.password);

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
        let text = response
            .text()
            .await
            .map_err(|e| AppError::Custom(format!("RC response read failed: {}", e)))?;

        // Only log non-200 responses to reduce noise
        if !status.is_success() {
            eprintln!("[RC_API] {} → {} | {}", endpoint, status.as_u16(),
                if text.len() > 200 { &text[..200] } else { &text });
        }

        Ok(text)
    }

    /// Logout current account via RC API.
    /// Tries DELETE on both v1 and v2 endpoints — ignores individual errors.
    pub async fn logout_via_rc(&self) -> Result<(), AppError> {
        let _ = self.rc_request(reqwest::Method::DELETE, "/rso-auth/v1/authorization", None).await;
        let _ = self.rc_request(reqwest::Method::DELETE, "/rso-auth/v2/authorizations", None).await;
        Ok(())
    }

    /// Check if a user is currently authorized via RSO (Riot Sign-On).
    pub async fn is_rso_authorized(&self) -> bool {
        let resp = match self
            .rc_request(reqwest::Method::GET, "/rso-auth/v1/authorization", None)
            .await
        {
            Ok(r) => r,
            Err(_) => return false,
        };

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&resp) {
            if json.get("errorCode").is_some() {
                return false;
            }
            if json.get("isAuthorized").and_then(|v| v.as_bool()) == Some(true) {
                return true;
            }
            if let Some(auth) = json.get("authorization") {
                if auth.get("accessToken").and_then(|v| v.as_str()).is_some() {
                    return true;
                }
            }
            if json.get("authorized").and_then(|v| v.as_bool()) == Some(true) {
                return true;
            }
            if json.get("subject").and_then(|v| v.as_str()).map(|s| !s.is_empty()) == Some(true) {
                return true;
            }
            if json.get("currentAccountId").and_then(|v| v.as_u64()).is_some() {
                return true;
            }
        }
        false
    }

    /// Initialize RSO session — required before login_via_rc can work.
    /// POST /rso-auth/v2/authorizations creates the RSO session.
    pub async fn init_rso_session(&self) -> Result<(), AppError> {
        let body = r#"{"clientId":"riot-client","trustLevels":["always_trusted"]}"#;
        let resp = self
            .rc_request(reqwest::Method::POST, "/rso-auth/v2/authorizations", Some(body))
            .await?;

        eprintln!("[RSO_INIT] v2 response: {}",
            if resp.len() > 300 { &resp[..300] } else { &resp });

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&resp) {
            if json.get("errorCode").is_some() {
                // v2 failed, try v1
                let resp_v1 = self
                    .rc_request(reqwest::Method::POST, "/rso-auth/v1/authorization", Some(body))
                    .await?;
                eprintln!("[RSO_INIT] v1 response: {}",
                    if resp_v1.len() > 300 { &resp_v1[..300] } else { &resp_v1 });
                if let Ok(json_v1) = serde_json::from_str::<serde_json::Value>(&resp_v1) {
                    if let Some(ec) = json_v1.get("errorCode").and_then(|v| v.as_str()) {
                        return Err(AppError::Custom(format!("RSO session init failed: {}", ec)));
                    }
                }
            }
        }
        Ok(())
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
                let method = json.get("multifactor")
                    .and_then(|m| m.get("method"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                return Err(AppError::Custom(format!("Login requires 2FA (method: {})", method)));
            }

            // Error
            if let Some(error) = json.get("error").and_then(|v| v.as_str()) {
                return Err(AppError::Custom(format!("Login failed: {} (type: {})", error, resp_type)));
            }
            if let Some(error_code) = json.get("errorCode").and_then(|v| v.as_str()) {
                let message = json.get("message").and_then(|v| v.as_str()).unwrap_or(error_code);
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
        // List of endpoints to try (in order of likelihood)
        let endpoints = [
            ("/product-launcher/v1/products/league_of_legends/patchlines/live", "POST"),
            ("/product-launcher/v1/products/league_of_legends/patchlines/live/launch", "POST"),
            ("/product-launcher/v1/launch", "POST"),
        ];

        let body = r#"{"productId":"league_of_legends","patchlineId":"live"}"#;

        for (endpoint, _) in &endpoints {
            match self.rc_request(reqwest::Method::POST, endpoint, Some(body)).await {
                Ok(r) if !r.contains("errorCode") => return Ok(()),
                _ => continue,
            }
        }

        // Last resort: start League directly via process
        Self::start_league_directly()
    }

    /// Start League Client directly via RiotClientServices.exe with launch args.
    fn start_league_directly() -> Result<(), AppError> {
        #[cfg(windows)]
        {
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
                                .args(["--launch-product=league_of_legends", "--launch-patchline=live"])
                                .spawn()
                                .map_err(|e| AppError::Custom(format!("Failed to launch League: {}", e)))?;
                            return Ok(());
                        }
                    }
                }
            }
            Err(AppError::Custom("Could not launch League of Legends".to_string()))
        }
        #[cfg(not(windows))]
        Err(AppError::Custom("Not supported on this platform".to_string()))
    }

    // --- Wait Helpers ---

    /// Wait for a process by name to appear (polling every 200ms).
    pub fn wait_for_process(name: &str, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if Self::is_process_running(name) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        false
    }

    /// Wait for RC lockfile to appear (polling every 200ms).
    pub fn wait_for_rc_lockfile(timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if Self::find_rc_lockfile().is_some() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        false
    }

    /// Wait for RC API to be responsive (not returning 404 on all endpoints).
    /// This is needed because RC lockfile appears before the HTTP API is ready.
    pub async fn wait_for_rc_api_ready(&self, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            // Try a simple GET — if it returns anything other than 404, API is ready
            match self.rc_request(reqwest::Method::GET, "/rso-auth/v1/authorization", None).await {
                Ok(resp) => {
                    if !resp.contains("RESOURCE_NOT_FOUND") {
                        return true;
                    }
                }
                Err(_) => {} // Connection error — RC not ready yet
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        false
    }

    /// Wait for RSO authorization state to match target (polling every 300ms).
    pub async fn wait_for_rso_state(&self, authorized: bool, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if self.is_rso_authorized().await == authorized {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        false
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

fn platform_to_server(platform: &str) -> String {
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
    }.to_string()
}
