#![allow(dead_code)]

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
    pub puuid: String,
    #[serde(default)]
    pub rank_icon_url: String,
    #[serde(default)]
    pub server: String,
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
            puuid: String::new(),
            rank_icon_url: String::new(),
            server: String::new(),
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
    pub puuid: String,
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

// --- Cloud sync models ---

/// Deserialize DateTime<Utc> that may or may not have a timezone suffix.
/// C# backends with MySQL often serialize DateTime without "Z" when Kind=Unspecified.
mod lenient_utc_datetime {
    use chrono::{DateTime, NaiveDateTime, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&date.to_rfc3339())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        // Try parsing with timezone first (standard RFC 3339 / ISO 8601)
        if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
            return Ok(dt.with_timezone(&Utc));
        }
        // Fallback: parse without timezone, assume UTC
        if let Ok(naive) = NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.f") {
            return Ok(naive.and_utc());
        }
        if let Ok(naive) = NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S") {
            return Ok(naive.and_utc());
        }
        Err(serde::de::Error::custom(format!(
            "Cannot parse datetime: '{}'",
            s
        )))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudAccountData {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub note: String,
    #[serde(default = "Utc::now")]
    #[serde(with = "lenient_utc_datetime")]
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
    pub puuid: String,
    #[serde(default)]
    pub rank_icon_url: String,
    #[serde(default)]
    pub server: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudProfilePayload {
    pub accounts: Vec<CloudAccountData>,
    #[serde(with = "lenient_utc_datetime")]
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rune_pages: Option<Vec<crate::models::rune::RunePage>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_settings: Option<crate::models::settings::UpdateSettings>,
}

pub type CloudAccountsPayload = CloudProfilePayload;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncMeta {
    #[serde(with = "lenient_utc_datetime")]
    pub updated_at: DateTime<Utc>,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SyncStatus {
    Idle,
    Syncing,
    Success {
        #[serde(rename = "lastSynced")]
        last_synced: String,
    },
    Error {
        message: String,
    },
    Disconnected,
}

// --- TOTP models ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpSetupInfo {
    pub secret: String,
    pub otpauth_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpSession {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpVerifyResponse {
    pub enabled: bool,
    #[serde(default)]
    pub recovery_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpValidateResponse {
    pub session_token: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpStatusResponse {
    pub enabled: bool,
}
