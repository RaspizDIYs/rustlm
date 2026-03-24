#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// User profile from GoodLuck platform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoodLuckUser {
    #[serde(alias = "userId")]
    pub user_id: String,
    #[serde(alias = "displayName")]
    pub display_name: String,
    #[serde(alias = "avatarUrl")]
    pub avatar_url: String,
    #[serde(default, alias = "riotAccounts")]
    pub riot_accounts: Vec<GoodLuckRiotAccount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_avatar_path: Option<String>,
}

/// Riot account metadata from GoodLuck profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoodLuckRiotAccount {
    #[serde(alias = "riotId")]
    pub riot_id: String,
    pub server: String,
    pub rank: String,
}

/// JWT + refresh token pair received from GoodLuck
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoodLuckTokens {
    pub token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

/// Auth response from POST /auth/rustlm/token (and /auth/refresh)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoodLuckAuthResponse {
    pub token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    #[serde(rename = "expiresIn")]
    pub expires_in: u64,
    /// Present on initial login, null on refresh
    #[serde(default)]
    pub user: Option<GoodLuckUser>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Persisted auth state (saved to disk, tokens are DPAPI-encrypted)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedGoodLuckAuth {
    pub encrypted_jwt: String,
    pub encrypted_refresh_token: String,
    pub user_id: String,
    pub display_name: String,
    pub avatar_url: String,
    pub expires_at: String,
    #[serde(default)]
    pub riot_accounts: Vec<GoodLuckRiotAccount>,
}

/// PKCE flow state (kept in memory during auth flow)
#[derive(Debug, Clone)]
pub struct PendingAuthFlow {
    pub state: String,
    pub code_verifier: String,
}

/// Result of account sync operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub created: u32,
    pub updated: u32,
    pub skipped: u32,
}

/// Result of GoodLuck profile import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlImportResult {
    pub imported: u32,
    pub updated: u32,
    pub skipped: u32,
    /// Accounts that were updated: (old_riot_id, new_riot_id)
    pub updated_pairs: Vec<(String, String)>,
}

/// Account metadata sent during sync (NO passwords!)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAccountData {
    #[serde(alias = "riotId")]
    pub riot_id: String,
    pub server: String,
    pub rank: String,
    #[serde(alias = "summonerName")]
    pub summoner_name: String,
}
