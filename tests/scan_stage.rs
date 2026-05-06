use clepho::db::{apply_v2_schema, SystemClock};
use clepho::pipeline::stages::{mark_done, scan::ScanStage, Stage, StageId, StageOutcome};
use rusqlite::Connection;
use tempfile::tempdir;

fn write_jpeg(path: &std::path::Path) {
    use image::{ImageBuffer, Rgb};
    let img: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_fn(8, 8, |_, _| Rgb([255u8, 0, 0]));
    img.save_with_format(path, image::ImageFormat::Jpeg)
        .unwrap();
}

#[test]
fn scan_inserts_and_processes() {
    let d = tempdir().unwrap();
    write_jpeg(&d.path().join("a.jpg"));
    write_jpeg(&d.path().join("b.jpg"));
    std::fs::write(d.path().join("readme.txt"), "ignored").unwrap();

    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();

    let stage = ScanStage::new(vec![d.path().to_path_buf()], vec!["jpg".into()]);
    let n = stage.discover(&c, d.path()).unwrap();
    assert_eq!(n, 2, "expected 2 jpg discoveries, txt ignored");

    let pending = stage.pending(&c, None, 10).unwrap();
    assert_eq!(pending.len(), 2);

    let clock = SystemClock;
    for p in pending {
        let out = stage.process_one(&c, &p, &clock).unwrap();
        assert!(matches!(out, StageOutcome::Ok));
        mark_done(&c, StageId::Scan, p.id, &clock).unwrap();
    }

    let after: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM photos WHERE file_hash IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after, 2);

    // Width/height/mime populated
    let (w, h, mime): (i64, i64, String) = c
        .query_row(
            "SELECT width, height, mime FROM photos WHERE path LIKE '%a.jpg' LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(w, 8);
    assert_eq!(h, 8);
    assert_eq!(mime, "image/jpeg");
}

#[test]
fn scan_is_idempotent_via_or_ignore() {
    let d = tempdir().unwrap();
    write_jpeg(&d.path().join("a.jpg"));
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();

    let stage = ScanStage::new(vec![d.path().to_path_buf()], vec!["jpg".into()]);
    stage.discover(&c, d.path()).unwrap();
    stage.discover(&c, d.path()).unwrap();
    let count: i64 = c
        .query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn missing_file_yields_fs_missing_error() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute(
        "INSERT INTO photos(path) VALUES ('/nonexistent/ghost.jpg')",
        [],
    )
    .unwrap();
    let stage = ScanStage::new(vec![], vec!["jpg".into()]);
    let pending = stage.pending(&c, None, 10).unwrap();
    assert_eq!(pending.len(), 1);
    let out = stage.process_one(&c, &pending[0], &SystemClock).unwrap();
    match out {
        StageOutcome::Err {
            error_class,
            message: _,
        } => assert_eq!(error_class, "fs_missing"),
        _ => panic!("expected fs_missing error, got {:?}", out),
    }
}
