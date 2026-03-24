use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64URL;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::error::AppError;
use crate::models::goodluck::{
    GoodLuckAuthResponse, GoodLuckUser, PendingAuthFlow, PersistedGoodLuckAuth,
};
use crate::services::crypto::{dpapi_protect, dpapi_unprotect};

#[derive(serde::Serialize, serde::Deserialize)]
struct GoodluckAvatarMeta {
    url: String,
}

#[cfg(feature = "goodluck-test")]
const GOODLUCK_API_BASE: &str = "https://test.gltournament.ru/api";
#[cfg(not(feature = "goodluck-test"))]
const GOODLUCK_API_BASE: &str = "https://gltournament.ru/api";

#[cfg(feature = "goodluck-test")]
const GOODLUCK_WEB_BASE: &str = "https://test.gltournament.ru";
#[cfg(not(feature = "goodluck-test"))]
const GOODLUCK_WEB_BASE: &str = "https://gltournament.ru";
const CLIENT_ID: &str = "rustlm";
const REDIRECT_URI: &str = "rustlm://auth/callback";

fn absolutize_goodluck_media_url(url: &str) -> String {
    let u = url.trim();
    if u.is_empty() {
        return String::new();
    }
    let lower = u.to_ascii_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("data:")
        || lower.starts_with("blob:")
    {
        return u.to_string();
    }
    if u.starts_with("//") {
        return format!("https:{u}");
    }
    let base = GOODLUCK_WEB_BASE.trim_end_matches('/');
    if u.starts_with('/') {
        format!("{base}{u}")
    } else {
        format!("{base}/{u}")
    }
}

pub struct GoodLuckAuthService {
    http_client: reqwest::Client,
    auth_state: Mutex<Option<AuthState>>,
    pending_flow: Mutex<Option<PendingAuthFlow>>,
    refresh_lock: tokio::sync::Mutex<()>,
    tokens_path: PathBuf,
}

#[derive(Debug, Clone)]
struct AuthState {
    user: GoodLuckUser,
    jwt: String,
    refresh_token: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

impl GoodLuckAuthService {
    pub fn new() -> Self {
        let roaming_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LolManager");

        fs::create_dir_all(&roaming_dir).ok();

        let tokens_path = roaming_dir.join("goodluck_auth.json");

        let service = Self {
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            auth_state: Mutex::new(None),
            pending_flow: Mutex::new(None),
            refresh_lock: tokio::sync::Mutex::new(()),
            tokens_path,
        };

        // Try to restore auth from disk
        service.try_restore_auth();
        service
    }

    /// Generate PKCE verifier + challenge and return the authorization URL
    pub fn start_auth_flow(&self) -> Result<String, AppError> {
        // Generate code_verifier (32 random bytes -> base64url)
        let mut verifier_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut verifier_bytes);
        let code_verifier = BASE64URL.encode(verifier_bytes);

        // code_challenge = SHA256(code_verifier), base64url
        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let challenge_hash = hasher.finalize();
        let code_challenge = BASE64URL.encode(challenge_hash);

        // Random state
        let mut state_bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut state_bytes);
        let state = BASE64URL.encode(state_bytes);

        // Save pending flow
        {
            let mut pending = self.pending_flow.lock().unwrap();
            *pending = Some(PendingAuthFlow {
                state: state.clone(),
                code_verifier,
            });
        }

        // Build authorization URL
        let url = format!(
            "{}/auth/rustlm?client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state={}",
            GOODLUCK_WEB_BASE,
            CLIENT_ID,
            urlencode(REDIRECT_URI),
            code_challenge,
            state
        );

