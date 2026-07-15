use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_FILE: OnceLock<Mutex<File>> = OnceLock::new();

#[cfg(test)]
static TEST_LOG_LINES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

pub fn init(path: PathBuf) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create log directory {}: {error}",
                parent.display()
            )
        })?;
    }

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("failed to open log file {}: {error}", path.display()))?;

    let _ = LOG_FILE.set(Mutex::new(file));
    Ok(())
}

pub fn log_line(message: impl AsRef<str>) {
    let message = message.as_ref();

    #[cfg(test)]
    if let Ok(mut lines) = TEST_LOG_LINES.get_or_init(|| Mutex::new(Vec::new())).lock() {
        lines.push(message.to_string());
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let line = format!("[{timestamp}] {message}\n");

    if let Some(file) = LOG_FILE.get() {
        if let Ok(mut file) = file.lock() {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
            return;
        }
    }

    #[cfg(debug_assertions)]
    eprint!("{line}");
}

#[cfg(test)]
pub fn captured_lines_for_test() -> Vec<String> {
    TEST_LOG_LINES
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .map(|lines| lines.clone())
        .unwrap_or_default()
}
