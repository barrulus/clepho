//! JSONL pipeline log (spec §8.7).
//! Path: ${XDG_STATE_HOME:-~/.local/state}/clepho/logs/clepho-YYYYMMDD.jsonl

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct LogRecord<'a> {
    pub ts: String,
    pub level: &'a str,
    pub component: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo_path: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_class: Option<&'a str>,
    pub message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<&'a serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[allow(dead_code)]
pub struct JsonlAppender {
    dir: PathBuf,
    handle: Mutex<Option<(String, std::fs::File)>>,
}

#[allow(dead_code)]
impl JsonlAppender {
    pub fn new(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        create_dir_all(&dir).with_context(|| format!("create log dir {:?}", dir))?;
        Ok(Self {
            dir,
            handle: Mutex::new(None),
        })
    }

    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let mut p = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
                p.push(".local/state");
                p
            });
        base.join("clepho/logs")
    }

    pub fn append(&self, rec: &LogRecord) -> Result<()> {
        let today = Utc::now().format("%Y%m%d").to_string();
        let mut guard = self.handle.lock().unwrap();
        let needs_new = match &*guard {
            Some((d, _)) => d != &today,
            None => true,
        };
        if needs_new {
            let path = self.dir.join(format!("clepho-{}.jsonl", today));
            let f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .with_context(|| format!("open log {:?}", path))?;
            *guard = Some((today.clone(), f));
        }
        let (_, file) = guard.as_mut().unwrap();
        serde_json::to_writer(&mut *file, rec)?;
        writeln!(file)?;
        Ok(())
    }

    /// Delete .jsonl files older than `retention_days`.
    /// Called by daemon on startup and once per 24h.
    pub fn rotate(&self, retention_days: u64) -> Result<()> {
        let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("clepho-") {
                continue;
            }
            let metadata = entry.metadata()?;
            let modified = metadata.modified()?;
            let modified_dt: chrono::DateTime<Utc> = modified.into();
            if modified_dt < cutoff {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn append_writes_jsonl_lines() {
        let d = tempdir().unwrap();
        let app = JsonlAppender::new(d.path()).unwrap();
        let rec = LogRecord {
            ts: "2026-05-04T12:00:00Z".into(),
            level: "info",
            component: "pipeline.scan",
            folder: Some("/photos"),
            photo_id: Some(1),
            photo_path: Some("/photos/a.jpg"),
            stage: Some("scan"),
            error_class: None,
            message: "scan complete",
            context: None,
            duration_ms: Some(42),
        };
        app.append(&rec).unwrap();
        app.append(&rec).unwrap();

        let files: Vec<_> = std::fs::read_dir(d.path()).unwrap().collect();
        assert_eq!(files.len(), 1);
        let p = files[0].as_ref().unwrap().path();
        let content = std::fs::read_to_string(&p).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed["component"], "pipeline.scan");
        assert_eq!(parsed["duration_ms"], 42);
        assert_eq!(parsed["photo_id"], 1);
        assert!(parsed.get("error_class").is_none(), "None fields skipped");
    }

    #[test]
    fn rotate_removes_old_files() {
        let d = tempdir().unwrap();
        let app = JsonlAppender::new(d.path()).unwrap();

        // Write a current log so something exists.
        let rec = LogRecord {
            ts: "2026-05-04T12:00:00Z".into(),
            level: "info",
            component: "test",
            folder: None,
            photo_id: None,
            photo_path: None,
            stage: None,
            error_class: None,
            message: "x",
            context: None,
            duration_ms: None,
        };
        app.append(&rec).unwrap();

        // Create an "old" file by writing it and backdating its mtime.
        let old_path = d.path().join("clepho-19990101.jsonl");
        std::fs::write(&old_path, "old\n").unwrap();
        let old_time =
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(915_148_800); // 1999-01-01
        let f = std::fs::File::open(&old_path).unwrap();
        f.set_modified(old_time).unwrap();

        app.rotate(7).unwrap();
        assert!(!old_path.exists(), "old file should be deleted");
    }
}