        Ok(url)
    }

    /// Handle the deep link callback: exchange auth code for tokens
    pub async fn handle_callback(
        &self,
        code: &str,
        state: &str,
    ) -> Result<GoodLuckUser, AppError> {
        // Verify state and get code_verifier
        let code_verifier = {
            let mut pending = self.pending_flow.lock().unwrap();
            let flow = pending
                .take()
                .ok_or_else(|| AppError::Custom("No pending auth flow".to_string()))?;

            if flow.state != state {
                return Err(AppError::Custom(
                    "State mismatch — possible CSRF attack".to_string(),
                ));
            }
            flow.code_verifier
        };

        // Exchange code for tokens
        let resp = self
            .http_client
            .post(&format!("{}/auth/rustlm/token", GOODLUCK_API_BASE))
            .json(&serde_json::json!({
                "code": code,
                "codeVerifier": code_verifier,
                "clientId": CLIENT_ID
            }))
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("Token exchange failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Custom(format!(
                "Token exchange returned {}: {}",
                status, body
            )));
        }

        let auth_response: GoodLuckAuthResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Custom(format!("Failed to parse auth response: {}", e)))?;

        let mut user = auth_response.user
            .ok_or_else(|| AppError::Custom("No user in auth response".to_string()))?;

        user.avatar_url = absolutize_goodluck_media_url(&user.avatar_url);
        let _ = self.attach_cached_avatar(&mut user).await;

        let expires_at =
            chrono::Utc::now() + chrono::Duration::seconds(auth_response.expires_in as i64);

        // Save auth state in memory
        {
            let mut auth = self.auth_state.lock().unwrap();
            *auth = Some(AuthState {
                user: user.clone(),
                jwt: auth_response.token.clone(),
                refresh_token: auth_response.refresh_token.clone(),
                expires_at,
            });
        }

        // Persist tokens to disk (DPAPI-encrypted)
        self.write_auth_to_disk(
            &auth_response.token,
            &auth_response.refresh_token,
            &user,
            expires_at,
        )?;

        Ok(user)
    }

    /// Get the current authenticated user, or None
    pub fn get_user(&self) -> Option<GoodLuckUser> {
        let auth = self.auth_state.lock().unwrap();
        auth.as_ref().map(|a| a.user.clone())
    }

    /// Check if we have a valid (non-expired) auth state
    pub fn is_connected(&self) -> bool {
        let auth = self.auth_state.lock().unwrap();
        // Return true if we have auth state at all (access token may be expired,
        // but get_token() will auto-refresh using the refresh token)
        auth.is_some()
    }

    /// Get the current JWT for API calls (auto-refresh if expired)
    pub async fn get_token(&self) -> Result<String, AppError> {
        // Check if token is expired
        let needs_refresh = {
            let auth = self.auth_state.lock().unwrap();
            match &*auth {
                Some(state) => chrono::Utc::now() >= state.expires_at,
                None => return Err(AppError::Custom("Not authenticated".to_string())),
            }
        };

        if needs_refresh {
            // Serialize refresh attempts so only one runs at a time.
            // If another caller already refreshed, we'll just read the new token.
            let _guard = self.refresh_lock.lock().await;
            let still_needs_refresh = {
                let auth = self.auth_state.lock().unwrap();
                match &*auth {
                    Some(state) => chrono::Utc::now() >= state.expires_at,
                    None => return Err(AppError::Custom("Not authenticated".to_string())),
                }
            };
            if still_needs_refresh {
                self.refresh_token().await?;
            }
        }

        let auth = self.auth_state.lock().unwrap();
        Ok(auth.as_ref().unwrap().jwt.clone())
    }

    /// Logout: revoke refresh token on server, clear local state
    pub async fn logout(&self) -> Result<(), AppError> {
        // Try to revoke on server (best effort)
        let token = {
            let auth = self.auth_state.lock().unwrap();
            auth.as_ref().map(|a| a.jwt.clone())
        };

        if let Some(jwt) = token {
            let _ = self
                .http_client
                .post(&format!("{}/auth/rustlm/revoke", GOODLUCK_API_BASE))
                .bearer_auth(&jwt)
                .send()
                .await;
        }

        // Clear in-memory state
        {
            let mut auth = self.auth_state.lock().unwrap();
            *auth = None;
        }

        // Delete persisted tokens
        if self.tokens_path.exists() {
            fs::remove_file(&self.tokens_path).ok();
        }

        self.clear_avatar_cache();

        Ok(())
    }

    /// Fetch latest profile from GoodLuck (`GET /auth/rustlm/me`), update nick / riot list / avatar cache.
    pub async fn refresh_profile_from_server(&self) -> Result<GoodLuckUser, AppError> {
        let url = format!("{}/auth/rustlm/me", GOODLUCK_API_BASE);

        let mut user: GoodLuckUser = {
            let mut last_err = None;
            let mut result = None;
            for attempt in 0..2u8 {
                let token = self.get_token().await?;
                let resp = self
                    .http_client
                    .get(&url)
                    .bearer_auth(&token)
                    .send()
                    .await
                    .map_err(|e| AppError::Custom(format!("Profile refresh failed: {}", e)))?;

                if resp.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                    let _ = self.refresh_token().await;
                    continue;
                }

                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    last_err = Some(format!("Profile refresh returned {}: {}", status, body));
                    break;
                }

                result = Some(
                    resp.json::<GoodLuckUser>()
                        .await
                        .map_err(|e| AppError::Custom(format!("Failed to parse profile: {}", e)))?
                );
                break;
            }
            match result {
                Some(u) => u,
                None => return Err(AppError::Custom(last_err.unwrap_or_else(|| "Profile refresh failed".to_string()))),
            }
        };

        user.avatar_url = absolutize_goodluck_media_url(&user.avatar_url);
        let _ = self.attach_cached_avatar(&mut user).await;

        let (jwt, refresh, expires_at) = {
            let mut auth = self.auth_state.lock().unwrap();
            let state = auth
                .as_mut()
                .ok_or_else(|| AppError::Custom("Not authenticated".to_string()))?;
            state.user.display_name = user.display_name.clone();
            state.user.avatar_url = user.avatar_url.clone();
            state.user.riot_accounts = user.riot_accounts.clone();
            state.user.local_avatar_path = user.local_avatar_path.clone();
            (
                state.jwt.clone(),
                state.refresh_token.clone(),
                state.expires_at,
            )
        };

        self.write_auth_to_disk(&jwt, &refresh, &user, expires_at)?;

        Ok(user)
    }

    /// Sync account metadata to GoodLuck (NO passwords)
    pub async fn sync_accounts(
        &self,
        accounts: Vec<crate::models::goodluck::SyncAccountData>,
    ) -> Result<crate::models::goodluck::SyncResult, AppError> {
        let payload: Vec<serde_json::Value> = accounts
            .iter()
            .map(|a| {
                serde_json::json!({
                    "riotId": a.riot_id,
                    "server": a.server,
                    "rank": a.rank,
                    "summonerName": a.summoner_name,
                })
            })
            .collect();

        let url = format!("{}/auth/rustlm/sync-accounts", GOODLUCK_API_BASE);

        for attempt in 0..2u8 {
            let token = self.get_token().await?;
            let resp = self
                .http_client
                .post(&url)
                .bearer_auth(&token)
                .json(&payload)
                .send()
                .await
                .map_err(|e| AppError::Custom(format!("Sync failed: {}", e)))?;

            if resp.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.refresh_token().await?;
                continue;
            }

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(AppError::Custom(format!(
                    "Sync returned {}: {}",
                    status, body
                )));
            }

            return resp.json()
                .await
                .map_err(|e| AppError::Custom(format!("Failed to parse sync response: {}", e)));
        }

        Err(AppError::Custom("Sync failed after retry".to_string()))
    }

    /// Delete all synced RustLM data from GoodLuck server
    pub async fn delete_server_data(&self) -> Result<(), AppError> {
        let url = format!("{}/auth/rustlm/sync-accounts", GOODLUCK_API_BASE);

        for attempt in 0..2u8 {
            let token = self.get_token().await?;
            let resp = self
                .http_client
                .delete(&url)
                .bearer_auth(&token)
                .send()
                .await
                .map_err(|e| AppError::Custom(format!("Delete server data failed: {}", e)))?;

            if resp.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.refresh_token().await?;
                continue;
            }

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(AppError::Custom(format!(
                    "Delete returned {}: {}",
                    status, body
                )));
            }

            return Ok(());
        }

        Err(AppError::Custom("Delete failed after retry".to_string()))
    }

    /// Get list of accounts currently synced on GoodLuck
    pub async fn get_synced_accounts(
        &self,
    ) -> Result<Vec<crate::models::goodluck::SyncAccountData>, AppError> {
        let token = self.get_token().await?;

        let resp = self
            .http_client
            .get(&format!("{}/auth/rustlm/sync-accounts", GOODLUCK_API_BASE))
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("Get synced accounts failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Custom(format!(
                "Get synced accounts returned {}: {}",
                status, body
            )));
        }

        resp.json()
            .await
            .map_err(|e| AppError::Custom(format!("Failed to parse synced accounts: {}", e)))
    }

    // --- Private helpers ---

    fn profile_data_dir(&self) -> PathBuf {
        self.tokens_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    fn avatar_bytes_path(&self) -> PathBuf {
        self.profile_data_dir().join("goodluck_avatar.cache")
    }

    fn avatar_meta_path(&self) -> PathBuf {
        self.profile_data_dir().join("goodluck_avatar.meta.json")
    }

    fn clear_avatar_cache(&self) {
        let _ = fs::remove_file(self.avatar_bytes_path());
        let _ = fs::remove_file(self.avatar_meta_path());
    }

    fn read_avatar_meta_url(&self) -> Option<String> {
        let s = fs::read_to_string(self.avatar_meta_path()).ok()?;
        serde_json::from_str::<GoodluckAvatarMeta>(&s)
            .ok()
            .map(|m| m.url)
    }

    fn bytes_to_data_url(bytes: &[u8]) -> String {
        use base64::engine::general_purpose::STANDARD as B64;
        use base64::Engine as _;
        let mime = if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            "image/png"
        } else if bytes.starts_with(&[0xFF, 0xD8]) {
            "image/jpeg"
        } else if bytes.starts_with(b"GIF") {
            "image/gif"
        } else if bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
            "image/webp"
        } else {
            "image/png"
        };
        format!("data:{};base64,{}", mime, B64.encode(bytes))
    }

    fn local_avatar_data_url_if_cache_matches(&self, remote_url: &str) -> Option<String> {
        let u = remote_url.trim();
        if u.is_empty() {
            return None;
        }
        if self.read_avatar_meta_url().as_deref() != Some(u) {
            return None;
        }
        let p = self.avatar_bytes_path();
        let bytes = fs::read(&p).ok()?;
        if bytes.is_empty() {
            return None;
        }
        Some(Self::bytes_to_data_url(&bytes))
    }

    async fn cache_avatar_if_needed(&self, remote_url: &str) -> Result<Option<String>, AppError> {
        let url = remote_url.trim();
        if url.is_empty() {
            self.clear_avatar_cache();
            return Ok(None);
        }

        if let Some(data_url) = self.local_avatar_data_url_if_cache_matches(url) {
            return Ok(Some(data_url));
        }

        let response = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("Avatar download failed: {}", e)))?;

        if !response.status().is_success() {
            return Ok(self.local_avatar_data_url_if_cache_matches(url));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| AppError::Custom(format!("Avatar body: {}", e)))?;

        if bytes.is_empty() {
            return Ok(None);
        }

        fs::create_dir_all(self.profile_data_dir()).ok();
        fs::write(self.avatar_bytes_path(), &bytes).map_err(|e| {
            AppError::Custom(format!("Failed to write avatar cache: {}", e))
        })?;

        let meta = GoodluckAvatarMeta {
            url: url.to_string(),
        };
        fs::write(
            self.avatar_meta_path(),
            serde_json::to_string(&meta).map_err(|e| AppError::Custom(e.to_string()))?,
        )
        .map_err(|e| AppError::Custom(format!("Avatar meta write: {}", e)))?;

        Ok(Some(Self::bytes_to_data_url(&bytes)))
    }

    async fn attach_cached_avatar(&self, user: &mut GoodLuckUser) -> Result<(), AppError> {
        user.local_avatar_path = self.cache_avatar_if_needed(&user.avatar_url).await?;
        Ok(())
    }

    fn write_auth_to_disk(
        &self,
        jwt: &str,
        refresh_token: &str,
        user: &GoodLuckUser,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AppError> {
        let encrypted_jwt = dpapi_protect(jwt.as_bytes())?;
        let encrypted_refresh = dpapi_protect(refresh_token.as_bytes())?;

        let avatar_url = absolutize_goodluck_media_url(&user.avatar_url);
        let persisted = PersistedGoodLuckAuth {
            encrypted_jwt,
            encrypted_refresh_token: encrypted_refresh,
            user_id: user.user_id.clone(),
            display_name: user.display_name.clone(),
            avatar_url,
            expires_at: expires_at.to_rfc3339(),
            riot_accounts: user.riot_accounts.clone(),
        };

        let content = serde_json::to_string_pretty(&persisted)?;
        fs::write(&self.tokens_path, content)?;
        Ok(())
    }

    fn try_restore_auth(&self) {
        if !self.tokens_path.exists() {
            return;
        }

        let content = match fs::read_to_string(&self.tokens_path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let persisted: PersistedGoodLuckAuth = match serde_json::from_str(&content) {
            Ok(p) => p,
            Err(_) => return,
        };

        let jwt = match dpapi_unprotect(&persisted.encrypted_jwt) {
            Ok(j) => j,
            Err(_) => return,
        };

        let refresh_token = match dpapi_unprotect(&persisted.encrypted_refresh_token) {
            Ok(r) => r,
            Err(_) => return,
        };

        let expires_at = match chrono::DateTime::parse_from_rfc3339(&persisted.expires_at) {
            Ok(dt) => dt.with_timezone(&chrono::Utc),
            Err(_) => return,
        };

        let abs_avatar = absolutize_goodluck_media_url(&persisted.avatar_url);
        let local_avatar = self.local_avatar_data_url_if_cache_matches(&abs_avatar);

        let mut auth = self.auth_state.lock().unwrap();
        *auth = Some(AuthState {
            user: GoodLuckUser {
                user_id: persisted.user_id,
                display_name: persisted.display_name,
                avatar_url: abs_avatar,
                riot_accounts: persisted.riot_accounts,
                local_avatar_path: local_avatar,
            },
            jwt,
            refresh_token,
            expires_at,
        });
    }

    async fn refresh_token(&self) -> Result<(), AppError> {
        let refresh_token = {
            let auth = self.auth_state.lock().unwrap();
            auth.as_ref()
                .map(|a| a.refresh_token.clone())
                .ok_or_else(|| AppError::Custom("Not authenticated".to_string()))?
        };

        let resp = self
            .http_client
            .post(&format!("{}/auth/refresh", GOODLUCK_API_BASE))
            .json(&serde_json::json!({
                "refreshToken": refresh_token
            }))
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("Token refresh failed: {}", e)))?;

        if !resp.status().is_success() {
            // Refresh token is invalid — clear auth
            {
                let mut auth = self.auth_state.lock().unwrap();
                *auth = None;
            }
            if self.tokens_path.exists() {
                fs::remove_file(&self.tokens_path).ok();
            }
            return Err(AppError::Custom(
                "Session expired. Please login again.".to_string(),
            ));
        }

        let auth_response: GoodLuckAuthResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Custom(format!("Failed to parse refresh response: {}", e)))?;

        let expires_at =
            chrono::Utc::now() + chrono::Duration::seconds(auth_response.expires_in as i64);

        // Refresh response has user=null — preserve existing user data
        let user = {
            let mut auth = self.auth_state.lock().unwrap();
            let state = auth
                .as_mut()
                .ok_or_else(|| AppError::Custom("Not authenticated".to_string()))?;
            state.jwt = auth_response.token.clone();
            state.refresh_token = auth_response.refresh_token.clone();
            state.expires_at = expires_at;
            state.user.clone()
        };

        // Update persisted tokens with existing user
        self.write_auth_to_disk(
            &auth_response.token,
            &auth_response.refresh_token,
            &user,
            expires_at,
        )?;

        Ok(())
    }
}

