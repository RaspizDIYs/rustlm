use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::Value;

use crate::error::AppError;
use crate::models::settings::UpdateSettings;

pub struct SettingsService {
    settings_path: PathBuf,
    update_settings_path: PathBuf,
    cache: Mutex<HashMap<String, Value>>,
}

impl SettingsService {
    pub fn new() -> Self {
        let base_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LolManager");

        fs::create_dir_all(&base_dir).ok();

        let settings_path = base_dir.join("settings.json");
        let update_settings_path = base_dir.join("update-settings.json");

        let cache = if settings_path.exists() {
            match fs::read_to_string(&settings_path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => HashMap::new(),
            }
        } else {
            HashMap::new()
        };

        Self {
            settings_path,
            update_settings_path,
            cache: Mutex::new(cache),
        }
    }

    pub fn load_setting<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
        default: T,
    ) -> T {
        let cache = self.cache.lock().unwrap();
        match cache.get(key) {
            Some(value) => serde_json::from_value(value.clone()).unwrap_or(default),
            None => default,
        }
    }

    pub fn save_setting<T: serde::Serialize>(
        &self,
        key: &str,
        value: &T,
    ) -> Result<(), AppError> {
        let json_value = serde_json::to_value(value)
            .map_err(|e| AppError::Custom(e.to_string()))?;

        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(key.to_string(), json_value);
        }

        self.flush()?;
        Ok(())
    }

    pub fn load_update_settings(&self) -> UpdateSettings {
        if self.update_settings_path.exists() {
            match fs::read_to_string(&self.update_settings_path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => UpdateSettings::default(),
            }
        } else {
            UpdateSettings::default()
        }
    }

    pub fn save_update_settings(&self, settings: &UpdateSettings) -> Result<(), AppError> {
        let content = serde_json::to_string_pretty(settings)?;
        fs::write(&self.update_settings_path, content)?;
        Ok(())
    }

    fn flush(&self) -> Result<(), AppError> {
        let cache = self.cache.lock().unwrap();
        let content = serde_json::to_string_pretty(&*cache)?;
        fs::write(&self.settings_path, content)?;
        Ok(())
    }

    pub fn export_settings_json_map(&self) -> serde_json::Map<String, Value> {
        let cache = self.cache.lock().unwrap();
        cache
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    pub fn replace_settings_json_map(
        &self,
        map: serde_json::Map<String, Value>,
    ) -> Result<(), AppError> {
        let mut new_cache: HashMap<String, Value> = HashMap::new();
        for (k, v) in map {
            new_cache.insert(k, v);
        }
        {
            let mut cache = self.cache.lock().unwrap();
            *cache = new_cache;
        }
        self.flush()
    }
}
