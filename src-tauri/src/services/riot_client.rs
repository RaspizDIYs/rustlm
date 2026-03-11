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

        // Try RiotClientInstalls.json first
        if let Some(local_app_data) = dirs::data_local_dir() {
            let installs_json = local_app_data
                .join("Riot Games")
                .join("RiotClientInstalls.json");
            if let Ok(content) = fs::read_to_string(&installs_json) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                    // Look for league_of_legends.product_install_full_path
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

    pub fn is_riot_client_running() -> bool {
        #[cfg(windows)]
        {
            let output = Command::new("tasklist")
                .args(["/FI", "IMAGENAME eq RiotClientUx.exe", "/NH"])
                .output();
            match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    stdout.contains("RiotClientUx.exe")
                }
                Err(_) => false,
            }
        }
        #[cfg(not(windows))]
        false
    }

    pub fn is_league_running() -> bool {
        #[cfg(windows)]
        {
            let output = Command::new("tasklist")
                .args(["/FI", "IMAGENAME eq LeagueClientUx.exe", "/NH"])
                .output();
            match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    stdout.contains("LeagueClientUx.exe")
                }
                Err(_) => false,
            }
        }
        #[cfg(not(windows))]
        false
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
            let local_app_data = dirs::data_local_dir()
                .ok_or_else(|| AppError::Custom("Cannot find LocalAppData".to_string()))?;

            let installs_json = local_app_data
                .join("Riot Games")
                .join("RiotClientInstalls.json");

            if installs_json.exists() {
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

        Ok(Some(AccountInfo {
            summoner_name: display_name,
            avatar_url,
            rank,
            rank_display,
            riot_id,
            puuid,
            summoner_level: summoner_level as i32,
        }))
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
}
