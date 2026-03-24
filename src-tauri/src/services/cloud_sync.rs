use std::sync::Arc;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use tauri::{Emitter, Manager};

use crate::error::AppError;
use crate::cloud_profile_apply;
use crate::models::account::{CloudAccountData, CloudProfilePayload, SyncStatus, TotpSession};
use crate::services::accounts_storage::AccountsStorage;
use crate::services::goodluck_auth::GoodLuckAuthService;
use crate::services::rune_pages_storage::RunePagesStorage;
use crate::services::settings::SettingsService;

fn classify_cloud_sync_error_message(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("totp_required") {
        return "totp_required".to_string();
    }
    if raw.contains("401")
        && (lower.contains("cloud upload returned")
            || lower.contains("cloud download returned")
            || lower.contains("cloud delete returned"))
    {
        return "goodluck_reauth".to_string();
    }
    raw.to_string()
}

pub struct CloudSyncService {
    accounts: Arc<AccountsStorage>,
    settings: Arc<SettingsService>,
    rune_pages: Arc<RunePagesStorage>,
    goodluck: Arc<GoodLuckAuthService>,
    totp_session: Mutex<Option<TotpSession>>,
    totp_expiry_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    sync_status: Mutex<SyncStatus>,
    last_synced_at: Mutex<Option<DateTime<Utc>>>,
    sync_lock: tokio::sync::Mutex<()>,
    debounce_notify: tokio::sync::Notify,
    app_handle: Mutex<Option<tauri::AppHandle>>,
}

impl CloudSyncService {
    pub fn new(
        accounts: Arc<AccountsStorage>,
        settings: Arc<SettingsService>,
        rune_pages: Arc<RunePagesStorage>,
        goodluck: Arc<GoodLuckAuthService>,
    ) -> Self {
        Self {
            accounts,
            settings,
            rune_pages,
            goodluck,
            totp_session: Mutex::new(None),
            totp_expiry_task: Mutex::new(None),
            sync_status: Mutex::new(SyncStatus::Disconnected),
            last_synced_at: Mutex::new(None),
            sync_lock: tokio::sync::Mutex::new(()),
            debounce_notify: tokio::sync::Notify::new(),
            app_handle: Mutex::new(None),
        }
    }

    pub fn set_app_handle(&self, handle: tauri::AppHandle) {
        let mut g = self.app_handle.lock().unwrap();
        *g = Some(handle);
    }

    fn emit_cloud_sync_complete(&self) {
        let g = self.app_handle.lock().unwrap();
        if let Some(h) = g.as_ref() {
            let _ = h.emit("cloud-sync-complete", ());
        }
    }

    fn emit_cloud_totp_required(&self) {
        let g = self.app_handle.lock().unwrap();
        if let Some(h) = g.as_ref() {
            let _ = h.emit("cloud-totp-required", ());
        }
    }

    pub fn get_status(&self) -> SyncStatus {
        self.sync_status.lock().unwrap().clone()
    }

    fn set_status(&self, status: SyncStatus) {
        let mut s = self.sync_status.lock().unwrap();
        *s = status;
    }

    fn abort_totp_expiry_task(&self) {
        if let Some(h) = self.totp_expiry_task.lock().unwrap().take() {
            h.abort();
        }
    }

    pub fn set_totp_session(&self, session: TotpSession) {
        self.abort_totp_expiry_task();
        let mut s = self.totp_session.lock().unwrap();
        *s = Some(session);
    }

    pub fn clear_cloud_totp_session(&self) {
        self.abort_totp_expiry_task();
        let mut s = self.totp_session.lock().unwrap();
        *s = None;
    }

    pub fn schedule_totp_expiry_notification(self: Arc<Self>) {
        let expires_at = {
            let s = self.totp_session.lock().unwrap();
            s.as_ref().map(|x| x.expires_at)
        };
        let Some(expires_at) = expires_at else {
            return;
        };

        self.abort_totp_expiry_task();

        let this = Arc::clone(&self);
        let h = tokio::spawn(async move {
            let now = Utc::now();
            let dur = if expires_at > now {
                (expires_at - now)
                    .to_std()
                    .unwrap_or(std::time::Duration::ZERO)
            } else {
                std::time::Duration::ZERO
            };
            tokio::time::sleep(dur).await;
            let should_notify = {
                let s = this.totp_session.lock().unwrap();
                match &*s {
                    Some(sess) if Utc::now() >= sess.expires_at => true,
                    _ => false,
                }
            };
            if should_notify {
                {
                    let mut s = this.totp_session.lock().unwrap();
                    if matches!(&*s, Some(sess) if Utc::now() >= sess.expires_at) {
                        *s = None;
                    }
                }
                if this.goodluck.is_connected() {
                    this.emit_cloud_totp_required();
                }
            }
        });
        *self.totp_expiry_task.lock().unwrap() = Some(h);
    }

    pub fn on_goodluck_logout(&self) {
        self.abort_totp_expiry_task();
        {
            let mut s = self.totp_session.lock().unwrap();
            *s = None;
        }
        self.set_status(SyncStatus::Disconnected);
        self.emit_cloud_sync_complete();
    }

