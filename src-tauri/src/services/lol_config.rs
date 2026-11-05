#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::services::file_logger::FileLogger;
use crate::services::riot_client::RiotClientService;

const PRESETS_DIRNAME: &str = "lol_config_presets";
const METADATA_FILENAME: &str = "metadata.json";
const PERSISTED_SETTINGS_FILENAME: &str = "PersistedSettings.json";
const CONFIG_SUBDIR: &str = "Config";
const LOLCFG_FORMAT_TAG: &str = "rustlm-lolcfg";
const LOLCFG_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct ConfigStatus {
    pub path: Option<String>,
    pub exists: bool,
    pub readonly: bool,
    pub league_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetMeta {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub source_app_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PresetsIndex {
    presets: Vec<PresetMeta>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LolCfgFile {
    format: String,
    version: u32,
    created_at: String,
    source_app_version: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    settings: serde_json::Value,
}

pub struct LolConfigService {
    riot_client: Arc<RiotClientService>,
    logger: Arc<FileLogger>,
    lock: Mutex<()>,
    login_in_progress: AtomicBool,
}

impl LolConfigService {
    pub fn new(riot_client: Arc<RiotClientService>, logger: Arc<FileLogger>) -> Self {
        Self {
            riot_client,
            logger,
            lock: Mutex::new(()),
            login_in_progress: AtomicBool::new(false),
        }
    }

    fn app_version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    // --- Path helpers ---

    fn presets_dir() -> Result<PathBuf, AppError> {
        let base = dirs::data_dir()
            .ok_or_else(|| AppError::Custom("APPDATA directory not available".into()))?;
        let dir = base.join("LolManager").join(PRESETS_DIRNAME);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    fn metadata_path() -> Result<PathBuf, AppError> {
        Ok(Self::presets_dir()?.join(METADATA_FILENAME))
    }

    fn preset_file_path(id: &str) -> Result<PathBuf, AppError> {
        validate_preset_id(id)?;
        Ok(Self::presets_dir()?.join(format!("{}.json", id)))
    }

    /// Path to the live `PersistedSettings.json` inside LoL install.
    fn persisted_settings_path(&self) -> Option<PathBuf> {
        let _ = self;
        let install_dir = RiotClientService::find_lol_install_dir()?;
        Some(
            install_dir
                .join(CONFIG_SUBDIR)
                .join(PERSISTED_SETTINGS_FILENAME),
        )
    }

    // --- Metadata I/O ---

    fn read_index() -> Result<PresetsIndex, AppError> {
        let path = Self::metadata_path()?;
        if !path.exists() {
            return Ok(PresetsIndex::default());
        }
        let raw = std::fs::read_to_string(&path)?;
        let index: PresetsIndex = serde_json::from_str(&raw)?;
        Ok(index)
    }

    fn write_index(index: &PresetsIndex) -> Result<(), AppError> {
        let path = Self::metadata_path()?;
        let raw = serde_json::to_string_pretty(index)?;
        super::network::atomic_write(&path, raw.as_bytes()).map_err(AppError::Custom)?;
        Ok(())
    }

    fn guest_journal_path() -> Result<PathBuf, AppError> {
        Ok(Self::presets_dir()?.join("guest-recovery.json"))
    }

    fn personal_choice_path() -> Result<PathBuf, AppError> {
        let base = dirs::data_local_dir()
            .ok_or_else(|| AppError::Custom("Local AppData недоступен".into()))?;
        let dir = base.join("LolManager");
        std::fs::create_dir_all(&dir)?;
        Ok(dir.join("guest-personal-preset.json"))
    }

    pub fn set_personal_preset(&self, id: String) -> Result<(), AppError> {
        let _guard = self.lock.lock().unwrap();
        self.ensure_normal_mutation()?;
        let raw = std::fs::read(Self::preset_file_path(&id)?)?;
        keyboard_settings(&raw, &raw)?;
        save_personal_choice(&Self::personal_choice_path()?, &id)
    }

    // Called after the old League client has closed and BEFORE foreign credentials are submitted.
    pub fn prepare_login(&self, shared: bool) -> Result<(), AppError> {
        let _guard = self.lock.lock().unwrap();
        ensure_game_closed()?;
        let path = Self::guest_journal_path()?;
        if path.exists() {
            let previous = GuestJournal::load(&path)?;
            if !previous.automatic {
                return Err(AppError::Custom(
                    "Сначала завершите ручной гостевой режим".into(),
                ));
            }
            let root = RiotClientService::find_lol_install_dir()
                .ok_or_else(|| AppError::Custom("Исходная установка недоступна".into()))?;
            previous.restore(&root, &path)?;
        }
        if !shared {
            return Ok(());
        }
        let root = RiotClientService::find_lol_install_dir()
            .ok_or_else(|| AppError::Custom("Установка League не найдена".into()))?;
        let dir = Self::presets_dir()?;
        let id = ensure_personal_preset(&root, &dir, &Self::personal_choice_path()?)?;
        let preset = std::fs::read(Self::preset_file_path(&id)?)?;
        let mut journal = GuestJournal::capture(&root, preset, GuestConfigMode::Keyboard)?;
        journal.automatic = true;
        journal.protect_applied_settings();
        journal.save(&path)?;
        self.login_in_progress.store(true, Ordering::SeqCst);
        Ok(())
    }

    // Called only after confirmed RSO authorization, immediately BEFORE launch_league.
    pub fn apply_for_shared_login(&self, username: &str) -> Result<(), AppError> {
        let _guard = self.lock.lock().unwrap();
        ensure_game_closed()?;
        if !self.login_in_progress.load(Ordering::SeqCst) {
            return Err(AppError::Custom("Гостевая сессия не подготовлена".into()));
        }
        let path = Self::guest_journal_path()?;
        let mut journal = GuestJournal::load(&path)?;
        let root = RiotClientService::find_lol_install_dir()
            .ok_or_else(|| AppError::Custom("Установка League не найдена".into()))?;
        journal.validate_root(&root)?;
        if !journal.automatic || username.trim().is_empty() {
            return Err(AppError::Custom("Некорректная общая сессия".into()));
        }
        journal.apply_prepared(&root)?;
        journal.shared_session_active = true;
        journal.username_fingerprint = Some(username_fingerprint(username));
        journal.launch_requested_at = Some(Utc::now().timestamp());
        journal.save(&path)
    }

    pub fn finish_login(&self, success: bool) -> Result<(), AppError> {
        let _guard = self.lock.lock().unwrap();
        if !self.login_in_progress.swap(false, Ordering::SeqCst) {
            return Ok(());
        }
        let path = Self::guest_journal_path()?;
        if !path.exists() {
            return Ok(());
        }
        if !success {
            let mut journal = GuestJournal::load(&path)?;
            journal.shared_session_active = false;
            journal.save(&path)?;
            ensure_game_closed()?;
            let root = RiotClientService::find_lol_install_dir()
                .ok_or_else(|| AppError::Custom("Исходная установка недоступна".into()))?;
            journal.restore(&root, &path)?;
        }
        Ok(())
    }

    pub async fn observe_shared_login(self: &Arc<Self>) -> Result<(), AppError> {
        let config = self.clone();
        let pending = tokio::task::spawn_blocking(move || config.automatic_recovery_pending())
            .await
            .map_err(|e| AppError::Custom(format!("Проверка гостевой сессии: {e}")))??;
        let Some(session_id) = pending else {
            return Ok(());
        };
        // Never infer logout from a network error or an empty identity.
        let username = self
            .riot_client
            .get_authorized_riot_login_username()
            .await
            .ok()
            .filter(|s| !s.trim().is_empty());
        let authorized = self.riot_client.rso_authorization_state().await;
        let config = self.clone();
        tokio::task::spawn_blocking(move || {
            config.observe_shared_login_state(&session_id, username.as_deref(), authorized)
        })
        .await
        .map_err(|e| AppError::Custom(format!("Восстановление гостевых настроек: {e}")))?
    }

    fn automatic_recovery_pending(&self) -> Result<Option<String>, AppError> {
        let _guard = self.lock.lock().unwrap();
        if self.login_in_progress.load(Ordering::SeqCst) {
            return Ok(None);
        }
        let path = Self::guest_journal_path()?;
        if !path.exists() {
            return Ok(None);
        }
        let journal = GuestJournal::load(&path)?;
        Ok(journal.automatic.then_some(journal.session_id))
    }

    fn observe_shared_login_state(
        &self,
        session_id: &str,
        username: Option<&str>,
        authorized: Option<bool>,
    ) -> Result<(), AppError> {
        let _guard = self.lock.lock().unwrap();
        let path = Self::guest_journal_path()?;
        if self.login_in_progress.load(Ordering::SeqCst) || !path.exists() {
            return Ok(());
        }
        let mut journal = GuestJournal::load(&path)?;
        if !journal.automatic || journal.session_id != session_id {
            return Ok(());
        }
        let probe = RiotClientService::unglitch_probe();
        let running = probe.backend_running || probe.ui_running || probe.game_running;
        let before = (journal.shared_session_active, journal.league_seen);
        let should_restore = journal.observe(running, username, authorized, Utc::now().timestamp());
        if before != (journal.shared_session_active, journal.league_seen) {
            journal.save(&path)?;
        }
        if should_restore {
            ensure_game_closed()?;
            let root = RiotClientService::find_lol_install_dir()
                .ok_or_else(|| AppError::Custom("Исходная установка недоступна".into()))?;
            journal.restore(&root, &path)?;
        }
        Ok(())
    }

    fn ensure_normal_mutation(&self) -> Result<(), AppError> {
        if self.login_in_progress.load(Ordering::SeqCst) {
            return Err(AppError::Custom("Дождитесь завершения входа".into()));
        }
        ensure_no_guest(&Self::guest_journal_path()?)?;
        ensure_game_closed()
    }

    pub fn guest_status(&self) -> Result<GuestConfigStatus, AppError> {
        let _guard = self.lock.lock().unwrap();
        let path = Self::guest_journal_path()?;
        if !path.exists() {
            return Ok(GuestConfigStatus {
                active: false,
                changed: false,
                pending_restore: false,
                personal_preset_id: read_personal_choice(&Self::personal_choice_path()?)?,
            });
        }
        let journal = GuestJournal::load(&path)?;
        let changed = journal.changed()?;
        Ok(GuestConfigStatus {
            active: true,
            changed,
            pending_restore: !journal.shared_session_active,
            personal_preset_id: read_personal_choice(&Self::personal_choice_path()?)?,
        })
    }

    pub fn guest_start(&self, id: String, mode: GuestConfigMode) -> Result<(), AppError> {
        let _guard = self.lock.lock().unwrap();
        self.ensure_normal_mutation()?;
        let preset = std::fs::read(Self::preset_file_path(&id)?)?;
        let _: serde_json::Value = serde_json::from_slice(&preset)?;
        let root = RiotClientService::find_lol_install_dir()
            .ok_or_else(|| AppError::Custom("Установка League не найдена".into()))?;
        let path = Self::guest_journal_path()?;
        let journal = GuestJournal::capture(&root, preset, mode)?;
        journal.save(&path)?; // Durable originals and intended bytes precede any live mutation.
        ensure_game_closed()?;
        journal.validate_root(&root)?;
        if journal.files.iter().any(|f| {
            snapshot(&journal.root.join(CONFIG_SUBDIR).join(&f.name))
                .ok()
                .as_ref()
                != Some(&f.original)
        }) {
            return Err(AppError::Custom(
                "Настройки изменились во время сохранения; требуется восстановление".into(),
            ));
        }
        write_snapshot(
            &journal
                .root
                .join(CONFIG_SUBDIR)
                .join(PERSISTED_SETTINGS_FILENAME),
            &journal.files[0].applied,
        )?;
        Ok(())
    }

    pub fn guest_restore(
        &self,
        confirm_changes: bool,
        save_name: Option<String>,
    ) -> Result<(), AppError> {
        let _guard = self.lock.lock().unwrap();
        if self.login_in_progress.load(Ordering::SeqCst) {
            return Err(AppError::Custom("Дождитесь завершения входа".into()));
        }
        ensure_game_closed()?;
        let path = Self::guest_journal_path()?;
        let mut journal = GuestJournal::load(&path)?;
        let root = RiotClientService::find_lol_install_dir()
            .ok_or_else(|| AppError::Custom("Исходная установка недоступна".into()))?;
        journal.validate_root(&root)?;
        if journal.changed()? && !confirm_changes {
            return Err(AppError::Custom(
                "Настройки изменены извне. Сохраните их или явно подтвердите восстановление".into(),
            ));
        }
        if let Some(name) = save_name {
            let name = name.trim();
            if name.is_empty() {
                return Err(AppError::Custom("Введите имя пресета".into()));
            }
            let raw = std::fs::read(
                journal
                    .root
                    .join(CONFIG_SUBDIR)
                    .join(PERSISTED_SETTINGS_FILENAME),
            )?;
            let _: serde_json::Value = serde_json::from_slice(&raw)?;
            let meta = PresetMeta {
                id: generate_preset_id(),
                name: name.into(),
                created_at: Utc::now().to_rfc3339(),
                source_app_version: Self::app_version(),
            };
            super::network::atomic_write(&Self::preset_file_path(&meta.id)?, &raw)
                .map_err(AppError::Custom)?;
            let mut index = Self::read_index()?;
            index.presets.insert(0, meta);
            Self::write_index(&index)?;
        }
        // Persist the explicit decision and current fingerprints before a partial restore.
        for file in &mut journal.files {
            file.applied = snapshot(&journal.root.join(CONFIG_SUBDIR).join(&file.name))?;
        }
        journal.save(&path)?;
        journal.restore(&root, &path)
    }

    // --- Public API ---

    pub fn get_status(&self) -> ConfigStatus {
        let path = self.persisted_settings_path();
        let path_str = path.as_ref().map(|p| p.display().to_string());
        let (exists, readonly) = match path.as_ref() {
            Some(p) if p.exists() => (true, is_readonly(p).unwrap_or(false)),
            _ => (false, false),
        };
        ConfigStatus {
            path: path_str,
            exists,
            readonly,
            league_running: RiotClientService::is_league_running(),
        }
    }

    pub fn set_readonly(&self, readonly: bool) -> Result<(), AppError> {
        let _guard = self.lock.lock().unwrap();
        self.ensure_normal_mutation()?;
        let path = self
            .persisted_settings_path()
            .ok_or_else(|| AppError::Custom("Установка League of Legends не найдена".into()))?;
        if !path.exists() {
            return Err(AppError::Custom(
                "Файл PersistedSettings.json ещё не создан — запустите игру хотя бы один раз"
                    .into(),
            ));
        }
        set_readonly_attribute(&path, readonly)?;
        self.logger.info(&format!(
            "lol_config: set_readonly={} on {}",
            readonly,
            path.display()
        ));
        Ok(())
    }

    pub fn list_presets(&self) -> Result<Vec<PresetMeta>, AppError> {
        let _guard = self.lock.lock().unwrap();
        let mut index = Self::read_index()?;
        // Drop orphans (preset metadata without backing file on disk).
        let dir = Self::presets_dir()?;
        let before = index.presets.len();
        index
            .presets
            .retain(|p| dir.join(format!("{}.json", p.id)).exists());
        if index.presets.len() != before {
            Self::write_index(&index)?;
            self.logger.info(&format!(
                "lol_config: cleaned {} orphan preset entries",
                before - index.presets.len()
            ));
        }
        Ok(index.presets)
    }

    pub fn create_preset(&self, name: String) -> Result<PresetMeta, AppError> {
        let _guard = self.lock.lock().unwrap();
        self.ensure_normal_mutation()?;
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return Err(AppError::Custom("Имя пресета не может быть пустым".into()));
        }
        let src = self
            .persisted_settings_path()
            .ok_or_else(|| AppError::Custom("Установка League of Legends не найдена".into()))?;
        if !src.exists() {
            return Err(AppError::Custom(
                "Файл PersistedSettings.json ещё не создан — запустите игру хотя бы один раз"
                    .into(),
            ));
        }
        let raw = std::fs::read_to_string(&src)?;
        // Validate JSON before storing — corrupted files should not silently propagate.
        let _: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| AppError::Custom(format!("PersistedSettings.json повреждён: {}", e)))?;

        let id = generate_preset_id();
        let dst = Self::preset_file_path(&id)?;
        super::network::atomic_write(&dst, raw.as_bytes()).map_err(AppError::Custom)?;

        let meta = PresetMeta {
            id: id.clone(),
            name: trimmed,
            created_at: Utc::now().to_rfc3339(),
            source_app_version: Self::app_version(),
        };
        let mut index = Self::read_index()?;
        index.presets.insert(0, meta.clone());
        Self::write_index(&index)?;
        self.logger.info(&format!(
            "lol_config: created preset {} ({})",
            meta.name, id
        ));
        Ok(meta)
    }

    pub fn apply_preset(&self, id: String) -> Result<(), AppError> {
        let _guard = self.lock.lock().unwrap();
        self.ensure_normal_mutation()?;
        let src = Self::preset_file_path(&id)?;
        if !src.exists() {
            return Err(AppError::Custom("Пресет не найден".into()));
        }
        let dst = self
            .persisted_settings_path()
            .ok_or_else(|| AppError::Custom("Установка League of Legends не найдена".into()))?;
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let raw = std::fs::read_to_string(&src)?;
        let _: serde_json::Value = serde_json::from_str(&raw)?;
        with_writable_swap(&dst, |target| {
            super::network::atomic_write(target, raw.as_bytes()).map_err(AppError::Custom)?;
            Ok(())
        })?;

        self.logger.info(&format!(
            "lol_config: applied preset {} → {}",
            id,
            dst.display()
        ));
        Ok(())
    }

    pub fn delete_preset(&self, id: String) -> Result<(), AppError> {
        let _guard = self.lock.lock().unwrap();
        self.ensure_normal_mutation()?;
        let path = Self::preset_file_path(&id)?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        let mut index = Self::read_index()?;
        index.presets.retain(|p| p.id != id);
        Self::write_index(&index)?;
        self.logger
            .info(&format!("lol_config: deleted preset {}", id));
        Ok(())
    }

    pub fn export_preset(&self, id: String, dst_path: String) -> Result<(), AppError> {
        let _guard = self.lock.lock().unwrap();
        self.ensure_normal_mutation()?;
        let preset_path = Self::preset_file_path(&id)?;
        if !preset_path.exists() {
            return Err(AppError::Custom("Пресет не найден".into()));
        }
        let index = Self::read_index()?;
        let meta = index
            .presets
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| AppError::Custom("Метаданные пресета не найдены".into()))?;

        let raw = std::fs::read_to_string(&preset_path)?;
        let settings: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| AppError::Custom(format!("Пресет повреждён: {}", e)))?;

        let payload = LolCfgFile {
            format: LOLCFG_FORMAT_TAG.into(),
            version: LOLCFG_FORMAT_VERSION,
            created_at: meta.created_at,
            source_app_version: meta.source_app_version,
            name: meta.name,
            description: None,
            settings,
        };
        let serialized = serde_json::to_string_pretty(&payload)?;
        std::fs::write(&dst_path, serialized)?;
        self.logger.info(&format!(
            "lol_config: exported preset {} → {}",
            id, dst_path
        ));
        Ok(())
    }

    pub fn import_preset(&self, src_path: String) -> Result<PresetMeta, AppError> {
        let _guard = self.lock.lock().unwrap();
        self.ensure_normal_mutation()?;
        let raw = std::fs::read_to_string(&src_path)?;
        let payload: LolCfgFile = serde_json::from_str(&raw)
            .map_err(|e| AppError::Custom(format!("Файл не является валидным .lolcfg: {}", e)))?;
        if payload.format != LOLCFG_FORMAT_TAG {
            return Err(AppError::Custom(
                "Файл не является пресетом RustLM (неверный формат)".into(),
            ));
        }
        if payload.version != LOLCFG_FORMAT_VERSION {
            return Err(AppError::Custom(format!(
                "Версия .lolcfg ({}) новее, чем поддерживает приложение",
                payload.version
            )));
        }

        let id = generate_preset_id();
        let dst = Self::preset_file_path(&id)?;
        let settings_raw = serde_json::to_string_pretty(&payload.settings)?;
        super::network::atomic_write(&dst, settings_raw.as_bytes()).map_err(AppError::Custom)?;

        let meta = PresetMeta {
            id: id.clone(),
            name: payload.name,
            created_at: Utc::now().to_rfc3339(),
            source_app_version: payload.source_app_version,
        };
        let mut index = Self::read_index()?;
        index.presets.insert(0, meta.clone());
        Self::write_index(&index)?;
        self.logger.info(&format!(
            "lol_config: imported preset {} ({})",
            meta.name, id
        ));
        Ok(meta)
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuestConfigMode {
    #[default]
    Keyboard,
    Full,
}

/// PersistedSettings stores virtual INI files in `files`, not top-level input settings.
fn keyboard_settings(current: &[u8], preset: &[u8]) -> Result<Vec<u8>, AppError> {
    use serde_json::Value;
    fn invalid() -> AppError {
        AppError::Custom("Режим «Клавиши»: нужен files с единственным Input.ini и непустыми sections/settings (name/value)".into())
    }
    fn input_index(files: &[Value]) -> Result<Option<usize>, AppError> {
        let mut index = None;
        for (i, file) in files.iter().enumerate() {
            let name = file
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .ok_or_else(|| {
                    AppError::Custom("Некорректный files/name в PersistedSettings".into())
                })?;
            if name.eq_ignore_ascii_case("Input.ini") {
                if index.is_some() {
                    return Err(AppError::Custom("В files несколько Input.ini".into()));
                }
                index = Some(i);
            }
        }
        Ok(index)
    }
    fn input_sections(input: &Value) -> Result<&Vec<Value>, AppError> {
        let sections = input
            .get("sections")
            .and_then(Value::as_array)
            .filter(|sections| !sections.is_empty())
            .ok_or_else(invalid)?;
        let mut section_names = std::collections::HashSet::new();
        for section in sections {
            let name = section
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(invalid)?;
            if !section_names.insert(name.to_ascii_lowercase()) {
                return Err(invalid());
            }
            let settings = section
                .get("settings")
                .and_then(Value::as_array)
                .filter(|settings| !settings.is_empty())
                .ok_or_else(invalid)?;
            let mut setting_names = std::collections::HashSet::new();
            for setting in settings {
                let name = setting
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.trim().is_empty())
                    .ok_or_else(invalid)?;
                if !setting_names.insert(name.to_ascii_lowercase())
                    || !setting.get("value").is_some_and(Value::is_string)
                {
                    return Err(invalid());
                }
            }
        }
        Ok(sections)
    }
    let source: Value = serde_json::from_slice(preset)?;
    let source_files = source
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    let source_input = &source_files[input_index(source_files)?.ok_or_else(invalid)?];
    let sections = input_sections(source_input)?;
    let mut target: Value = serde_json::from_slice(current)?;
    let target_files = target
        .get_mut("files")
        .and_then(Value::as_array_mut)
        .ok_or_else(invalid)?;
    let index = input_index(target_files)?.ok_or_else(invalid)?;
    input_sections(&target_files[index])?;
    target_files[index]["sections"] = Value::Array(sections.clone());
    Ok(serde_json::to_vec_pretty(&target)?)
}

const GUEST_FILES: [&str; 3] = [PERSISTED_SETTINGS_FILENAME, "input.ini", "game.cfg"];

#[derive(Debug, Serialize)]
pub struct GuestConfigStatus {
    pub active: bool,
    pub changed: bool,
    pub pending_restore: bool,
    pub personal_preset_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FileSnapshot {
    bytes: Option<Vec<u8>>,
    readonly: bool,
    #[cfg(windows)]
    attributes: u32,
}
#[derive(Debug, Serialize, Deserialize)]
struct GuestFile {
    name: String,
    original: FileSnapshot,
    applied: FileSnapshot,
}
#[derive(Debug, Serialize, Deserialize)]
struct GuestJournal {
    root: PathBuf,
    identity: String,
    config_identity: String,
    files: Vec<GuestFile>,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    automatic: bool,
    #[serde(default)]
    shared_session_active: bool,
    #[serde(default)]
    league_seen: bool,
    #[serde(default)]
    username_fingerprint: Option<String>,
    #[serde(default)]
    launch_requested_at: Option<i64>,
}

fn username_fingerprint(username: &str) -> String {
    use sha2::{Digest, Sha256};
    hex_encode(&Sha256::digest(username.trim().to_lowercase().as_bytes()))
}

fn read_personal_choice(path: &Path) -> Result<Option<String>, AppError> {
    safe_path(path)?;
    if !path.try_exists()? {
        return Ok(None);
    }
    let id: String = serde_json::from_slice(&std::fs::read(path)?)?;
    validate_preset_id(&id)?;
    Ok(Some(id))
}

fn save_personal_choice(path: &Path, id: &str) -> Result<(), AppError> {
    validate_preset_id(id)?;
    safe_path(path)?;
    super::network::atomic_write(path, &serde_json::to_vec(id)?).map_err(AppError::Custom)
}

fn ensure_personal_preset(root: &Path, dir: &Path, choice: &Path) -> Result<String, AppError> {
    if let Some(id) = read_personal_choice(choice)? {
        let path = dir.join(format!("{id}.json"));
        safe_path(&path)?;
        let raw = std::fs::read(path)?;
        keyboard_settings(&raw, &raw)?;
        return Ok(id);
    }
    // The first shared entry snapshots THIS PC before foreign credentials are submitted.
    let source = root.join(CONFIG_SUBDIR).join(PERSISTED_SETTINGS_FILENAME);
    let raw = snapshot(&source)?.bytes.ok_or_else(|| {
        AppError::Custom("Сначала запустите свой аккаунт League для создания настроек".into())
    })?;
    keyboard_settings(&raw, &raw)?;
    let id = generate_preset_id();
    super::network::atomic_write(&dir.join(format!("{id}.json")), &raw)
        .map_err(AppError::Custom)?;
    let index_path = dir.join(METADATA_FILENAME);
    let mut index: PresetsIndex = if index_path.try_exists()? {
        serde_json::from_slice(&std::fs::read(&index_path)?)?
    } else {
        PresetsIndex::default()
    };
    index.presets.insert(
        0,
        PresetMeta {
            id: id.clone(),
            name: "Мои клавиши на этом ПК".into(),
            created_at: Utc::now().to_rfc3339(),
            source_app_version: LolConfigService::app_version(),
        },
    );
    super::network::atomic_write(&index_path, &serde_json::to_vec_pretty(&index)?)
        .map_err(AppError::Custom)?;
    save_personal_choice(choice, &id)?;
    Ok(id)
}

fn ensure_no_guest(path: &Path) -> Result<(), AppError> {
    if path.try_exists()? {
        return Err(AppError::Custom(
            "Сначала восстановите гостевую сессию".into(),
        ));
    }
    Ok(())
}

fn ensure_game_closed() -> Result<(), AppError> {
    let probe = RiotClientService::unglitch_probe();
    if probe.backend_running || probe.ui_running || probe.game_running {
        return Err(AppError::Custom(
            "Закройте League Client и игру; восстановление остаётся ожидающим".into(),
        ));
    }
    Ok(())
}

fn safe_path(path: &Path) -> Result<(), AppError> {
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(meta) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if meta.file_attributes() & 0x400 != 0 {
                        return Err(AppError::Custom("Reparse point в пути конфигов".into()));
                    }
                }
                if meta.file_type().is_symlink() {
                    return Err(AppError::Custom("Символические ссылки запрещены".into()));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

fn root_identity(path: &Path) -> Result<String, AppError> {
    safe_path(path)?;
    let meta = std::fs::metadata(path)?;
    if !meta.is_dir() {
        return Err(AppError::Custom("Установка не является каталогом".into()));
    }
    #[cfg(windows)]
    {
        use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
        use windows::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION},
        };
        let file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x02000000)
            .open(path)?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }
            .map_err(|e| AppError::Custom(format!("Не удалось проверить установку: {e}")))?;
        Ok(format!(
            "{}:{}:{}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        ))
    }
    #[cfg(not(windows))]
    {
        Ok(format!("{:?}", meta.created()?))
    }
}

fn snapshot(path: &Path) -> Result<FileSnapshot, AppError> {
    safe_path(path)?;
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => Ok(FileSnapshot {
            bytes: Some(std::fs::read(path)?),
            readonly: meta.permissions().readonly(),
            #[cfg(windows)]
            attributes: {
                use std::os::windows::fs::MetadataExt;
                meta.file_attributes()
            },
        }),
        Ok(_) => Err(AppError::Custom(
            "Вместо файла настроек найден каталог".into(),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(FileSnapshot {
            bytes: None,
            readonly: false,
            #[cfg(windows)]
            attributes: 0,
        }),
        Err(e) => Err(e.into()),
    }
}

fn write_snapshot(path: &Path, value: &FileSnapshot) -> Result<(), AppError> {
    safe_path(path)?;
    let was_readonly = path.exists() && is_readonly(path)?;
    if was_readonly {
        set_readonly_attribute(path, false)?;
    }
    let result = (|| {
        if let Some(bytes) = &value.bytes {
            super::network::atomic_write(path, bytes).map_err(AppError::Custom)?;
            set_readonly_attribute(path, value.readonly)?;
            #[cfg(windows)]
            {
                use std::os::windows::ffi::OsStrExt;
                use windows::{
                    core::PCWSTR,
                    Win32::Storage::FileSystem::{SetFileAttributesW, FILE_FLAGS_AND_ATTRIBUTES},
                };
                let wide: Vec<u16> = path
                    .as_os_str()
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
                let attrs = if value.attributes == 0 {
                    0x80
                } else {
                    value.attributes
                };
                unsafe {
                    SetFileAttributesW(PCWSTR(wide.as_ptr()), FILE_FLAGS_AND_ATTRIBUTES(attrs))
                }
                .map_err(|e| AppError::Custom(format!("Не удалось восстановить атрибуты: {e}")))?;
            }
        } else if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    })();
    if result.is_err() && was_readonly && path.exists() {
        set_readonly_attribute(path, true)?;
    }
    result
}

impl GuestJournal {
    fn capture(root: &Path, preset: Vec<u8>, mode: GuestConfigMode) -> Result<Self, AppError> {
        safe_path(root)?;
        let root = std::fs::canonicalize(root)?;
        let identity = root_identity(&root)?;
        let config_identity = root_identity(&root.join(CONFIG_SUBDIR))?;
        let mut files = Vec::new();
        for name in GUEST_FILES {
            let original = snapshot(&root.join(CONFIG_SUBDIR).join(name))?;
            let mut applied = original.clone();
            if name == PERSISTED_SETTINGS_FILENAME {
                applied.bytes = Some(match mode {
                    GuestConfigMode::Full => preset.clone(),
                    GuestConfigMode::Keyboard => keyboard_settings(
                        original.bytes.as_deref().ok_or_else(|| {
                            AppError::Custom(
                                "Для режима «Клавиши» нужен текущий PersistedSettings.json".into(),
                            )
                        })?,
                        &preset,
                    )?,
                });
                #[cfg(windows)]
                if applied.attributes == 0 {
                    applied.attributes = 0x80;
                }
            }
            files.push(GuestFile {
                name: name.into(),
                original,
                applied,
            });
        }
        Ok(Self {
            root,
            identity,
            config_identity,
            files,
            session_id: generate_preset_id(),
            automatic: false,
            shared_session_active: false,
            league_seen: false,
            username_fingerprint: None,
            launch_requested_at: None,
        })
    }
    fn protect_applied_settings(&mut self) {
        self.files[0].applied.readonly = true;
        #[cfg(windows)]
        {
            self.files[0].applied.attributes |= 1;
        }
    }

    fn apply_prepared(&self, root: &Path) -> Result<(), AppError> {
        self.validate_root(root)?;
        for file in &self.files {
            if snapshot(&self.root.join(CONFIG_SUBDIR).join(&file.name))? != file.original {
                return Err(AppError::Custom(
                    "Настройки изменились до запуска League; автоматическая запись отменена".into(),
                ));
            }
        }
        write_snapshot(
            &self
                .root
                .join(CONFIG_SUBDIR)
                .join(PERSISTED_SETTINGS_FILENAME),
            &self.files[0].applied,
        )
    }

    fn observe(
        &mut self,
        running: bool,
        username: Option<&str>,
        authorized: Option<bool>,
        now: i64,
    ) -> bool {
        if !self.automatic {
            return false;
        }
        if authorized == Some(false)
            || username
                .zip(self.username_fingerprint.as_ref())
                .is_some_and(|(actual, expected)| username_fingerprint(actual) != *expected)
        {
            self.shared_session_active = false;
        }
        if running {
            self.league_seen = true;
            return false;
        }
        // Launch may return before League processes appear; don't mistake that gap for an exit.
        !self.shared_session_active
            || self.league_seen
            || self
                .launch_requested_at
                .map_or(true, |started| now.saturating_sub(started) >= 120)
    }

    fn save(&self, path: &Path) -> Result<(), AppError> {
        safe_path(path)?;
        super::network::atomic_write(path, &serde_json::to_vec(self)?).map_err(AppError::Custom)
    }
    fn load(path: &Path) -> Result<Self, AppError> {
        safe_path(path)?;
        let journal: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        if journal.files.len() != 3
            || journal
                .files
                .iter()
                .zip(GUEST_FILES)
                .any(|(f, name)| f.name != name)
        {
            return Err(AppError::Custom(
                "Некорректный журнал восстановления".into(),
            ));
        }
        Ok(journal)
    }
    fn validate_root(&self, current: &Path) -> Result<(), AppError> {
        safe_path(current)?;
        if std::fs::canonicalize(current)? != self.root
            || root_identity(&self.root)? != self.identity
            || root_identity(&self.root.join(CONFIG_SUBDIR))? != self.config_identity
        {
            return Err(AppError::Custom(
                "Исходная установка перемещена или заменена; восстановление запрещено".into(),
            ));
        }
        for file in &self.files {
            safe_path(&self.root.join(CONFIG_SUBDIR).join(&file.name))?;
        }
        Ok(())
    }
    fn changed(&self) -> Result<bool, AppError> {
        self.validate_root(&self.root)?;
        for f in &self.files {
            let current = snapshot(&self.root.join(CONFIG_SUBDIR).join(&f.name))?;
            if current != f.applied && current != f.original {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn restore(&self, current: &Path, journal_path: &Path) -> Result<(), AppError> {
        self.validate_root(current)?;
        for f in &self.files {
            let target = self.root.join(CONFIG_SUBDIR).join(&f.name);
            let current = snapshot(&target)?;
            if current == f.original {
                continue;
            }
            if current != f.applied {
                return Err(AppError::Custom(
                    "Конфликт настроек: подтвердите восстановление повторно".into(),
                ));
            }
            write_snapshot(&target, &f.original)?;
        }
        std::fs::remove_file(journal_path)?;
        Ok(())
    }
}

fn validate_preset_id(id: &str) -> Result<(), AppError> {
    if !id.starts_with("p_")
        || id.len() > 100
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(AppError::Custom(
            "Некорректный идентификатор пресета".into(),
        ));
    }
    Ok(())
}

// --- Preset ID generation ---

fn generate_preset_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    let ts = Utc::now().timestamp();
    format!("p_{:x}_{}", ts, hex_encode(&bytes))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

// --- Read-only attribute (Win32 with std fallback) ---

fn is_readonly(path: &Path) -> Result<bool, AppError> {
    let meta = std::fs::metadata(path)?;
    Ok(meta.permissions().readonly())
}

#[cfg(windows)]
fn set_readonly_attribute(path: &Path, readonly: bool) -> Result<(), AppError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_READONLY, FILE_FLAGS_AND_ATTRIBUTES,
        INVALID_FILE_ATTRIBUTES,
    };

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let attrs = GetFileAttributesW(PCWSTR(wide.as_ptr()));
        if attrs == INVALID_FILE_ATTRIBUTES {
            return Err(AppError::Custom(format!(
                "GetFileAttributesW failed for {}",
                path.display()
            )));
        }
        let ro_bit = FILE_ATTRIBUTE_READONLY.0;
        let new_attrs = if readonly {
            attrs | ro_bit
        } else {
            attrs & !ro_bit
        };
        if new_attrs == attrs {
            return Ok(());
        }
        SetFileAttributesW(PCWSTR(wide.as_ptr()), FILE_FLAGS_AND_ATTRIBUTES(new_attrs))
            .map_err(|e| AppError::Custom(format!("SetFileAttributesW failed: {}", e)))?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_readonly_attribute(path: &Path, readonly: bool) -> Result<(), AppError> {
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_readonly(readonly);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

/// Run `op` with a guaranteed-writable target file. If the file was read-only
/// going in, the attribute is restored afterwards (even if `op` fails).
fn with_writable_swap<F>(path: &Path, op: F) -> Result<(), AppError>
where
    F: FnOnce(&Path) -> Result<(), AppError>,
{
    let was_readonly = path.exists() && is_readonly(path).unwrap_or(false);
    if was_readonly {
        set_readonly_attribute(path, false)?;
    }
    let result = op(path);
    if was_readonly {
        // Best-effort restore — don't mask the original error.
        let _ = set_readonly_attribute(path, true);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    struct GuestFixture {
        dir: PathBuf,
        root: PathBuf,
        journal: PathBuf,
    }
    impl GuestFixture {
        fn new() -> Self {
            let dir =
                std::env::temp_dir().join(format!("rustlm-guest-test-{}", rand::random::<u64>()));
            let root = dir.join("League");
            std::fs::create_dir_all(root.join(CONFIG_SUBDIR)).unwrap();
            Self {
                journal: dir.join("recovery.json"),
                root,
                dir,
            }
        }
        fn file(&self, name: &str) -> PathBuf {
            self.root.join(CONFIG_SUBDIR).join(name)
        }
        fn capture(&self) -> GuestJournal {
            let journal = GuestJournal::capture(
                &self.root,
                br#"{"files":[]}"#.to_vec(),
                GuestConfigMode::Full,
            )
            .unwrap();
            journal.save(&self.journal).unwrap();
            journal
        }
    }
    impl Drop for GuestFixture {
        fn drop(&mut self) {
            for name in GUEST_FILES {
                let _ = set_readonly_attribute(&self.file(name), false);
            }
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    fn settings_fixture(key: &str, width: &str) -> serde_json::Value {
        serde_json::json!({
            "version": 7,
            "unknownRoot": {"keep": true},
            "files": [
                {"name": "Game.cfg", "sections": [
                    {"name": "General", "settings": [{"name": "Width", "value": width}, {"name": "WindowMode", "value": "2"}]},
                    {"name": "Video", "settings": [{"name": "EffectsQuality", "value": "4"}]},
                    {"name": "UnknownPCSection", "custom": {"keep": [1, 2, 3]}}
                ]},
                {"name": "Input.ini", "localMetadata": "keep", "sections": [
                    {"name": "GameEvents", "settings": [{"name": "evtCastSpell1", "value": key}]}
                ]},
                {"name": "Unknown.cfg", "sections": [{"arbitrary": "keep"}]}
            ]
        })
    }
    #[test]
    fn first_shared_entry_bootstraps_own_preset_once_per_pc_before_foreign_settings() {
        let f = GuestFixture::new();
        let presets = f.dir.join("presets");
        std::fs::create_dir(&presets).unwrap();
        let choice = f.dir.join("local-preset.json");
        let own = serde_json::to_vec(&settings_fixture("[q]", "2560")).unwrap();
        std::fs::write(f.file(PERSISTED_SETTINGS_FILENAME), &own).unwrap();
        let id = ensure_personal_preset(&f.root, &presets, &choice).unwrap();
        assert_eq!(read_personal_choice(&choice).unwrap(), Some(id.clone()));
        assert_eq!(
            std::fs::read(presets.join(format!("{id}.json"))).unwrap(),
            own
        );
        std::fs::write(
            f.file(PERSISTED_SETTINGS_FILENAME),
            serde_json::to_vec(&settings_fixture("[a]", "1024")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            ensure_personal_preset(&f.root, &presets, &choice).unwrap(),
            id
        );
        assert_eq!(
            std::fs::read(presets.join(format!("{id}.json"))).unwrap(),
            own
        );
        let index: PresetsIndex =
            serde_json::from_slice(&std::fs::read(presets.join(METADATA_FILENAME)).unwrap())
                .unwrap();
        assert_eq!(index.presets.len(), 1);
        let other_pc = f.dir.join("another-pc-choice.json");
        assert_eq!(read_personal_choice(&other_pc).unwrap(), None);
    }

    #[test]
    fn auto_bootstrap_refuses_unrecognized_settings_and_invalid_saved_choice() {
        let f = GuestFixture::new();
        let presets = f.dir.join("presets");
        std::fs::create_dir(&presets).unwrap();
        let choice = f.dir.join("local-preset.json");
        std::fs::write(f.file(PERSISTED_SETTINGS_FILENAME), br#"{"files":[]}"#).unwrap();
        assert!(ensure_personal_preset(&f.root, &presets, &choice).is_err());
        assert!(!choice.exists());
        assert_eq!(std::fs::read_dir(&presets).unwrap().count(), 0);
        std::fs::write(&choice, br#""../foreign""#).unwrap();
        assert!(read_personal_choice(&choice).is_err());
    }

    #[test]
    fn prepared_auto_apply_protects_keys_and_restart_restores_original_attributes() {
        let f = GuestFixture::new();
        let own = serde_json::to_vec(&settings_fixture("[q]", "2560")).unwrap();
        std::fs::write(f.file(PERSISTED_SETTINGS_FILENAME), &own).unwrap();
        std::fs::write(f.file("game.cfg"), b"Width=2560").unwrap();
        let preset = serde_json::to_vec(&settings_fixture("[a]", "1024")).unwrap();
        let mut journal =
            GuestJournal::capture(&f.root, preset, GuestConfigMode::Keyboard).unwrap();
        journal.automatic = true;
        journal.protect_applied_settings();
        journal.save(&f.journal).unwrap();
        assert!(!is_readonly(&f.file(PERSISTED_SETTINGS_FILENAME)).unwrap());
        journal.apply_prepared(&f.root).unwrap();
        assert!(is_readonly(&f.file(PERSISTED_SETTINGS_FILENAME)).unwrap());
        assert_eq!(
            snapshot(&f.file("game.cfg")).unwrap(),
            journal.files[2].original
        );
        journal.shared_session_active = true;
        journal.username_fingerprint = Some(username_fingerprint("SharedUser"));
        journal.launch_requested_at = Some(1000);
        journal.save(&f.journal).unwrap();
        let mut restarted = GuestJournal::load(&f.journal).unwrap();
        assert!(restarted.automatic);
        assert_eq!(restarted.session_id, journal.session_id);
        assert!(!restarted.observe(true, Some("shareduser"), Some(true), 1001));
        assert!(restarted.observe(false, None, None, 1002));
        restarted.restore(&f.root, &f.journal).unwrap();
        assert_eq!(
            snapshot(&f.file(PERSISTED_SETTINGS_FILENAME)).unwrap(),
            journal.files[0].original
        );
        assert!(!is_readonly(&f.file(PERSISTED_SETTINGS_FILENAME)).unwrap());
        assert_eq!(
            std::fs::read(f.file(PERSISTED_SETTINGS_FILENAME)).unwrap(),
            own
        );
    }

    #[test]
    fn auto_apply_refuses_cloud_changes_between_prepare_and_rso_without_overwriting() {
        let f = GuestFixture::new();
        let own = serde_json::to_vec(&settings_fixture("[q]", "2560")).unwrap();
        std::fs::write(f.file(PERSISTED_SETTINGS_FILENAME), &own).unwrap();
        let journal = GuestJournal::capture(&f.root, own, GuestConfigMode::Keyboard).unwrap();
        journal.save(&f.journal).unwrap();
        let foreign = serde_json::to_vec(&settings_fixture("[z]", "1024")).unwrap();
        std::fs::write(f.file(PERSISTED_SETTINGS_FILENAME), &foreign).unwrap();
        assert!(journal.apply_prepared(&f.root).is_err());
        assert!(journal.restore(&f.root, &f.journal).is_err());
        assert_eq!(
            std::fs::read(f.file(PERSISTED_SETTINGS_FILENAME)).unwrap(),
            foreign
        );
        assert!(f.journal.exists());
    }

    #[test]
    fn watcher_defers_launch_gap_and_running_logout_or_account_change() {
        let f = GuestFixture::new();
        let mut journal = f.capture();
        assert!(!journal.observe(false, None, Some(false), 1000)); // manual sessions stay manual
        journal.automatic = true;
        journal.shared_session_active = true;
        journal.username_fingerprint = Some(username_fingerprint("SharedUser"));
        journal.launch_requested_at = Some(1000);
        assert!(!journal.observe(false, None, None, 1001));
        assert!(journal.shared_session_active); // unavailable identity is not a logout
        assert!(!journal.observe(true, Some("SharedUser"), Some(true), 1002));
        assert!(!journal.observe(true, Some("DifferentUser"), Some(true), 1003));
        assert!(!journal.shared_session_active);
        assert!(journal.observe(false, None, None, 1004));
        journal.shared_session_active = true;
        assert!(!journal.observe(true, None, Some(false), 1005));
        assert!(!journal.shared_session_active);
        journal.shared_session_active = true;
        journal.league_seen = false;
        assert!(journal.observe(false, None, None, 1120)); // launch never materialized
    }

    #[test]
    fn failed_or_cancelled_launch_restores_protected_settings() {
        let f = GuestFixture::new();
        let original = serde_json::to_vec(&settings_fixture("[q]", "2560")).unwrap();
        std::fs::write(f.file(PERSISTED_SETTINGS_FILENAME), &original).unwrap();
        let mut journal = GuestJournal::capture(
            &f.root,
            serde_json::to_vec(&settings_fixture("[a]", "1024")).unwrap(),
            GuestConfigMode::Keyboard,
        )
        .unwrap();
        journal.automatic = true;
        journal.protect_applied_settings();
        journal.save(&f.journal).unwrap();
        journal.apply_prepared(&f.root).unwrap();
        assert!(journal.observe(false, None, None, 1000));
        journal.restore(&f.root, &f.journal).unwrap();
        assert_eq!(
            std::fs::read(f.file(PERSISTED_SETTINGS_FILENAME)).unwrap(),
            original
        );
        assert!(!f.journal.exists());
    }

    #[test]
    fn keyboard_merge_replaces_virtual_input_sections_only() {
        let current = settings_fixture("[q]", "2560");
        let mut preset = settings_fixture("[a]", "1024");
        preset["version"] = serde_json::json!(99);
        preset["files"][1]["name"] = serde_json::json!("input.INI");
        preset["files"][1]["localMetadata"] = serde_json::json!("do not copy");
        let merged: serde_json::Value = serde_json::from_slice(
            &keyboard_settings(
                &serde_json::to_vec(&current).unwrap(),
                &serde_json::to_vec(&preset).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let mut expected = current;
        expected["files"][1]["sections"] = preset["files"][1]["sections"].clone();
        assert_eq!(merged, expected);
        assert_eq!(
            merged["files"][1]["sections"][0]["settings"][0]["value"],
            "[a]"
        );
    }
    #[test]
    fn keyboard_mode_preserves_pc_files_and_rollback_restores_all_three() {
        let f = GuestFixture::new();
        let original = serde_json::to_vec(&settings_fixture("[q]", "2560")).unwrap();
        std::fs::write(f.file(PERSISTED_SETTINGS_FILENAME), &original).unwrap();
        std::fs::write(f.file("input.ini"), b"[GameEvents]\nevtCastSpell1=[q]\n").unwrap();
        std::fs::write(f.file("game.cfg"), b"[General]\nWidth=2560\nWindowMode=2\n").unwrap();
        set_readonly_attribute(&f.file("game.cfg"), true).unwrap();
        let journal = GuestJournal::capture(
            &f.root,
            serde_json::to_vec(&settings_fixture("[a]", "1024")).unwrap(),
            GuestConfigMode::default(),
        )
        .unwrap();
        journal.save(&f.journal).unwrap();
        write_snapshot(
            &f.file(PERSISTED_SETTINGS_FILENAME),
            &journal.files[0].applied,
        )
        .unwrap();
        for i in [1, 2] {
            assert_eq!(
                snapshot(&f.file(GUEST_FILES[i])).unwrap(),
                journal.files[i].original
            );
            assert_eq!(journal.files[i].applied, journal.files[i].original);
        }
        let live: serde_json::Value =
            serde_json::from_slice(&std::fs::read(f.file(PERSISTED_SETTINGS_FILENAME)).unwrap())
                .unwrap();
        assert_eq!(
            live["files"][0],
            settings_fixture("[q]", "2560")["files"][0]
        );
        assert_eq!(
            live["files"][1]["sections"][0]["settings"][0]["value"],
            "[a]"
        );
        GuestJournal::load(&f.journal)
            .unwrap()
            .restore(&f.root, &f.journal)
            .unwrap();
        for (i, name) in GUEST_FILES.iter().enumerate() {
            assert_eq!(snapshot(&f.file(name)).unwrap(), journal.files[i].original);
        }
    }
    #[test]
    fn keyboard_mode_rejects_missing_ambiguous_and_malformed_input_schema() {
        use serde_json::json;
        let current = serde_json::to_vec(&settings_fixture("[q]", "2560")).unwrap();
        let invalid = [
            json!({}),
            json!({"files": {}}),
            json!({"files": []}),
            json!({"files": [{"name": "Game.cfg", "sections": []}]}),
            json!({"files": [{"name": "../Input.ini", "sections": []}]}),
            json!({"files": [{"name": "Input.ini", "sections": {"GameEvents": {"evtCastSpell1": "[a]"}}}]}),
            json!({"files": [{"name": "Input.ini", "sections": []}]}),
            json!({"files": [{"name": "Input.ini", "sections": [{"name": "GameEvents", "settings": []}]}]}),
            json!({"files": [{"name": "Input.ini", "sections": [{"name": "GameEvents", "settings": [{"name": "evtCastSpell1"}]}]}]}),
            json!({"files": [{"name": "Input.ini", "sections": [{"name": "GameEvents", "settings": [{"name": "evtCastSpell1", "value": 1}]}]}]}),
            json!({"files": [{"name": "Input.ini"}, {"name": "input.ini"}]}),
            json!({"files": [{"name": "Input.ini", "sections": [{"name": "", "settings": [{"name": "key", "value": "[a]"}]}]}]}),
        ];
        for value in invalid {
            assert!(
                keyboard_settings(&current, &serde_json::to_vec(&value).unwrap()).is_err(),
                "{value}"
            );
        }
        assert!(keyboard_settings(&current, b"broken json").is_err());
        assert!(keyboard_settings(b"{}", &current).is_err());
        assert!(keyboard_settings(
            br#"{"files":[{"name":"Input.ini","sections":{}}]}"#,
            &current
        )
        .is_err());
        let mut duplicate_setting = settings_fixture("[a]", "1024");
        let setting = duplicate_setting["files"][1]["sections"][0]["settings"][0].clone();
        duplicate_setting["files"][1]["sections"][0]["settings"]
            .as_array_mut()
            .unwrap()
            .push(setting);
        assert!(
            keyboard_settings(&current, &serde_json::to_vec(&duplicate_setting).unwrap()).is_err()
        );
        assert!(keyboard_settings(br#"{"files":[{"name":"Game.cfg"}]}"#, &current).is_err());
        let mut duplicate = settings_fixture("[a]", "1024");
        let section = duplicate["files"][1]["sections"][0].clone();
        duplicate["files"][1]["sections"]
            .as_array_mut()
            .unwrap()
            .push(section);
        assert!(keyboard_settings(&current, &serde_json::to_vec(&duplicate).unwrap()).is_err());
    }
    #[test]
    fn keyboard_rejection_leaves_live_files_unchanged_and_no_journal() {
        let f = GuestFixture::new();
        let preset = serde_json::to_vec(&settings_fixture("[a]", "1024")).unwrap();
        assert!(GuestJournal::capture(&f.root, preset.clone(), GuestConfigMode::Keyboard).is_err());
        assert!(!f.file(PERSISTED_SETTINGS_FILENAME).exists());
        let original = serde_json::to_vec(&settings_fixture("[q]", "2560")).unwrap();
        std::fs::write(f.file(PERSISTED_SETTINGS_FILENAME), &original).unwrap();
        assert!(GuestJournal::capture(
            &f.root,
            br#"{"files":[]}"#.to_vec(),
            GuestConfigMode::Keyboard
        )
        .is_err());
        assert_eq!(
            std::fs::read(f.file(PERSISTED_SETTINGS_FILENAME)).unwrap(),
            original
        );
        assert!(!f.journal.exists());
    }
    #[test]
    fn full_mode_is_explicit_and_keeps_original_preset_bytes() {
        let f = GuestFixture::new();
        let original = serde_json::to_vec(&settings_fixture("[q]", "2560")).unwrap();
        std::fs::write(f.file(PERSISTED_SETTINGS_FILENAME), original).unwrap();
        let preset = serde_json::to_vec(&settings_fixture("[a]", "1024")).unwrap();
        let journal =
            GuestJournal::capture(&f.root, preset.clone(), GuestConfigMode::Full).unwrap();
        assert_eq!(journal.files[0].applied.bytes, Some(preset));
        assert!(matches!(
            GuestConfigMode::default(),
            GuestConfigMode::Keyboard
        ));
        assert!(serde_json::from_str::<GuestConfigMode>("\"unknown\"").is_err());
    }
    #[test]
    fn guest_journal_blocks_normal_mutations_even_when_corrupted() {
        let f = GuestFixture::new();
        assert!(ensure_no_guest(&f.journal).is_ok());
        std::fs::write(&f.journal, b"partial journal").unwrap();
        assert!(ensure_no_guest(&f.journal).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn guest_rejects_reparse_points() {
        let f = GuestFixture::new();
        let link = f.dir.join("redirect");
        let result = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&f.root)
            .output()
            .unwrap();
        assert!(result.status.success());
        assert!(GuestJournal::capture(&link, b"{}".to_vec(), GuestConfigMode::Full).is_err());
        std::fs::remove_dir(link).unwrap();
    }
    #[test]
    fn guest_restart_restores_original_bytes_readonly_and_absent_files() {
        let f = GuestFixture::new();
        std::fs::write(f.file(PERSISTED_SETTINGS_FILENAME), b"original").unwrap();
        std::fs::write(f.file("input.ini"), b"keys").unwrap();
        set_readonly_attribute(&f.file(PERSISTED_SETTINGS_FILENAME), true).unwrap();
        let journal = f.capture();
        write_snapshot(
            &f.file(PERSISTED_SETTINGS_FILENAME),
            &journal.files[0].applied,
        )
        .unwrap();
        drop(journal);
        let mut restarted = GuestJournal::load(&f.journal).unwrap();
        assert!(!restarted.changed().unwrap());
        std::fs::write(f.file("game.cfg"), b"created by game").unwrap();
        assert!(restarted.changed().unwrap());
        assert!(restarted.restore(&f.root, &f.journal).is_err());
        assert!(f.journal.exists());
        restarted.files[2].applied = snapshot(&f.file("game.cfg")).unwrap();
        restarted.save(&f.journal).unwrap();
        restarted.restore(&f.root, &f.journal).unwrap();
        assert_eq!(
            std::fs::read(f.file(PERSISTED_SETTINGS_FILENAME)).unwrap(),
            b"original"
        );
        assert!(is_readonly(&f.file(PERSISTED_SETTINGS_FILENAME)).unwrap());
        assert_eq!(
            snapshot(&f.file(PERSISTED_SETTINGS_FILENAME)).unwrap(),
            restarted.files[0].original
        );
        assert_eq!(std::fs::read(f.file("input.ini")).unwrap(), b"keys");
        assert!(!f.file("game.cfg").exists());
        assert!(!f.journal.exists());
    }
    #[test]
    fn guest_partial_restore_keeps_journal_and_retries_without_losing_originals() {
        let f = GuestFixture::new();
        std::fs::write(f.file(PERSISTED_SETTINGS_FILENAME), b"original").unwrap();
        std::fs::write(f.file("input.ini"), b"keys").unwrap();
        let mut journal = f.capture();
        write_snapshot(
            &f.file(PERSISTED_SETTINGS_FILENAME),
            &journal.files[0].applied,
        )
        .unwrap();
        std::fs::write(f.file("input.ini"), b"guest keys").unwrap();
        journal.files[1].applied = snapshot(&f.file("input.ini")).unwrap();
        journal.save(&f.journal).unwrap();
        std::fs::remove_file(f.file("input.ini")).unwrap();
        std::fs::create_dir(f.file("input.ini")).unwrap();
        assert!(journal.restore(&f.root, &f.journal).is_err());
        assert_eq!(
            std::fs::read(f.file(PERSISTED_SETTINGS_FILENAME)).unwrap(),
            b"original"
        );
        assert!(f.journal.exists());
        std::fs::remove_dir(f.file("input.ini")).unwrap();
        std::fs::write(f.file("input.ini"), b"guest keys").unwrap();
        GuestJournal::load(&f.journal)
            .unwrap()
            .restore(&f.root, &f.journal)
            .unwrap();
        assert_eq!(std::fs::read(f.file("input.ini")).unwrap(), b"keys");
    }
    #[test]
    fn guest_failed_apply_and_missing_original_are_recoverable() {
        let f = GuestFixture::new();
        let journal = f.capture();
        std::fs::create_dir(f.file(PERSISTED_SETTINGS_FILENAME)).unwrap();
        assert!(write_snapshot(
            &f.file(PERSISTED_SETTINGS_FILENAME),
            &journal.files[0].applied
        )
        .is_err());
        assert!(f.journal.exists());
        std::fs::remove_dir(f.file(PERSISTED_SETTINGS_FILENAME)).unwrap();
        write_snapshot(
            &f.file(PERSISTED_SETTINGS_FILENAME),
            &journal.files[0].applied,
        )
        .unwrap();
        GuestJournal::load(&f.journal)
            .unwrap()
            .restore(&f.root, &f.journal)
            .unwrap();
        assert!(!f.file(PERSISTED_SETTINGS_FILENAME).exists());
    }
    #[test]
    fn guest_refuses_external_edits_and_other_installation() {
        let f = GuestFixture::new();
        let other = GuestFixture::new();
        std::fs::write(f.file(PERSISTED_SETTINGS_FILENAME), b"original").unwrap();
        let journal = f.capture();
        std::fs::write(f.file(PERSISTED_SETTINGS_FILENAME), b"external edit").unwrap();
        assert!(journal.changed().unwrap());
        assert!(journal.restore(&f.root, &f.journal).is_err());
        assert!(journal.restore(&other.root, &f.journal).is_err());
        assert_eq!(
            std::fs::read(f.file(PERSISTED_SETTINGS_FILENAME)).unwrap(),
            b"external edit"
        );
    }
    #[test]
    fn guest_refuses_moved_install_and_replaced_config_directory() {
        let f = GuestFixture::new();
        let journal = f.capture();
        let moved = f.dir.join("MovedLeague");
        std::fs::rename(&f.root, &moved).unwrap();
        assert!(journal.validate_root(&moved).is_err());
        std::fs::rename(&moved, &f.root).unwrap();
        let config = f.root.join(CONFIG_SUBDIR);
        std::fs::rename(&config, f.root.join("OldConfig")).unwrap();
        std::fs::create_dir(&config).unwrap();
        assert!(journal.validate_root(&f.root).is_err());
    }
    #[test]
    fn guest_rejects_traversal_in_recovery_manifest() {
        let f = GuestFixture::new();
        let mut journal = f.capture();
        journal.files[0].name = "../outside".into();
        journal.save(&f.journal).unwrap();
        assert!(GuestJournal::load(&f.journal).is_err());
    }
    #[test]
    fn preset_ids_cannot_escape_the_preset_directory() {
        assert!(validate_preset_id(&generate_preset_id()).is_ok());
        for id in [
            "",
            "..",
            "../accounts",
            r"..\accounts",
            "C:/accounts",
            "metadata.json",
            "a:b",
            "a/b",
            r"a\b",
        ] {
            assert!(validate_preset_id(id).is_err(), "{id}");
        }
    }
    #[test]
    fn failed_config_replacement_restores_readonly_attribute() {
        let dir =
            std::env::temp_dir().join(format!("rustlm-config-test-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("PersistedSettings.json");
        std::fs::write(&path, b"{}").unwrap();
        set_readonly_attribute(&path, true).unwrap();
        assert!(
            with_writable_swap(&path, |_| Err(AppError::Custom("synthetic failure".into())))
                .is_err()
        );
        assert!(is_readonly(&path).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"{}");
        set_readonly_attribute(&path, false).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
