use std::sync::Arc;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use tauri::Emitter;

use crate::error::AppError;
use crate::models::account::{SyncStatus, TotpSession};
use crate::services::accounts_storage::AccountsStorage;
use crate::services::goodluck_auth::GoodLuckAuthService;
use crate::services::rune_pages_storage::RunePagesStorage;
use crate::services::settings::SettingsService;

pub struct CloudSyncService {
    accounts: Arc<AccountsStorage>,
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
        _settings: Arc<SettingsService>,
        _rune_pages: Arc<RunePagesStorage>,
        goodluck: Arc<GoodLuckAuthService>,
    ) -> Self {
        Self {
            accounts,
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
        let Some(user) = self.goodluck.get_user() else {
            return SyncStatus::Disconnected;
        };
        if !super::vault::path(&user.user_id).exists() {
            return SyncStatus::Idle;
        }
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
        Ok(())
    }

    fn vault_user(&self) -> Result<String, AppError> {
        self.goodluck
            .get_user()
            .map(|u| u.user_id)
            .ok_or_else(|| AppError::Custom("Войдите в GoodLuck".into()))
    }
    pub fn vault_connected(&self) -> bool {
        self.vault_user()
            .ok()
            .is_some_and(|id| super::vault::load(&id).is_ok())
    }
    pub async fn connect_vault(&self, code: Option<String>) -> Result<Option<String>, AppError> {
        let _lock = self.sync_lock.lock().await;
        let user = self.vault_user()?;
        let remote = self
            .goodluck
            .vault_request(
                reqwest::Method::GET,
                None,
                self.get_valid_totp_session().as_deref(),
            )
            .await?;
        if super::vault::load(&user).is_ok() {
            return Err(AppError::Custom("Этот ПК уже подключён к хранилищу".into()));
        }
        let creating = code.is_none();
        let key = code.unwrap_or_else(super::vault::new_key);
        let mut state = super::vault::LocalVault {
            key: key.trim().into(),
            ..Default::default()
        };
        if let Some(envelope) = remote.get("envelope").filter(|v| !v.is_null()) {
            if creating {
                return Err(AppError::Custom(
                    "Хранилище уже существует. Введите код с первого ПК".into(),
                ));
            }
            let envelope = serde_json::from_value(envelope.clone())?;
            super::vault::open(&user, &state.key, &envelope)?;
            state.baseline = Some(serde_json::json!({"accounts":{}}));
            state.revision = remote
                .get("revision")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
        } else if !creating {
            return Err(AppError::Custom(
                "Хранилище ещё не создано на первом ПК".into(),
            ));
        }
        if creating && remote.get("legacy").and_then(|v| v.as_bool()) == Some(true) {
            let legacy = self
                .goodluck
                .legacy_cloud_for_migration(self.get_valid_totp_session().as_deref())
                .await?;
            self.accounts
                .migrate_missing_cloud_accounts(legacy.accounts)?;
        }
        // First merge has an empty base: disjoint accounts merge; differing copies require a choice.
        super::vault::save(&user, &state)?;
        if creating {
            self.vault_cycle("merge").await?;
            Ok(Some(state.key))
        } else {
            Ok(None)
        }
    }
    pub async fn disconnect_vault(&self) -> Result<(), AppError> {
        let _lock = self.sync_lock.lock().await;
        let path = super::vault::path(&self.vault_user()?);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        self.set_status(SyncStatus::Idle);
        Ok(())
    }
    pub fn vault_recovery_code(&self) -> Result<String, AppError> {
        Ok(super::vault::load(&self.vault_user()?)?.key)
    }
    pub async fn rotate_vault(&self) -> Result<String, AppError> {
        let _lock = self.sync_lock.lock().await;
        self.vault_cycle("merge").await?;
        let user = self.vault_user()?;
        let mut state = super::vault::load(&user)?;
        let document = state
            .baseline
            .clone()
            .ok_or_else(|| AppError::Custom("Сначала синхронизируйте хранилище".into()))?;
        let key = super::vault::new_key();
        let envelope = super::vault::seal(&user, &key, &document)?;
        // Keep the pending key locally before the remote write so interruption cannot lose access.
        state.pending_key = Some(key.clone());
        super::vault::save(&user, &state)?;
        let result = self
            .goodluck
            .vault_request(
                reqwest::Method::PUT,
                Some(serde_json::json!({"expectedRevision":state.revision,"envelope":envelope})),
                self.get_valid_totp_session().as_deref(),
            )
            .await?;
        state.key = key.clone();
        state.pending_key = None;
        state.revision = result
            .get("revision")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        super::vault::save(&user, &state)?;
        Ok(key)
    }
    pub async fn resolve_vault(&self, mode: &str) -> Result<(), AppError> {
        let _lock = self.sync_lock.lock().await;
        self.vault_cycle(mode).await
    }
    async fn vault_cycle(&self, mode: &str) -> Result<(), AppError> {
        let user = self.vault_user()?;
        let mut state = super::vault::load(&user)?;
        let local = self.accounts.vault_document()?;
        let session = self.get_valid_totp_session();
        let remote = self
            .goodluck
            .vault_request(reqwest::Method::GET, None, session.as_deref())
            .await?;
        let revision = remote
            .get("revision")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        if revision.is_none() && state.revision.is_some() {
            return Err(AppError::Custom("Облачное хранилище удалено. Локальные аккаунты сохранены; подключите хранилище заново".into()));
        }
        let document = if let Some(value) = remote.get("envelope").filter(|v| !v.is_null()) {
            let envelope = serde_json::from_value(value.clone())?;
            match super::vault::open(&user, &state.key, &envelope) {
                Ok(value) => value,
                Err(error) => {
                    if let Some(pending) = state.pending_key.clone() {
                        let value = super::vault::open(&user, &pending, &envelope)?;
                        state.key = pending;
                        state.pending_key = None;
                        value
                    } else {
                        return Err(error);
                    }
                }
            }
        } else {
            serde_json::json!({"accounts":{}})
        };
        let base = state
            .baseline
            .clone()
            .unwrap_or_else(|| serde_json::json!({"accounts":{}}));
        let merged = match mode {
            "local" => super::vault::merge_with_choice(&base, &local, &document, Some(true))?,
            "remote" => super::vault::merge_with_choice(&base, &local, &document, Some(false))?,
            _ => super::vault::merge(&base, &local, &document)?,
        };
        if merged != document || revision.is_none() {
            let envelope = super::vault::seal(&user, &state.key, &merged)?;
            let saved = self
                .goodluck
                .vault_request(
                    reqwest::Method::PUT,
                    Some(serde_json::json!({"expectedRevision":revision,"envelope":envelope})),
                    session.as_deref(),
                )
                .await?;
            state.revision = saved
                .get("revision")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
        } else {
            state.revision = revision;
        }
        self.accounts.apply_vault_document(&local, &merged)?;
        state.baseline = Some(merged);
        super::vault::save(&user, &state)?;
        self.set_status(SyncStatus::Success {
            last_synced: Utc::now().to_rfc3339(),
        });
        self.emit_cloud_sync_complete();
        Ok(())
    }
    async fn push_inner(&self) -> Result<(), AppError> {
        if let Some(user) = self.goodluck.get_user() {
            if !super::vault::path(&user.user_id).exists() {
                self.set_status(SyncStatus::Idle);
                return Ok(());
            }
        } else {
            self.set_status(SyncStatus::Disconnected);
            return Ok(());
        }
        self.set_status(SyncStatus::Syncing);
        let result = self.vault_cycle("merge").await;
        if let Err(ref e) = result {
            self.set_status(SyncStatus::Error {
                message: e.to_string(),
            });
        }
        result
    }
    async fn pull_inner(&self, _app: &crate::state::AppState) -> Result<usize, AppError> {
        self.push_inner().await?;
        Ok(0)
    }

    pub async fn delete_cloud_data(&self, totp_code: Option<&str>) -> Result<(), AppError> {
        let _guard = self.sync_lock.lock().await;
        self.goodluck.delete_cloud_accounts(totp_code).await?;
        if let Ok(user) = self.vault_user() {
            let path = super::vault::path(&user);
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }
        *self.last_synced_at.lock().unwrap() = None;
        self.set_status(SyncStatus::Idle);
        Ok(())
    }

    pub fn notify_change(&self) {
        self.debounce_notify.notify_one();
    }

    pub fn start_debounce_loop(self: &Arc<Self>, _app: &tauri::AppHandle) {
        let this = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    _=this.debounce_notify.notified()=>{},
                    _=tokio::time::sleep(std::time::Duration::from_secs(30))=>{},
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
                if !this.goodluck.is_connected() {
                    continue;
                }
                // Merge against the encrypted baseline; concurrent changes never overwrite silently.
                let _ = this.push().await;
                this.emit_cloud_sync_complete();
            }
        });
    }
}
