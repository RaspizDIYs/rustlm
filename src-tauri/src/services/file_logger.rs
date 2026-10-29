#![allow(dead_code)]

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Local;

const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024; // 10MB

pub struct FileLogger {
    log_path: PathBuf,
    lock: Mutex<()>,
    generation: AtomicU64,
}

impl FileLogger {
    pub fn new() -> Self {
        let base_dir = super::repair_paths::root()
            .unwrap_or_else(|_| std::env::temp_dir().join("RustLM/Repair Tools"))
            .join("logs");

        fs::create_dir_all(&base_dir).ok();

        Self {
            log_path: base_dir.join("debug.log"),
            lock: Mutex::new(()),
            generation: AtomicU64::new(1),
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
        let msg = format!("🌐 {} {} → {} {}", method, url, status_code, truncated);
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

    pub fn clear(&self) -> Result<(), std::io::Error> {
        let _guard = self.lock.lock().unwrap();
        fs::write(&self.log_path, "")?;
        self.generation.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn chunk(&self, offset: u64, generation: u64) -> Result<LogChunk, String> {
        let _guard = self.lock.lock().unwrap();
        let current = self.generation.load(Ordering::Relaxed);
        let file = match fs::File::open(&self.log_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(LogChunk {lines: vec![], offset: 0, generation: current, reset: true}),
            Err(error) => return Err(error.to_string()),
        };
        read_chunk(file, offset, generation, current).map_err(|e| e.to_string())
    }

    fn write_log(&self, level: &str, message: &str) {
        let _guard = self.lock.lock().unwrap();

        // Rotate if too large
        if let Ok(metadata) = fs::metadata(&self.log_path) {
            if metadata.len() > MAX_LOG_SIZE {
                if fs::write(&self.log_path, "").is_ok() { self.generation.fetch_add(1, Ordering::Relaxed); }
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

#[derive(serde::Serialize)]
pub struct LogChunk { pub lines: Vec<String>, pub offset: u64, pub generation: u64, pub reset: bool }

fn read_chunk(file: fs::File, offset: u64, generation: u64, current: u64) -> std::io::Result<LogChunk> {
    let length = file.metadata()?.len();
    let reset = generation != current || offset > length;
    // Only the recent tail is needed by the viewer; the complete file stays exportable.
    let start = if reset { length.saturating_sub(256 * 1024) } else { offset };
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    if reset && start > 0 { reader.read_until(b'\n', &mut bytes)?; bytes.clear(); }
    let mut lines = Vec::new();
    while reader.stream_position()?.saturating_sub(start) < 256 * 1024 {
        bytes.clear();
        if reader.read_until(b'\n', &mut bytes)? == 0 { break; }
        lines.push(String::from_utf8_lossy(&bytes).trim_end_matches(['\r','\n']).to_string());
    }
    if lines.len() > 2000 { lines.drain(..lines.len()-2000); }
    Ok(LogChunk { lines, offset: reader.stream_position()?, generation: current, reset })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunks_read_only_new_lines_and_reset_after_clear() {
        let dir = std::env::temp_dir().join(format!("rustlm-log-test-{}", rand::random::<u64>()));
        fs::create_dir_all(&dir).unwrap();
        let logger = FileLogger { log_path:dir.join("debug.log"), lock:Mutex::new(()), generation:AtomicU64::new(1) };
        logger.info("Первая строка");
        let first = logger.chunk(0,0).unwrap();
        assert!(first.reset); assert_eq!(first.lines.len(),1);
        assert!(logger.chunk(first.offset, first.generation).unwrap().lines.is_empty());
        logger.warning("Вторая строка");
        let next = logger.chunk(first.offset, first.generation).unwrap();
        assert_eq!(next.lines.len(),1); assert!(next.lines[0].ends_with("Вторая строка")); assert!(!next.reset);
        logger.clear().unwrap(); logger.error("После очистки");
        let reset = logger.chunk(next.offset,next.generation).unwrap();
        assert!(reset.reset); assert_eq!(reset.lines.len(),1); assert!(reset.lines[0].ends_with("После очистки"));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn large_logs_return_a_bounded_tail() {
        let dir = std::env::temp_dir().join(format!("rustlm-log-test-{}", rand::random::<u64>())); fs::create_dir_all(&dir).unwrap();
        let path = dir.join("debug.log"); fs::write(&path,"[INFO] строка\n".repeat(50000)).unwrap();
        let result = read_chunk(fs::File::open(&path).unwrap(),0,0,1).unwrap();
        assert!(result.reset); assert_eq!(result.lines.len(),2000);
        assert!(result.lines.iter().all(|line| line == "[INFO] строка"));
        fs::remove_dir_all(dir).unwrap();
    }
}
