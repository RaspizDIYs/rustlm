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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoodLuckFriend {
    pub id: String,
    #[serde(default)]
    pub username: String,
    #[serde(alias = "displayName")]
    pub display_name: String,
    #[serde(default, alias = "avatarUrl")]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GoodLuckUserSearchResponse {
    pub users: Vec<GoodLuckFriend>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedAccount {
    pub id: String,
    #[serde(alias = "ownerUserId")]
    pub owner_user_id: String,
    #[serde(alias = "ownerDisplayName")]
    pub owner_display_name: String,
    #[serde(default, alias = "ownerAvatarUrl")]
    pub owner_avatar_url: Option<String>,
    #[serde(alias = "recipientUserId")]
    pub recipient_user_id: String,
    #[serde(alias = "recipientDisplayName")]
    pub recipient_display_name: String,
    pub username: String,
    #[serde(alias = "riotId")]
    pub riot_id: String,
    #[serde(default)]
    pub puuid: String,
    pub server: String,
    #[serde(alias = "createdAt")]
    pub created_at: String,
    #[serde(default, alias = "expiresAt")]
    pub expires_at: Option<String>,
    #[serde(default, alias = "lastAccessedAt")]
    pub last_accessed_at: Option<String>,
    #[serde(default, alias = "accessCount")]
    pub access_count: u32,
    #[serde(default, alias = "cleanupCompletedAt")]
    pub cleanup_completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedAccountsResponse {
    #[serde(alias = "availableToMe")]
    pub available_to_me: Vec<SharedAccount>,
    #[serde(alias = "sharedByMe")]
    pub shared_by_me: Vec<SharedAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareAccountRequest {
    pub recipient_user_id: String,
    pub username: String,
    pub password: String,
    pub riot_id: String,
    pub puuid: String,
    pub server: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SharedAccountCredentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoodLuckFriendRequest {
    pub id: String,
    #[serde(default)]
    pub requester_id: String,
    #[serde(default)]
    pub requester_username: String,
    #[serde(default)]
    pub requester_display_name: String,
    #[serde(default)]
    pub requester_avatar_url: Option<String>,
    #[serde(default)]
    pub created_at: String,
}
