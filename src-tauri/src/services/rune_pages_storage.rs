use std::path::PathBuf;
use std::sync::Mutex;

use crate::error::AppError;
use crate::models::rune::RunePage;

pub struct RunePagesStorage {
    file_path: PathBuf,
    lock: Mutex<()>,
}

impl RunePagesStorage {
    #[cfg(test)]
    pub(crate) fn for_test(file_path: PathBuf) -> Self {
        Self { file_path, lock: Mutex::new(()) }
    }
    pub fn new() -> Self {
        let roaming = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LolManager");
        let legacy = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LolManager");

        let file_path = roaming.join("rune-pages.json");

        // Migrate from legacy location if needed
        if !file_path.exists() {
            let legacy_path = legacy.join("rune-pages.json");
            if legacy_path.exists() {
                let _ = std::fs::create_dir_all(&roaming);
                let _ = std::fs::copy(&legacy_path, &file_path);
            }
        }

        Self { file_path, lock: Mutex::new(()) }
    }

    fn read_pages(&self) -> Result<Vec<RunePage>, AppError> {
        let backup = self.file_path.with_extension("json.bak");
        if !self.file_path.exists() && !backup.exists() { return Ok(Vec::new()); }
        for path in [&self.file_path, &backup] {
            if let Ok(bytes) = std::fs::read(path) {
                if let Ok(pages) = serde_json::from_slice(&bytes) { return Ok(pages); }
            }
        }
        Err(AppError::Custom("Не удалось прочитать страницы рун; файлы сохранены без изменений".into()))
    }

    pub fn load_all(&self) -> Vec<RunePage> {
        let _guard = self.lock.lock().unwrap();
        self.read_pages().unwrap_or_default()
    }

    fn write_pages(&self, pages: &[RunePage]) -> Result<(), AppError> {
        if let Some(parent) = self.file_path.parent() { std::fs::create_dir_all(parent)?; }
        if let Ok(bytes) = std::fs::read(&self.file_path) {
            if serde_json::from_slice::<Vec<RunePage>>(&bytes).is_ok() {
                super::network::atomic_write(&self.file_path.with_extension("json.bak"), &bytes).map_err(AppError::Custom)?;
            }
        }
        let json = serde_json::to_vec_pretty(pages)?;
        super::network::atomic_write(&self.file_path, &json).map_err(AppError::Custom)
    }

    pub fn save_all(&self, pages: &[RunePage]) -> Result<(), AppError> {
        let _guard = self.lock.lock().unwrap();
        self.write_pages(pages)
    }

    pub fn save(&self, page: RunePage) -> Result<(), AppError> {
        let _guard = self.lock.lock().unwrap();
        let mut pages = self.read_pages()?;
        if let Some(existing) = pages.iter_mut().find(|p| p.name == page.name) { *existing = page; }
        else { pages.push(page); }
        self.write_pages(&pages)
    }

    pub fn delete(&self, page_name: &str) -> Result<(), AppError> {
        let _guard = self.lock.lock().unwrap();
        let mut pages = self.read_pages()?;
        pages.retain(|p| p.name != page_name);
        self.write_pages(&pages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_saves_keep_both_pages_and_corrupt_primary_recovers_backup() {
        let dir = std::env::temp_dir().join(format!("rustlm-runes-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        let storage = std::sync::Arc::new(RunePagesStorage::for_test(dir.join("runes.json")));
        let gate = std::sync::Arc::new(std::sync::Barrier::new(3));
        let tasks: Vec<_> = ["first", "second"].into_iter().map(|name| {
            let storage = storage.clone(); let gate = gate.clone();
            std::thread::spawn(move || {gate.wait(); storage.save(RunePage {name:name.into(), ..Default::default()}).unwrap();})
        }).collect();
        gate.wait(); for task in tasks {task.join().unwrap();}
        assert_eq!(storage.load_all().len(), 2);
        storage.save_all(&storage.load_all()).unwrap();
        std::fs::write(&storage.file_path, b"{broken").unwrap();
        assert_eq!(storage.load_all().len(), 2);
        storage.save(RunePage {name:"third".into(), ..Default::default()}).unwrap();
        assert_eq!(storage.load_all().len(), 3);
        std::fs::write(&storage.file_path, b"{broken").unwrap();
        std::fs::write(storage.file_path.with_extension("json.bak"), b"{broken").unwrap();
        assert!(storage.delete("first").is_err());
        assert_eq!(std::fs::read(&storage.file_path).unwrap(), b"{broken");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
