use clepho::db::apply_v2_schema;
use clepho::pipeline::stages::{exif::ExifStage, Stage};
use rusqlite::Connection;

#[test]
fn already_done_photos_are_skipped() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute(
        "INSERT INTO photos(path, scan_done_at, exif_done_at) VALUES
         ('/p/a.jpg', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
         ('/p/b.jpg', '2026-01-01T00:00:00Z', NULL)",
        [],
    )
    .unwrap();

    let stage = ExifStage;
    let pending = stage.pending(&c, None, 100).unwrap();
    let paths: Vec<String> = pending
        .iter()
        .map(|p| p.path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(paths, vec!["/p/b.jpg"], "only photo b is pending exif");
}

#[test]
fn pending_respects_prereq() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute("INSERT INTO photos(path) VALUES ('/p/a.jpg')", [])
        .unwrap();
    let stage = ExifStage;
    let pending = stage.pending(&c, None, 100).unwrap();
    assert!(
        pending.is_empty(),
        "exif requires scan_done_at IS NOT NULL"
    );
}

#[test]
fn full_dag_chain_each_stage_gates_on_predecessor() {
    use clepho::pipeline::stages::{index::IndexStage, thumb::ThumbStage};
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    // Photo with everything but llm done
    c.execute(
        "INSERT INTO photos(path, scan_done_at, exif_done_at, thumb_done_at)
         VALUES ('/p/a.jpg', 'x', 'x', 'x')",
        [],
    )
    .unwrap();

    // Index requires thumb AND llm AND exif — llm missing, so not pending.
    let pending = IndexStage.pending(&c, None, 10).unwrap();
    assert!(
        pending.is_empty(),
        "index should not be pending when llm_done_at is NULL"
    );

    // Thumb is already done — also not pending.
    let stage = ThumbStage::new(std::path::PathBuf::from("/tmp"), 64);
    let pending = stage.pending(&c, None, 10).unwrap();
    assert!(pending.is_empty());
}

#[test]
fn folder_filter_only_matches_prefix() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute(
        "INSERT INTO photos(path, scan_done_at) VALUES
         ('/photos/a/x.jpg','x'),
         ('/photos/b/y.jpg','x'),
         ('/photos/a/sub/z.jpg','x')",
        [],
    )
    .unwrap();

    let stage = ExifStage;
    let pending = stage.pending(&c, Some("/photos/a"), 10).unwrap();
    let paths: Vec<String> = pending
        .iter()
        .map(|p| p.path.to_string_lossy().into_owned())
        .collect();
    assert!(paths.contains(&"/photos/a/x.jpg".to_string()));
    assert!(paths.contains(&"/photos/a/sub/z.jpg".to_string()));
    assert!(!paths.contains(&"/photos/b/y.jpg".to_string()));
}
