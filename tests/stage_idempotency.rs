use clepho::db::{apply_v2_schema, pipeline_write_facet, FacetTable, FixedClock};
use rusqlite::Connection;

#[test]
fn pipeline_writing_same_object_twice_is_idempotent() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");
    c.execute("INSERT INTO photos(path) VALUES ('/p/a.jpg')", [])
        .unwrap();
    c.execute("INSERT INTO objects(name) VALUES ('sunset')", [])
        .unwrap();

    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();

    let count: i64 = c
        .query_row("SELECT COUNT(*) FROM photo_objects", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1, "no duplicate rows from idempotent writes");
}

#[test]
fn rerunning_full_pipeline_on_done_photo_is_no_op() {
    use clepho::pipeline::stages::{exif::ExifStage, Stage};
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute(
        "INSERT INTO photos(path, scan_done_at, exif_done_at, taken_at)
         VALUES ('/p/a.jpg', '2026-01-01', '2026-01-01', '2024-06-12T00:00:00Z')",
        [],
    )
    .unwrap();

    let pending = ExifStage.pending(&c, None, 10).unwrap();
    assert!(pending.is_empty());

    let taken: Option<String> = c
        .query_row("SELECT taken_at FROM photos WHERE id=1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(taken.as_deref(), Some("2024-06-12T00:00:00Z"));
}

#[test]
fn idempotent_outcome_is_signalled_explicitly() {
    use clepho::db::provenance::WriteOutcome;
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");
    c.execute("INSERT INTO photos(path) VALUES ('/p/a.jpg')", [])
        .unwrap();
    c.execute("INSERT INTO objects(name) VALUES ('sunset')", [])
        .unwrap();

    let first = pipeline_write_facet(
        &c,
        FacetTable::PhotoObjects,
        1,
        1,
        "object",
        "sunset",
        &clk,
    )
    .unwrap();
    assert_eq!(first, WriteOutcome::Inserted);

    let second = pipeline_write_facet(
        &c,
        FacetTable::PhotoObjects,
        1,
        1,
        "object",
        "sunset",
        &clk,
    )
    .unwrap();
    assert_eq!(
        second,
        WriteOutcome::Idempotent,
        "second pipeline write must be Idempotent, not Inserted"
    );
}
