use anyhow::Result;
use std::path::PathBuf;
use walkdir::WalkDir;

pub fn discover_images(directory: &PathBuf, extensions: &[String]) -> Result<Vec<PathBuf>> {
    let mut images = Vec::new();

    for entry in WalkDir::new(directory)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext_lower = ext.to_string_lossy().to_lowercase();
                if extensions.iter().any(|e| e.to_lowercase() == ext_lower) {
                    images.push(path.to_path_buf());
                }
            }
        }
    }

    // Sort by path for consistent ordering
    images.sort();

    Ok(images)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::tempdir;

    #[test]
    fn test_discover_images() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_path_buf();

        // Create some test files
        File::create(dir.path().join("photo1.jpg")).unwrap();
        File::create(dir.path().join("photo2.png")).unwrap();
        File::create(dir.path().join("document.txt")).unwrap();

        // Create subdirectory with more images
        fs::create_dir(dir.path().join("subdir")).unwrap();
        File::create(dir.path().join("subdir/photo3.jpeg")).unwrap();

        let extensions = vec!["jpg".to_string(), "jpeg".to_string(), "png".to_string()];
        let images = discover_images(&dir_path, &extensions).unwrap();

        assert_eq!(images.len(), 3);
    }
}
