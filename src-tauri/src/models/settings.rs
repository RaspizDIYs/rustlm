#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateSettings {
    #[serde(default = "default_true")]
    pub auto_update_enabled: bool,
    #[serde(default = "default_update_channel")]
    pub update_channel: String,
    #[serde(default = "default_check_interval")]
    pub check_interval_hours: i32,
    #[serde(default)]
    pub last_check_time: DateTime<Utc>,
    #[serde(default)]
    pub skip_version: bool,
    #[serde(default)]
    pub skipped_version: String,
    #[serde(default)]
    pub github_token: String,
    #[serde(default = "default_update_mode")]
    pub update_mode: String,
}

fn default_true() -> bool {
    true
}
fn default_update_channel() -> String {
    "stable".to_string()
}
fn default_check_interval() -> i32 {
    24
}
fn default_update_mode() -> String {
    "Velopack".to_string()
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            auto_update_enabled: true,
            update_channel: "stable".to_string(),
            check_interval_hours: 24,
            last_check_time: DateTime::<Utc>::default(),
            skip_version: false,
            skipped_version: String::new(),
            github_token: String::new(),
            update_mode: "Velopack".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RevealSettings {
    #[serde(default)]
    pub is_enabled: bool,
    #[serde(default)]
    pub riot_api_key: String,
    #[serde(default = "default_region")]
    pub selected_region: String,
}

fn default_region() -> String {
    "euw1".to_string()
}

impl Default for RevealSettings {
    fn default() -> Self {
        Self {
            is_enabled: false,
            riot_api_key: String::new(),
            selected_region: "euw1".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LeagueSettings {
    #[serde(default)]
    pub prefer_manual_path: bool,
    pub install_directory: Option<String>,
    pub last_detected_install_directory: Option<String>,
    #[serde(default)]
    pub last_detected_at_utc: DateTime<Utc>,
}

impl Default for LeagueSettings {
    fn default() -> Self {
        Self {
            prefer_manual_path: false,
            install_directory: None,
            last_detected_install_directory: None,
            last_detected_at_utc: DateTime::<Utc>::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogFilters {
    #[serde(default = "default_true")]
    pub show_login: bool,
    #[serde(default = "default_true")]
    pub show_http: bool,
    #[serde(default = "default_true")]
    pub show_ui: bool,
    #[serde(default = "default_true")]
    pub show_process: bool,
    #[serde(default = "default_true")]
    pub show_info: bool,
    #[serde(default = "default_true")]
    pub show_warning: bool,
    #[serde(default = "default_true")]
    pub show_error: bool,
    #[serde(default)]
    pub show_debug: bool,
}

impl Default for LogFilters {
    fn default() -> Self {
        Self {
            show_login: true,
            show_http: true,
            show_ui: true,
            show_process: true,
            show_info: true,
            show_warning: true,
            show_error: true,
            show_debug: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub runtime: String,
    #[serde(default)]
    pub architecture: String,
}

impl SystemInfo {
    pub fn load() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            runtime: format!("Rust {}", env!("CARGO_PKG_VERSION")),
            architecture: std::env::consts::ARCH.to_string(),
        }
    }
}
