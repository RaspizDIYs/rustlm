use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Local;

const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024; // 10MB

pub struct FileLogger {
    log_path: PathBuf,
    lock: Mutex<()>,
}

impl FileLogger {
    pub fn new() -> Self {
        let base_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LolManager");

        fs::create_dir_all(&base_dir).ok();

        Self {
            log_path: base_dir.join("debug.log"),
            lock: Mutex::new(()),
        }
    }

    pub fn log_path(&self) -> &PathBuf {
        &self.log_path
    }

    pub fn info(&self, message: &str) {
        self.write_log("INFO", message);
    }

    pub fn error(&self, message: &str) {
        self.write_log("ERROR", message);
    }

    pub fn warning(&self, message: &str) {
        self.write_log("WARN", message);
    }

    pub fn debug(&self, message: &str) {
        self.write_log("DEBUG", message);
    }

    pub fn http_request(&self, method: &str, url: &str, status_code: u16, response: Option<&str>) {
        let truncated = response
            .map(|r| if r.len() > 100 { &r[..100] } else { r })
            .unwrap_or("");
        let msg = format!(
            "🌐 {} {} → {} {}",
            method, url, status_code, truncated
        );
        self.write_log("HTTP", &msg);
    }

    pub fn process_event(&self, process_name: &str, action: &str, details: Option<&str>) {
        let msg = match details {
            Some(d) => format!("⚙️ [{}] {} — {}", process_name, action, d),
            None => format!("⚙️ [{}] {}", process_name, action),
        };
        self.write_log("PROC", &msg);
    }

    pub fn ui_event(&self, component: &str, action: &str, result: Option<&str>) {
        let msg = match result {
            Some(r) => format!("🖱️ [{}] {} → {}", component, action, r),
            None => format!("🖱️ [{}] {}", component, action),
        };
        self.write_log("UI", &msg);
    }

    pub fn login_flow(&self, step: &str, details: Option<&str>) {
        let msg = match details {
            Some(d) => format!("🔐 {} — {}", step, d),
            None => format!("🔐 {}", step),
        };
        self.write_log("LOGIN", &msg);
    }

    pub fn get_log_lines(&self) -> Vec<String> {
        match fs::read_to_string(&self.log_path) {
            Ok(content) => content.lines().map(String::from).collect(),
            Err(_) => Vec::new(),
        }
    }

    fn write_log(&self, level: &str, message: &str) {
        let _guard = self.lock.lock().unwrap();

        // Rotate if too large
        if let Ok(metadata) = fs::metadata(&self.log_path) {
            if metadata.len() > MAX_LOG_SIZE {
                fs::write(&self.log_path, "").ok();
            }
        }

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let line = format!("[{}] [{}] {}\n", timestamp, level, message);

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
        {
            file.write_all(line.as_bytes()).ok();
        }
    }
}
