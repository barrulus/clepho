//! Smoke test: every fixture listed in the manifest exists on disk, decodes
//! as a valid image, and matches its has_exif flag. Catches drift between
//! the manifest and the committed jpegs.

use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/photos")
}

#[test]
fn fixture_manifest_matches_disk() {
    let dir = fixtures_dir();
    let manifest_path = dir.join("manifest.json");
    let manifest_str = std::fs::read_to_string(&manifest_path).unwrap_or_else(|_| {
        panic!(
            "missing {}; regenerate with `cargo run --example build_photo_fixtures`",
            manifest_path.display()
        )
    });
    let manifest: serde_json::Value = serde_json::from_str(&manifest_str).unwrap();

    for entry in manifest["photos"].as_array().unwrap() {
        let file = entry["file"].as_str().unwrap();
        let has_exif = entry["has_exif"].as_bool().unwrap();

        let path = dir.join(file);
        assert!(path.exists(), "missing fixture file: {}", path.display());

        // Decodes as an image
        image::open(&path).unwrap_or_else(|e| panic!("decode {}: {}", file, e));

        // EXIF presence matches manifest
        let f = std::fs::File::open(&path).unwrap();
        let mut br = std::io::BufReader::new(f);
        let actual_has_exif = exif::Reader::new().read_from_container(&mut br).is_ok();
        assert_eq!(
            actual_has_exif, has_exif,
            "fixture {} has_exif={} but manifest claims has_exif={}",
            file, actual_has_exif, has_exif
        );
    }
}
