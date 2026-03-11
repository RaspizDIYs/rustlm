use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AccountRecord {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub encrypted_password: String,
    #[serde(default)]
    pub note: String,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub avatar_url: String,
    #[serde(default)]
    pub summoner_name: String,
    #[serde(default)]
    pub rank: String,
    #[serde(default)]
    pub rank_display: String,
    #[serde(default)]
    pub riot_id: String,
    #[serde(default)]
    pub rank_icon_url: String,
    #[serde(skip)]
    pub is_selected: bool,
}

impl Default for AccountRecord {
    fn default() -> Self {
        Self {
            username: String::new(),
            encrypted_password: String::new(),
            note: String::new(),
            created_at: Utc::now(),
            avatar_url: String::new(),
            summoner_name: String::new(),
            rank: String::new(),
            rank_display: String::new(),
            riot_id: String::new(),
            rank_icon_url: String::new(),
            is_selected: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct EncryptedExportData {
    #[serde(default = "default_version")]
    pub version: i32,
    #[serde(default = "default_app_name")]
    pub app_name: String,
    #[serde(default = "Utc::now")]
    pub exported_at: DateTime<Utc>,
    #[serde(default)]
    pub encrypted_accounts: String,
    #[serde(default)]
    pub salt: String,
    #[serde(default)]
    pub iv: Option<String>,
}

fn default_version() -> i32 {
    3
}
fn default_app_name() -> String {
    "LolManager".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ExportAccountRecord {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub note: String,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub avatar_url: String,
    #[serde(default)]
    pub summoner_name: String,
    #[serde(default)]
    pub rank: String,
    #[serde(default)]
    pub rank_display: String,
    #[serde(default)]
    pub riot_id: String,
    #[serde(default)]
    pub rank_icon_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LegacyExportAccountRecord {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
}
