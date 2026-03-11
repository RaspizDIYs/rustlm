use std::path::PathBuf;

use crate::error::AppError;
use crate::models::rune::RunePage;

pub struct RunePagesStorage {
    file_path: PathBuf,
}

impl RunePagesStorage {
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

        Self { file_path }
    }

    fn ensure_dir(&self) {
        if let Some(parent) = self.file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    pub fn load_all(&self) -> Vec<RunePage> {
        if let Ok(content) = std::fs::read_to_string(&self.file_path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    pub fn save_all(&self, pages: &[RunePage]) -> Result<(), AppError> {
        self.ensure_dir();

        // Create backup
        if self.file_path.exists() {
            let backup = self.file_path.with_extension("json.bak");
            let _ = std::fs::copy(&self.file_path, &backup);
        }

        let json = serde_json::to_string_pretty(pages)?;
        std::fs::write(&self.file_path, json)?;
        Ok(())
    }

    pub fn save(&self, page: RunePage) -> Result<(), AppError> {
        let mut pages = self.load_all();

        if let Some(existing) = pages.iter_mut().find(|p| p.name == page.name) {
            *existing = page;
        } else {
            pages.push(page);
        }

        self.save_all(&pages)
    }

    pub fn delete(&self, page_name: &str) -> Result<(), AppError> {
        let mut pages = self.load_all();
        pages.retain(|p| p.name != page_name);
        self.save_all(&pages)
    }
}
