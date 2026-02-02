use anyhow::Result;
use image::DynamicImage;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::ThumbnailConfig;

/// Manages thumbnail generation and caching
pub struct ThumbnailManager {
    cache_dir: PathBuf,
    size: u32,
}

/// Apply rotation to an image based on degrees (0, 90, 180, 270)
fn apply_rotation(img: DynamicImage, rotation_degrees: i32) -> DynamicImage {
    match rotation_degrees {
        90 => img.rotate90(),
        180 => img.rotate180(),
        270 => img.rotate270(),
        _ => img,
    }
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

    /// Generate a cache filename from the original path and rotation
    /// Uses a hash of the path + rotation to avoid conflicts and ensure
    /// thumbnails are regenerated when rotation changes
    fn cache_path(&self, original: &Path, rotation_degrees: i32) -> PathBuf {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        original.to_string_lossy().hash(&mut hasher);
        // Include rotation in hash so thumbnails are regenerated when rotation changes
        rotation_degrees.hash(&mut hasher);
        let hash = hasher.finish();

        self.cache_dir.join(format!("{:016x}.jpg", hash))
    }

    /// Generate a cache filename without rotation (legacy, for checking old cache)
    #[allow(dead_code)]
    fn cache_path_no_rotation(&self, original: &Path) -> PathBuf {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        original.to_string_lossy().hash(&mut hasher);
        let hash = hasher.finish();

        self.cache_dir.join(format!("{:016x}.jpg", hash))
    }

    /// Check if a cached thumbnail exists for the given path and rotation
    #[allow(dead_code)]
    pub fn has_cached(&self, original: &Path, rotation_degrees: i32) -> bool {
        self.cache_path(original, rotation_degrees).exists()
    }

    /// Get the cached thumbnail path if it exists (with rotation)
    pub fn get_cached_path(&self, original: &Path, rotation_degrees: i32) -> Option<PathBuf> {
        let cache_path = self.cache_path(original, rotation_degrees);
        if cache_path.exists() {
            Some(cache_path)
        } else {
            None
        }
    }

    /// Generate and cache a thumbnail for the given image with rotation applied
    /// rotation_degrees: 0, 90, 180, or 270 degrees clockwise
    /// Returns the path to the cached thumbnail
    pub fn generate(&self, original: &Path, rotation_degrees: i32) -> Result<PathBuf> {
        self.ensure_cache_dir()?;

        let cache_path = self.cache_path(original, rotation_degrees);

        // Skip if already cached
        if cache_path.exists() {
            return Ok(cache_path);
        }

        // Open and resize image
        let img = image::open(original)?;
        let thumbnail = img.thumbnail(self.size, self.size);

        // Apply rotation (from EXIF orientation + user rotation)
        let rotated = apply_rotation(thumbnail, rotation_degrees);

        // Save as JPEG (smaller file size, fast to load)
        rotated.save(&cache_path)?;

        Ok(cache_path)
    }

    /// Invalidate cached thumbnail for an image (all rotations)
    /// Call this when user changes rotation to force regeneration
    pub fn invalidate(&self, original: &Path) {
        // Remove thumbnails for all possible rotations
        for rotation in [0, 90, 180, 270] {
            let cache_path = self.cache_path(original, rotation);
            let _ = fs::remove_file(cache_path);
        }
        // Also remove legacy non-rotation thumbnail
        let legacy_path = self.cache_path_no_rotation(original);
        let _ = fs::remove_file(legacy_path);
    }

    /// Get cache directory path
    #[allow(dead_code)]
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}
