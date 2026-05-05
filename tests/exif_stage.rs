use clepho::db::{apply_v2_schema, SystemClock};
use clepho::pipeline::stages::{exif::ExifStage, Stage, StageOutcome};
use rusqlite::Connection;
use tempfile::TempDir;

fn synth_no_exif_jpeg() -> (TempDir, std::path::PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("plain.jpg");
    let img: image::ImageBuffer<image::Rgb<u8>, _> =
        image::ImageBuffer::from_fn(8, 8, |_, _| image::Rgb([0u8, 0, 0]));
    img.save(&p).unwrap();
    (d, p)
}

#[test]
fn exif_no_metadata_is_clean_negative() {
    let (_tmp, path) = synth_no_exif_jpeg();

    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute(
        "INSERT INTO photos(path, scan_done_at) VALUES (?1, '2026-05-04T00:00:00Z')",
        rusqlite::params![path.to_str()],
    )
    .unwrap();

    let stage = ExifStage;
    let pending = stage.pending(&c, None, 10).unwrap();
    assert_eq!(pending.len(), 1);

    let outcome = stage.process_one(&c, &pending[0], &SystemClock).unwrap();
    assert!(matches!(outcome, StageOutcome::Ok));

    let (taken, lat, lon, err): (
        Option<String>,
        Option<f64>,
        Option<f64>,
        Option<String>,
    ) = c
        .query_row(
            "SELECT taken_at, gps_lat, gps_lon, exif_error FROM photos WHERE id=?1",
            rusqlite::params![pending[0].id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert!(taken.is_none(), "no-EXIF jpeg should leave taken_at NULL");
    assert!(lat.is_none());
    assert!(lon.is_none());
    assert!(err.is_none(), "absent EXIF is not an error");
}

#[test]
fn exif_pending_requires_scan_done() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute(
        "INSERT INTO photos(path, scan_done_at) VALUES
         ('a.jpg','2026-01-01T00:00:00Z'),
         ('b.jpg', NULL)",
        [],
    )
    .unwrap();

    let stage = ExifStage;
    let pending = stage.pending(&c, None, 10).unwrap();
    assert_eq!(pending.len(), 1, "only scan-done photos should be pending");
    assert_eq!(pending[0].path, std::path::PathBuf::from("a.jpg"));
}

#[test]
fn exif_missing_file_classified_fs_missing() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute(
        "INSERT INTO photos(path, scan_done_at) VALUES
         ('/nonexistent/ghost.jpg','2026-05-04T00:00:00Z')",
        [],
    )
    .unwrap();

    let stage = ExifStage;
    let pending = stage.pending(&c, None, 10).unwrap();
    assert_eq!(pending.len(), 1);

    match stage.process_one(&c, &pending[0], &SystemClock).unwrap() {
        StageOutcome::Err {
            error_class,
            message: _,
        } => assert_eq!(error_class, "fs_missing"),
        other => panic!("expected fs_missing, got {:?}", other),
    }
}
