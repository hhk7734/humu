use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::config::humu_dir;

const MAX_LOG_SIZE: u64 = 1_000_000; // 1MB

static LOGGER: Mutex<Option<File>> = Mutex::new(None);

/// Initialize the logger. Truncates the log file if it exceeds the size limit.
pub fn init() {
    let path = log_path();
    // Truncate if over size limit
    if let Ok(meta) = fs::metadata(&path) {
        if meta.len() > MAX_LOG_SIZE {
            let _ = fs::write(&path, "");
        }
    }
    if let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) {
        *LOGGER.lock().unwrap() = Some(file);
    }
}

/// Returns the log file path.
pub fn log_path() -> PathBuf {
    humu_dir().join("humu.log")
}

/// Write a log message with timestamp.
pub fn write(msg: &str) {
    let mut guard = LOGGER.lock().unwrap();
    if let Some(ref mut file) = *guard {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = writeln!(file, "[{now}] {msg}");
    }
}

#[macro_export]
macro_rules! humu_log {
    ($($arg:tt)*) => {
        $crate::log::write(&format!($($arg)*))
    };
}
