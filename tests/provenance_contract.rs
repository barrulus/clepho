//! Spec §6.2 enforcement — every operation × every starting state → expected outcome.
//! If any cell of this matrix is wrong, the whole alignment effort breaks.

use clepho::db::{
    apply_v2_schema, pipeline_write_facet, user_add_facet, user_confirm_facet, user_reject_facet,
    user_remove_facet, FacetTable, FixedClock, WriteOutcome,
};
use rusqlite::Connection;

fn setup() -> (Connection, FixedClock) {
    let conn = Connection::open_in_memory().unwrap();
    apply_v2_schema(&conn).unwrap();
    conn.execute("INSERT INTO photos(path) VALUES ('p1.jpg')", [])
        .unwrap();
    conn.execute("INSERT INTO objects(name) VALUES ('sunset')", [])
        .unwrap();
    let clock = FixedClock::iso("2026-05-04T12:00:00Z");
    (conn, clock)
}

fn row_state(conn: &Connection) -> Option<(String, Option<String>)> {
    conn.query_row(
        "SELECT source, confirmed_at FROM photo_objects WHERE photo_id=1 AND object_id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .ok()
}

// --- Pipeline writes -------------------------------------------------------

#[test]
fn pipeline_write_to_empty_inserts_ai_unconfirmed() {
    let (c, clk) = setup();
    let outcome =
        pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::Inserted);
    assert_eq!(row_state(&c), Some(("ai".into(), None)));
}

#[test]
fn pipeline_write_to_existing_ai_unconfirmed_is_idempotent() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    let outcome =
        pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::Idempotent);
    assert_eq!(row_state(&c), Some(("ai".into(), None)));
}

#[test]
fn pipeline_write_skipped_when_existing_user_row() {
    let (c, clk) = setup();
    user_add_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk).unwrap();
    let outcome =
        pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::SkippedExisting);
    let (src, conf) = row_state(&c).unwrap();
    assert_eq!(src, "user");
    assert!(conf.is_some());
}

#[test]
fn pipeline_write_skipped_when_existing_ai_confirmed() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    user_confirm_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk).unwrap();
    let outcome =
        pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::SkippedExisting);
}

#[test]
fn pipeline_write_skipped_when_value_rejected() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    user_reject_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert!(row_state(&c).is_none());

    let outcome =
        pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::SkippedRejected);
    assert!(row_state(&c).is_none());
}

// --- User add / confirm / remove / reject ---------------------------------

#[test]
fn user_add_to_empty_inserts_user_confirmed() {
    let (c, clk) = setup();
    let outcome = user_add_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::Inserted);
    let (src, conf) = row_state(&c).unwrap();
    assert_eq!(src, "user");
    assert_eq!(conf.unwrap(), "2026-05-04T12:00:00+00:00");
}

#[test]
fn user_add_existing_ai_upgrades_to_user_confirmed() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    let outcome = user_add_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::UpdatedConfirmed);
    let (src, conf) = row_state(&c).unwrap();
    assert_eq!(src, "user");
    assert!(conf.is_some());
}

#[test]
fn user_confirm_promotes_ai_to_confirmed() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    let outcome = user_confirm_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::UpdatedConfirmed);
    let (src, conf) = row_state(&c).unwrap();
    assert_eq!(src, "ai");
    assert!(conf.is_some());
}

#[test]
fn user_confirm_fails_on_missing_row() {
    let (c, clk) = setup();
    let r = user_confirm_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk);
    assert!(r.is_err(), "expected error for confirm of missing row");
}

#[test]
fn user_remove_deletes_row() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    user_remove_facet(&c, FacetTable::PhotoObjects, 1, 1).unwrap();
    assert!(row_state(&c).is_none());
}

#[test]
fn user_remove_does_not_record_rejection() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    user_remove_facet(&c, FacetTable::PhotoObjects, 1, 1).unwrap();

    let rejected_count: i64 = c
        .query_row("SELECT COUNT(*) FROM rejected_suggestions", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(rejected_count, 0);

    let outcome =
        pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert_eq!(outcome, WriteOutcome::Inserted);
}

#[test]
fn user_reject_deletes_and_records_rejection() {
    let (c, clk) = setup();
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    user_reject_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    assert!(row_state(&c).is_none());

    let cnt: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM rejected_suggestions WHERE photo_id=1 AND facet='object' AND value='sunset'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cnt, 1);
}

#[test]
fn user_add_after_reject_clears_rejection() {
    // Spec §6.4: "If the user later adds the rejected value back manually,
    // the rejected_suggestions row for that combo is deleted."
    let (c, clk) = setup();
    user_reject_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    user_add_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk).unwrap();

    let cnt: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM rejected_suggestions WHERE photo_id=1 AND facet='object'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        cnt, 0,
        "user re-adding a rejected value must clear the rejection (spec §6.4)"
    );
}
