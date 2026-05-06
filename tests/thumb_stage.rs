use clepho::db::{apply_v2_schema, SystemClock};
use clepho::pipeline::stages::{thumb::ThumbStage, Stage, StageOutcome};
use rusqlite::Connection;
use tempfile::tempdir;

fn make_jpeg(path: &std::path::Path, w: u32, h: u32) {
    let img: image::ImageBuffer<image::Rgb<u8>, _> =
        image::ImageBuffer::from_fn(w, h, |x, y| image::Rgb([x as u8, y as u8, 0u8]));
    img.save_with_format(path, image::ImageFormat::Jpeg)
        .unwrap();
}

#[test]
fn thumb_writes_cache_file_at_sharded_path() {
    let src_dir = tempdir().unwrap();
    let cache_dir = tempdir().unwrap();
    let src = src_dir.path().join("photo.jpg");
    make_jpeg(&src, 64, 64);

    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute(
        "INSERT INTO photos(path, file_hash, scan_done_at, exif_done_at)
         VALUES (?1, 'abcdef0123456789', '2026-05-04T00:00:00Z', '2026-05-04T00:00:00Z')",
        rusqlite::params![src.to_str()],
    )
    .unwrap();

    let stage = ThumbStage::new(cache_dir.path().to_path_buf(), 256);
    let pending = stage.pending(&c, None, 10).unwrap();
    assert_eq!(pending.len(), 1);
    let outcome = stage.process_one(&c, &pending[0], &SystemClock).unwrap();
    assert!(matches!(outcome, StageOutcome::Ok));

    let expected = cache_dir
        .path()
        .join("ab")
        .join("cd")
        .join("abcdef0123456789.png");
    assert!(expected.exists(), "thumbnail expected at {:?}", expected);
}

#[test]
fn thumb_max_edge_caps_dimensions() {
    let src_dir = tempdir().unwrap();
    let cache_dir = tempdir().unwrap();
    let src = src_dir.path().join("big.jpg");
    make_jpeg(&src, 1024, 768);

    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute(
        "INSERT INTO photos(path, file_hash, scan_done_at, exif_done_at)
         VALUES (?1, 'fedcba9876543210', '2026-05-04T00:00:00Z', '2026-05-04T00:00:00Z')",
        rusqlite::params![src.to_str()],
    )
    .unwrap();

    let stage = ThumbStage::new(cache_dir.path().to_path_buf(), 128);
    let pending = stage.pending(&c, None, 10).unwrap();
    stage.process_one(&c, &pending[0], &SystemClock).unwrap();

    let out = cache_dir
        .path()
        .join("fe")
        .join("dc")
        .join("fedcba9876543210.png");
    let img = image::open(&out).unwrap();
    assert!(img.width() <= 128 && img.height() <= 128);
    // Aspect roughly preserved
    let aspect = img.width() as f64 / img.height() as f64;
    assert!((aspect - 1024.0 / 768.0).abs() < 0.05);
}

#[test]
fn thumb_without_scan_yields_missing_hash() {
    let src_dir = tempdir().unwrap();
    let cache_dir = tempdir().unwrap();
    let src = src_dir.path().join("photo.jpg");
    make_jpeg(&src, 16, 16);

    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    // exif_done_at set so it's pending, but file_hash NULL
    c.execute(
        "INSERT INTO photos(path, scan_done_at, exif_done_at)
         VALUES (?1, '2026-05-04T00:00:00Z', '2026-05-04T00:00:00Z')",
        rusqlite::params![src.to_str()],
    )
    .unwrap();

    let stage = ThumbStage::new(cache_dir.path().to_path_buf(), 64);
    let pending = stage.pending(&c, None, 10).unwrap();
    let outcome = stage.process_one(&c, &pending[0], &SystemClock).unwrap();
    match outcome {
        StageOutcome::Err { error_class, .. } => assert_eq!(error_class, "missing_hash"),
        other => panic!("expected missing_hash, got {:?}", other),
    }
}
