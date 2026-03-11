use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueClientInfo {
    pub install_directory: Option<String>,
    pub lockfile_path: Option<String>,
    pub port: Option<i32>,
    pub password: Option<String>,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    pub league_client_ux_pid: Option<i32>,
    pub command_line: Option<String>,
    pub last_updated_utc: DateTime<Utc>,
}

fn default_protocol() -> String {
    "https".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConnectivityStatus {
    pub is_riot_client_running: bool,
    pub is_league_running: bool,
    pub rc_lockfile_found: bool,
    pub lcu_lockfile_found: bool,
    pub lcu_port: Option<i32>,
    pub lcu_http_ok: bool,
    pub lcu_lockfile_path: Option<String>,
    pub league_install_path: Option<String>,
}

impl Default for ClientConnectivityStatus {
    fn default() -> Self {
        Self {
            is_riot_client_running: false,
            is_league_running: false,
            rc_lockfile_found: false,
            lcu_lockfile_found: false,
            lcu_port: None,
            lcu_http_ok: false,
            lcu_lockfile_path: None,
            league_install_path: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LockfileInfo {
    pub name: String,
    pub pid: u32,
    pub port: u16,
    pub password: String,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CloseBehavior {
    AskEveryTime,
    MinimizeToTray,
    ExitApp,
}

impl Default for CloseBehavior {
    fn default() -> Self {
        Self::AskEveryTime
    }
}
