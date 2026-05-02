#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

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
}

impl LolConfigService {
    pub fn new(riot_client: Arc<RiotClientService>, logger: Arc<FileLogger>) -> Self {
        Self { riot_client, logger }
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
        Ok(Self::presets_dir()?.join(format!("{}.json", id)))
    }

    /// Path to the live `PersistedSettings.json` inside LoL install.
    fn persisted_settings_path(&self) -> Option<PathBuf> {
        let _ = self;
        let install_dir = RiotClientService::find_lol_install_dir()?;
        Some(install_dir.join(CONFIG_SUBDIR).join(PERSISTED_SETTINGS_FILENAME))
    }

    // --- Metadata I/O ---

    fn read_index() -> Result<PresetsIndex, AppError> {
        let path = Self::metadata_path()?;
        if !path.exists() {
            return Ok(PresetsIndex::default());
        }
        let raw = std::fs::read_to_string(&path)?;
        let index: PresetsIndex = serde_json::from_str(&raw).unwrap_or_default();
        Ok(index)
    }

    fn write_index(index: &PresetsIndex) -> Result<(), AppError> {
        let path = Self::metadata_path()?;
        let raw = serde_json::to_string_pretty(index)?;
        std::fs::write(&path, raw)?;
        Ok(())
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
        let path = self
            .persisted_settings_path()
            .ok_or_else(|| AppError::Custom("Установка League of Legends не найдена".into()))?;
        if !path.exists() {
            return Err(AppError::Custom(
                "Файл PersistedSettings.json ещё не создан — запустите игру хотя бы один раз".into(),
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
        let mut index = Self::read_index()?;
        // Drop orphans (preset metadata without backing file on disk).
        let dir = Self::presets_dir()?;
        let before = index.presets.len();
        index.presets.retain(|p| dir.join(format!("{}.json", p.id)).exists());
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
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return Err(AppError::Custom("Имя пресета не может быть пустым".into()));
        }
        let src = self
            .persisted_settings_path()
            .ok_or_else(|| AppError::Custom("Установка League of Legends не найдена".into()))?;
        if !src.exists() {
            return Err(AppError::Custom(
                "Файл PersistedSettings.json ещё не создан — запустите игру хотя бы один раз".into(),
            ));
        }
        let raw = std::fs::read_to_string(&src)?;
        // Validate JSON before storing — corrupted files should not silently propagate.
        let _: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| AppError::Custom(format!("PersistedSettings.json повреждён: {}", e)))?;

        let id = generate_preset_id();
        let dst = Self::preset_file_path(&id)?;
        std::fs::write(&dst, raw)?;

        let meta = PresetMeta {
            id: id.clone(),
            name: trimmed,
            created_at: Utc::now().to_rfc3339(),
            source_app_version: Self::app_version(),
        };
        let mut index = Self::read_index()?;
        index.presets.insert(0, meta.clone());
        Self::write_index(&index)?;
        self.logger
            .info(&format!("lol_config: created preset {} ({})", meta.name, id));
        Ok(meta)
    }

    pub fn apply_preset(&self, id: String) -> Result<(), AppError> {
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
        with_writable_swap(&dst, |target| {
            std::fs::write(target, &raw)?;
            Ok(())
        })?;

        self.logger
            .info(&format!("lol_config: applied preset {} → {}", id, dst.display()));
        Ok(())
    }

    pub fn delete_preset(&self, id: String) -> Result<(), AppError> {
        let path = Self::preset_file_path(&id)?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        let mut index = Self::read_index()?;
        index.presets.retain(|p| p.id != id);
        Self::write_index(&index)?;
        self.logger.info(&format!("lol_config: deleted preset {}", id));
        Ok(())
    }

    pub fn export_preset(&self, id: String, dst_path: String) -> Result<(), AppError> {
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
        self.logger
            .info(&format!("lol_config: exported preset {} → {}", id, dst_path));
        Ok(())
    }

    pub fn import_preset(&self, src_path: String) -> Result<PresetMeta, AppError> {
        let raw = std::fs::read_to_string(&src_path)?;
        let payload: LolCfgFile = serde_json::from_str(&raw)
            .map_err(|e| AppError::Custom(format!("Файл не является валидным .lolcfg: {}", e)))?;
        if payload.format != LOLCFG_FORMAT_TAG {
            return Err(AppError::Custom(
                "Файл не является пресетом RustLM (неверный формат)".into(),
            ));
        }
        if payload.version > LOLCFG_FORMAT_VERSION {
            return Err(AppError::Custom(format!(
                "Версия .lolcfg ({}) новее, чем поддерживает приложение",
                payload.version
            )));
        }

        let id = generate_preset_id();
        let dst = Self::preset_file_path(&id)?;
        let settings_raw = serde_json::to_string_pretty(&payload.settings)?;
        std::fs::write(&dst, settings_raw)?;

        let meta = PresetMeta {
            id: id.clone(),
            name: payload.name,
            created_at: Utc::now().to_rfc3339(),
            source_app_version: payload.source_app_version,
        };
        let mut index = Self::read_index()?;
        index.presets.insert(0, meta.clone());
        Self::write_index(&index)?;
        self.logger
            .info(&format!("lol_config: imported preset {} ({})", meta.name, id));
        Ok(meta)
    }
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
        GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_READONLY,
        FILE_FLAGS_AND_ATTRIBUTES, INVALID_FILE_ATTRIBUTES,
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
