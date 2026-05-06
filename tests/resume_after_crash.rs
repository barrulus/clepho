//! Crash-resume invariants. Spec §4.8: a stage that was interrupted between
//! `process_one` returning Ok and `mark_done` writing must look exactly like
//! "not yet processed" on next start, so the scheduler picks it up again.

use clepho::db::{apply_v2_schema, pipeline_write_facet, FacetTable, FixedClock};
use clepho::pipeline::stages::{exif::ExifStage, mark_done, mark_error, Stage, StageId};
use rusqlite::Connection;

#[test]
fn interrupted_stage_leaves_done_at_null_so_retry_picks_it_up() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute(
        "INSERT INTO photos(path, scan_done_at) VALUES ('/p/a.jpg', '2026-01-01')",
        [],
    )
    .unwrap();

    // Simulate a crash mid-stage: nothing called mark_done.
    let pending = ExifStage.pending(&c, None, 10).unwrap();
    assert_eq!(
        pending.len(),
        1,
        "photo is still pending after a simulated crash"
    );
}

#[test]
fn double_run_of_same_stage_does_not_duplicate_facet_rows() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");
    c.execute("INSERT INTO photos(path) VALUES ('/p/a.jpg')", [])
        .unwrap();
    c.execute("INSERT INTO objects(name) VALUES ('sunset')", [])
        .unwrap();

    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();

    let n: i64 = c
        .query_row("SELECT COUNT(*) FROM photo_objects", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn mark_done_after_prior_error_clears_error_field() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute(
        "INSERT INTO photos(path, scan_done_at) VALUES ('/p/a.jpg', '2026-01-01')",
        [],
    )
    .unwrap();
    let id: i64 = c
        .query_row("SELECT id FROM photos LIMIT 1", [], |r| r.get(0))
        .unwrap();

    // Simulate previous run that errored
    mark_error(&c, StageId::Exif, id, "transient failure").unwrap();
    let err: Option<String> = c
        .query_row("SELECT exif_error FROM photos WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(err.as_deref(), Some("transient failure"));

    // Retry succeeds
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");
    mark_done(&c, StageId::Exif, id, &clk).unwrap();

    let (done, err): (Option<String>, Option<String>) = c
        .query_row(
            "SELECT exif_done_at, exif_error FROM photos WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(done.is_some(), "exif_done_at should be set");
    assert!(err.is_none(), "stale error should be cleared on success");
}

#[test]
fn user_data_survives_unfinished_stage_state() {
    use clepho::db::provenance::user_add_facet;
    // A user wrote a tag while the LLM stage was mid-process. The crash
    // happened before mark_done. The user's tag must still be there on
    // retry, with source='user' intact.
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");
    c.execute(
        "INSERT INTO photos(path, scan_done_at, exif_done_at)
         VALUES ('/p/a.jpg', '2026-01-01', '2026-01-01')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO objects(name) VALUES ('sunset')", [])
        .unwrap();

    user_add_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk).unwrap();

    // Pretend LLM crashed mid-way: it had partially written an AI suggestion
    // for the same object. The provenance contract should leave the user row
    // alone (SkippedExisting).
    let outcome =
        pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert_eq!(
        outcome,
        clepho::db::provenance::WriteOutcome::SkippedExisting,
        "pipeline must not overwrite user-added rows on crash retry"
    );

    let src: String = c
        .query_row(
            "SELECT source FROM photo_objects WHERE photo_id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(src, "user");
}