    fn get_valid_totp_session(&self) -> Option<String> {
        let s = self.totp_session.lock().unwrap();
        match &*s {
            Some(session) if Utc::now() < session.expires_at => Some(session.token.clone()),
            _ => None,
        }
    }

    pub fn get_valid_totp_session_public(&self) -> Option<String> {
        self.get_valid_totp_session()
    }

    #[allow(dead_code)]
    pub fn needs_totp(&self) -> bool {
        self.get_valid_totp_session().is_none()
    }

    fn build_cloud_accounts(&self) -> Vec<CloudAccountData> {
        self.accounts
            .load_all_with_passwords()
            .into_iter()
            .map(|(record, password)| CloudAccountData {
                username: record.username,
                password,
                note: record.note,
                created_at: record.created_at,
                avatar_url: record.avatar_url,
                summoner_name: record.summoner_name,
                rank: record.rank,
                rank_display: record.rank_display,
                riot_id: record.riot_id,
                puuid: record.puuid,
                rank_icon_url: record.rank_icon_url,
                server: record.server,
            })
            .collect()
    }

    fn build_profile_payload(&self) -> CloudProfilePayload {
        CloudProfilePayload {
            accounts: self.build_cloud_accounts(),
            updated_at: Utc::now(),
            settings: Some(self.settings.export_settings_json_map()),
            rune_pages: Some(self.rune_pages.load_all()),
            update_settings: Some(self.settings.load_update_settings()),
        }
    }

    pub async fn push(&self) -> Result<(), AppError> {
        let _guard = self.sync_lock.lock().await;
        self.push_inner().await
    }

    pub async fn pull(&self, app: &crate::state::AppState) -> Result<usize, AppError> {
        let _guard = self.sync_lock.lock().await;
        self.pull_inner(app).await
    }

    pub async fn sync(&self, app: &crate::state::AppState) -> Result<(), AppError> {
        let _guard = self.sync_lock.lock().await;
        self.pull_inner(app).await?;
        self.push_inner().await?;
        Ok(())
    }

    async fn push_inner(&self) -> Result<(), AppError> {
        if !self.goodluck.is_connected() {
            self.set_status(SyncStatus::Disconnected);
            return Err(AppError::Custom("Not connected to GoodLuck".to_string()));
        }

        self.set_status(SyncStatus::Syncing);

        let profile = self.build_profile_payload();
        let session = self.get_valid_totp_session();

        match self
            .goodluck
            .upload_cloud_profile(profile, session.as_deref())
            .await
        {
            Ok(_meta) => {
                let now = Utc::now();
                *self.last_synced_at.lock().unwrap() = Some(now);
                self.set_status(SyncStatus::Success {
                    last_synced: now.to_rfc3339(),
                });
                Ok(())
            }
            Err(e) => {
                let msg = classify_cloud_sync_error_message(&e.to_string());
                self.set_status(SyncStatus::Error {
                    message: msg.clone(),
                });
                if msg == "totp_required" {
                    self.clear_cloud_totp_session();
                    self.emit_cloud_totp_required();
                }
                Err(e)
            }
        }
    }

    async fn pull_inner(&self, app: &crate::state::AppState) -> Result<usize, AppError> {
        if !self.goodluck.is_connected() {
            self.set_status(SyncStatus::Disconnected);
            return Err(AppError::Custom("Not connected to GoodLuck".to_string()));
        }

        self.set_status(SyncStatus::Syncing);

        let session = self.get_valid_totp_session();

        match self
            .goodluck
            .download_cloud_accounts(session.as_deref())
            .await
        {
            Ok(Some(payload)) => {
                let imported = cloud_profile_apply::apply_cloud_profile(app, &payload).await?;
                let now = Utc::now();
                *self.last_synced_at.lock().unwrap() = Some(now);
                self.set_status(SyncStatus::Success {
                    last_synced: now.to_rfc3339(),
                });
                Ok(imported)
            }
            Ok(None) => {
                self.set_status(SyncStatus::Idle);
                Ok(0)
            }
            Err(e) => {
                let msg = classify_cloud_sync_error_message(&e.to_string());
                self.set_status(SyncStatus::Error {
                    message: msg.clone(),
                });
                if msg == "totp_required" {
                    self.clear_cloud_totp_session();
                    self.emit_cloud_totp_required();
                }
                Err(e)
            }
        }
    }

    pub async fn delete_cloud_data(&self, totp_code: Option<&str>) -> Result<(), AppError> {
        let _guard = self.sync_lock.lock().await;
        self.goodluck.delete_cloud_accounts(totp_code).await?;
        *self.last_synced_at.lock().unwrap() = None;
        self.set_status(SyncStatus::Idle);
        Ok(())
    }

    pub fn notify_change(&self) {
        self.debounce_notify.notify_one();
    }

    pub fn start_debounce_loop(self: &Arc<Self>, app: &tauri::AppHandle) {
        let this = Arc::clone(self);
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                this.debounce_notify.notified().await;
                tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
                if !this.goodluck.is_connected() {
                    continue;
                }
                let state = handle.state::<crate::state::AppState>();
                let _ = this.sync(&*state).await;
                this.emit_cloud_sync_complete();
            }
        });
    }
}