// --- Cloud accounts API ---

impl GoodLuckAuthService {
    pub async fn upload_cloud_profile(
        &self,
        profile: crate::models::account::CloudProfilePayload,
        totp_session: Option<&str>,
    ) -> Result<crate::models::account::CloudSyncMeta, AppError> {
        let url = format!("{}/auth/rustlm/cloud-accounts", GOODLUCK_API_BASE);

        for attempt in 0..2u8 {
            let token = self.get_token().await?;
            let mut req = self.http_client.put(&url).bearer_auth(&token).json(&profile);
            if let Some(session) = totp_session {
                req = req.header("X-TOTP-Session", session);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| AppError::Custom(format!("Cloud upload failed: {}", e)))?;

            if resp.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.refresh_token().await?;
                continue;
            }

            if resp.status() == reqwest::StatusCode::FORBIDDEN {
                return Err(AppError::Custom("totp_required".to_string()));
            }

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                if body.to_ascii_lowercase().contains("totp_required") {
                    return Err(AppError::Custom("totp_required".to_string()));
                }
                return Err(AppError::Custom(format!(
                    "Cloud upload returned {}: {}",
                    status, body
                )));
            }

            return resp.json().await.map_err(|e| {
                AppError::Custom(format!("Failed to parse cloud upload response: {}", e))
            });
        }

        Err(AppError::Custom("Cloud upload failed after retry".to_string()))
    }

    pub async fn download_cloud_accounts(
        &self,
        totp_session: Option<&str>,
    ) -> Result<Option<crate::models::account::CloudProfilePayload>, AppError> {
        let url = format!("{}/auth/rustlm/cloud-accounts", GOODLUCK_API_BASE);

        for attempt in 0..2u8 {
            let token = self.get_token().await?;
            let mut req = self.http_client.get(&url).bearer_auth(&token);
            if let Some(session) = totp_session {
                req = req.header("X-TOTP-Session", session);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| AppError::Custom(format!("Cloud download failed: {}", e)))?;

            if resp.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.refresh_token().await?;
                continue;
            }

            if resp.status() == reqwest::StatusCode::NOT_FOUND {
                return Ok(None);
            }

            if resp.status() == reqwest::StatusCode::FORBIDDEN {
                return Err(AppError::Custom("totp_required".to_string()));
            }

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                if body.to_ascii_lowercase().contains("totp_required") {
                    return Err(AppError::Custom("totp_required".to_string()));
                }
                return Err(AppError::Custom(format!(
                    "Cloud download returned {}: {}",
                    status, body
                )));
            }

            let body_text = resp.text().await.map_err(|e| {
                AppError::Custom(format!("Failed to read cloud accounts response: {}", e))
            })?;
            let payload: crate::models::account::CloudProfilePayload =
                serde_json::from_str(&body_text).map_err(|e| {
                    log::error!("Cloud profile response body: {}", body_text);
                    AppError::Custom(format!("Failed to parse cloud profile: {}", e))
                })?;
            return Ok(Some(payload));
        }

        Err(AppError::Custom(
            "Cloud download failed after retry".to_string(),
        ))
    }

    pub async fn delete_cloud_accounts(
        &self,
        totp_session: Option<&str>,
    ) -> Result<(), AppError> {
        let token = self.get_token().await?;
        let mut req = self
            .http_client
            .delete(&format!("{}/auth/rustlm/cloud-accounts", GOODLUCK_API_BASE))
            .bearer_auth(&token);
        if let Some(session) = totp_session {
            req = req.header("X-TOTP-Session", session);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("Cloud delete failed: {}", e)))?;

        if resp.status() == reqwest::StatusCode::FORBIDDEN {
            return Err(AppError::Custom("totp_required".to_string()));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if body.to_ascii_lowercase().contains("totp_required") {
                return Err(AppError::Custom("totp_required".to_string()));
            }
            return Err(AppError::Custom(format!(
                "Cloud delete returned {}: {}",
                status, body
            )));
        }

        Ok(())
    }

    // --- TOTP 2FA API ---

    pub async fn totp_setup(&self) -> Result<crate::models::account::TotpSetupInfo, AppError> {
        let token = self.get_token().await?;
        let resp = self
            .http_client
            .post(&format!("{}/auth/rustlm/2fa/setup", GOODLUCK_API_BASE))
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("TOTP setup failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Custom(format!(
                "TOTP setup returned {}: {}",
                status, body
            )));
        }

        resp.json()
            .await
            .map_err(|e| AppError::Custom(format!("Failed to parse TOTP setup: {}", e)))
    }

    pub async fn totp_confirm(&self, code: &str) -> Result<crate::models::account::TotpVerifyResponse, AppError> {
        let token = self.get_token().await?;
        let resp = self
            .http_client
            .post(&format!("{}/auth/rustlm/2fa/verify", GOODLUCK_API_BASE))
            .bearer_auth(&token)
            .json(&serde_json::json!({ "code": code }))
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("TOTP verify failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Custom(format!(
                "TOTP verify returned {}: {}",
                status, body
            )));
        }

        resp.json()
            .await
            .map_err(|e| AppError::Custom(format!("Failed to parse TOTP verify: {}", e)))
    }

    pub async fn totp_disable(&self, code: &str) -> Result<(), AppError> {
        let token = self.get_token().await?;
        let resp = self
            .http_client
            .post(&format!("{}/auth/rustlm/2fa/disable", GOODLUCK_API_BASE))
            .bearer_auth(&token)
            .json(&serde_json::json!({ "code": code }))
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("TOTP disable failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Custom(format!(
                "TOTP disable returned {}: {}",
                status, body
            )));
        }

        Ok(())
    }

    pub async fn totp_status(&self) -> Result<bool, AppError> {
        let token = self.get_token().await?;
        let resp = self
            .http_client
            .get(&format!("{}/auth/rustlm/2fa/status", GOODLUCK_API_BASE))
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("TOTP status check failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Custom(format!(
                "TOTP status returned {}: {}",
                status, body
            )));
        }

        let data: crate::models::account::TotpStatusResponse =
            resp.json().await.map_err(|e| {
                AppError::Custom(format!("Failed to parse TOTP status: {}", e))
            })?;
        Ok(data.enabled)
    }

    pub async fn totp_validate(
        &self,
        code: &str,
    ) -> Result<crate::models::account::TotpSession, AppError> {
        let token = self.get_token().await?;
        let resp = self
            .http_client
            .post(&format!("{}/auth/rustlm/2fa/validate", GOODLUCK_API_BASE))
            .bearer_auth(&token)
            .json(&serde_json::json!({ "code": code }))
            .send()
            .await
            .map_err(|e| AppError::Custom(format!("TOTP validate failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Custom(format!(
                "TOTP validate returned {}: {}",
                status, body
            )));
        }

        let data: crate::models::account::TotpValidateResponse =
            resp.json().await.map_err(|e| {
                AppError::Custom(format!("Failed to parse TOTP validate: {}", e))
            })?;

        Ok(crate::models::account::TotpSession {
            token: data.session_token,
            expires_at: chrono::Utc::now()
                + chrono::Duration::seconds(data.expires_in as i64),
        })
    }
}

fn urlencode(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}
