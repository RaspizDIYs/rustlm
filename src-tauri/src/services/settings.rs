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

        let cache = read_settings(&settings_path).unwrap_or_default();

        Self {
            settings_path,
            update_settings_path,
            cache: Mutex::new(cache),
        }
    }

    pub fn load_setting<T: serde::de::DeserializeOwned>(&self, key: &str, default: T) -> T {
        let cache = self.cache.lock().unwrap();
        match cache.get(key) {
            Some(value) => serde_json::from_value(value.clone()).unwrap_or(default),
            None => default,
        }
    }

    pub fn save_setting<T: serde::Serialize>(&self, key: &str, value: &T) -> Result<(), AppError> {
        let json_value =
            serde_json::to_value(value).map_err(|e| AppError::Custom(e.to_string()))?;

        let mut cache = self.cache.lock().unwrap();
        let mut next = cache.clone();
        next.insert(key.to_string(), json_value);
        write_settings(&self.settings_path, &next)?;
        *cache = next;
        Ok(())
    }

    pub fn load_update_settings(&self) -> UpdateSettings {
        read_settings(&self.update_settings_path).unwrap_or_default()
    }

    pub fn save_update_settings(&self, settings: &UpdateSettings) -> Result<(), AppError> {
        let _guard = self.cache.lock().unwrap();
        write_settings(&self.update_settings_path, settings)
    }

    pub fn export_settings_json_map(&self) -> serde_json::Map<String, Value> {
        let cache = self.cache.lock().unwrap();
        cache.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    pub fn replace_settings_json_map(
        &self,
        map: serde_json::Map<String, Value>,
    ) -> Result<(), AppError> {
        let mut new_cache: HashMap<String, Value> = HashMap::new();
        for (k, v) in map {
            new_cache.insert(k, v);
        }
        let mut cache = self.cache.lock().unwrap();
        write_settings(&self.settings_path, &new_cache)?;
        *cache = new_cache;
        Ok(())
    }
}

fn read_settings<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Option<T> {
    [path.to_path_buf(), path.with_extension("json.bak")].into_iter()
        .find_map(|file| fs::read(file).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()))
}

fn write_settings<T: serde::Serialize>(path: &std::path::Path, value: &T) -> Result<(), AppError> {
    let bytes = serde_json::to_vec_pretty(value)?;
    if let Ok(previous) = fs::read(path) {
        if serde_json::from_slice::<Value>(&previous).is_ok() {
            super::network::atomic_write(&path.with_extension("json.bak"), &previous).map_err(AppError::Custom)?;
        }
    }
    super::network::atomic_write(path, &bytes).map_err(AppError::Custom)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn folder() -> PathBuf {
        let path = std::env::temp_dir().join(format!("rustlm-settings-test-{}", rand::random::<u64>()));
        fs::create_dir_all(&path).unwrap(); path
    }
    #[test]
    fn damaged_primary_recovers_the_last_valid_backup() {
        let dir = folder(); let path = dir.join("settings.json");
        write_settings(&path, &serde_json::json!({"HideLogins":true})).unwrap();
        write_settings(&path, &serde_json::json!({"HideLogins":false})).unwrap();
        fs::write(&path, b"{broken").unwrap();
        assert_eq!(read_settings::<Value>(&path).unwrap()["HideLogins"], true);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn failed_write_leaves_the_in_memory_settings_unchanged() {
        let dir = folder();
        let service = SettingsService {settings_path:dir.join("missing/settings.json"), update_settings_path:dir.join("update.json"), cache:Mutex::new(HashMap::from([("value".into(), Value::from(1))]))};
        assert!(service.save_setting("value", &2).is_err());
        assert_eq!(service.load_setting("value", 0), 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
