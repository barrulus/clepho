use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::ThumbnailConfig;

/// Manages thumbnail generation and caching
pub struct ThumbnailManager {
    cache_dir: PathBuf,
    size: u32,
}

impl ThumbnailManager {
    pub fn new(config: &ThumbnailConfig) -> Self {
        Self {
            cache_dir: config.path.clone(),
            size: config.size,
        }
    }

    /// Ensure cache directory exists
    fn ensure_cache_dir(&self) -> Result<()> {
        if !self.cache_dir.exists() {
            fs::create_dir_all(&self.cache_dir)?;
        }
        Ok(())
    }

    /// Generate a cache filename from the original path
    /// Uses a hash of the path to avoid conflicts
    fn cache_path(&self, original: &Path) -> PathBuf {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        original.to_string_lossy().hash(&mut hasher);
        let hash = hasher.finish();

        self.cache_dir.join(format!("{:016x}.jpg", hash))
    }

    /// Check if a cached thumbnail exists for the given path
    pub fn has_cached(&self, original: &Path) -> bool {
        self.cache_path(original).exists()
    }

    /// Get the cached thumbnail path if it exists
    pub fn get_cached_path(&self, original: &Path) -> Option<PathBuf> {
        let cache_path = self.cache_path(original);
        if cache_path.exists() {
            Some(cache_path)
        } else {
            None
        }
    }

    /// Generate and cache a thumbnail for the given image
    /// Returns the path to the cached thumbnail
    pub fn generate(&self, original: &Path) -> Result<PathBuf> {
        self.ensure_cache_dir()?;

        let cache_path = self.cache_path(original);

        // Skip if already cached
        if cache_path.exists() {
            return Ok(cache_path);
        }

        // Open and resize image
        let img = image::open(original)?;
        let thumbnail = img.thumbnail(self.size, self.size);

        // Save as JPEG (smaller file size, fast to load)
        thumbnail.save(&cache_path)?;

        Ok(cache_path)
    }

    /// Get cache directory path
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}
