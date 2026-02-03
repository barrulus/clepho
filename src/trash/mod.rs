use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::TrashConfig;

pub struct TrashManager {
    config: TrashConfig,
}

/// Entry for file-system based trash listing (alternative to database tracking)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TrashEntry {
    pub trash_path: PathBuf,
    pub original_path: PathBuf,
    pub trashed_at: DateTime<Utc>,
    pub size: u64,
}

/// Result of a cleanup operation
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct CleanupResult {
    pub files_deleted: usize,
    pub bytes_freed: u64,
}

impl TrashManager {
    pub fn new(config: TrashConfig) -> Self {
        Self { config }
    }

    /// Ensure the trash directory exists
    fn ensure_trash_dir(&self) -> Result<()> {
        if !self.config.path.exists() {
            fs::create_dir_all(&self.config.path)
                .context("Failed to create trash directory")?;
        }
        Ok(())
    }

    /// Generate a unique trash filename to avoid conflicts.
    /// Uses a global atomic counter to ensure uniqueness even when called
    /// from multiple threads within the same second.
    fn generate_trash_name(&self, original: &Path) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let timestamp = Utc::now().timestamp();
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let original_name = original.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let extension = original.extension()
            .map(|s| format!(".{}", s.to_string_lossy()))
            .unwrap_or_default();

        let trash_name = format!("{}_{}_{}{}", original_name, timestamp, seq, extension);
        self.config.path.join(trash_name)
    }

    /// Move file to trash, returns new path
    pub fn move_to_trash(&self, path: &Path) -> Result<PathBuf> {
        self.ensure_trash_dir()?;

        let trash_path = self.generate_trash_name(path);

        // Try rename first (fastest, same filesystem)
        match fs::rename(path, &trash_path) {
            Ok(_) => Ok(trash_path),
            Err(_) => {
                // Fall back to copy + delete for cross-filesystem moves
                fs::copy(path, &trash_path)
                    .context("Failed to copy file to trash")?;
                fs::remove_file(path)
                    .context("Failed to remove original file after copying to trash")?;
                Ok(trash_path)
            }
        }
    }

    /// Restore file from trash to original location
    pub fn restore(&self, trash_path: &Path, original_path: &Path) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = original_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .context("Failed to create parent directory for restore")?;
            }
        }

        // Check if original path already exists
        if original_path.exists() {
            anyhow::bail!("Cannot restore: file already exists at {}", original_path.display());
        }

        // Try rename first
        match fs::rename(trash_path, original_path) {
            Ok(_) => Ok(()),
            Err(_) => {
                // Fall back to copy + delete
                fs::copy(trash_path, original_path)
                    .context("Failed to copy file from trash")?;
                fs::remove_file(trash_path)
                    .context("Failed to remove file from trash after copying")?;
                Ok(())
            }
        }
    }

    /// Permanently delete a trashed file
    pub fn delete_permanently(&self, trash_path: &Path) -> Result<()> {
        fs::remove_file(trash_path)
            .context("Failed to permanently delete file")?;
        Ok(())
    }

    /// Get all files in trash directory (for simple listing without DB)
    #[allow(dead_code)]
    pub fn list_trash_files(&self) -> Result<Vec<PathBuf>> {
        if !self.config.path.exists() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();
        for entry in fs::read_dir(&self.config.path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                files.push(entry.path());
            }
        }
        Ok(files)
    }

    /// Get total trash size in bytes (file-system based, alternative to DB query)
    #[allow(dead_code)]
    pub fn total_size(&self) -> Result<u64> {
        if !self.config.path.exists() {
            return Ok(0);
        }

        let mut total = 0u64;
        for entry in fs::read_dir(&self.config.path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                total += entry.metadata()?.len();
            }
        }
        Ok(total)
    }

    /// Get trash path
    #[allow(dead_code)]
    pub fn trash_path(&self) -> &Path {
        &self.config.path
    }

    /// Get max age in days
    pub fn max_age_days(&self) -> u32 {
        self.config.max_age_days
    }

    /// Get max size in bytes
    pub fn max_size_bytes(&self) -> u64 {
        self.config.max_size_bytes
    }
}
