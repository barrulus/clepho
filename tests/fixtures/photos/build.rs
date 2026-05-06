//! One-shot fixture builder. Run with:
//!
//!     cargo run --example build_photo_fixtures
//!
//! Generates tiny synthetic JPEGs and a manifest. Outputs are committed to
//! the repo; this binary exists for regeneration, not for CI.
//!
//! EXIF-bearing fixtures are deliberately deferred — the `image` crate
//! doesn't write EXIF, and pulling in `little_exif` just for fixture builds
//! is more weight than it's worth right now. Tests that need real EXIF data
//! are marked accordingly until the fixtures gain EXIF tags.

fn main() {
    use image::{ImageBuffer, Rgb};

    let dir = std::path::Path::new("tests/fixtures/photos");
    std::fs::create_dir_all(dir).expect("create fixture dir");

    let solid = |r: u8, g: u8, b: u8| -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        ImageBuffer::from_fn(32, 32, |_, _| Rgb([r, g, b]))
    };

    // Filenames hint at intended use; the JPEGs themselves are interchangeable.
    let entries = [
        ("rome_2024_06_12.jpg", (255u8, 100u8, 0u8)),
        ("florence_2024_06_15.jpg", (50, 150, 200)),
        ("no_exif.jpg", (100, 100, 100)),
        ("no_gps.jpg", (200, 200, 200)),
    ];

    for (name, (r, g, b)) in entries {
        let path = dir.join(name);
        solid(r, g, b)
            .save_with_format(&path, image::ImageFormat::Jpeg)
            .unwrap_or_else(|e| panic!("write {:?}: {}", path, e));
        println!("wrote {}", path.display());
    }

    let manifest = serde_json::json!({
        "note": "synthesised JPEGs without EXIF. Tests asserting real EXIF \
                 presence should stay marked #[ignore] until little_exif (or \
                 similar) is wired into this generator.",
        "photos": entries.iter().map(|(file, _)| {
            serde_json::json!({"file": file, "has_exif": false})
        }).collect::<Vec<_>>(),
    });
    let manifest_path = dir.join("manifest.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    println!("wrote {}", manifest_path.display());
}
