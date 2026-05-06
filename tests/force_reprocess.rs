use clepho::db::{
    apply_v2_schema, pipeline_write_facet, user_add_facet, FacetTable, FixedClock,
};
use clepho::pipeline::reprocess::{apply_reset, count_confirmed_values, count_user_values};
use clepho::pipeline::stages::StageId;
use rusqlite::Connection;

#[test]
fn reset_clears_done_but_preserves_user_and_confirmed_values() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");

    // Photo 1: AI-described, not yet confirmed.
    c.execute(
        "INSERT INTO photos(path, scan_done_at, exif_done_at, llm_done_at, index_done_at,
                            description, description_source)
         VALUES ('/photos/p1.jpg', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z',
                 '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z',
                 'AI desc', 'ai')",
        [],
    )
    .unwrap();

    // Photo 2: user-edited description, must be preserved.
    c.execute(
        "INSERT INTO photos(path, scan_done_at, exif_done_at, llm_done_at, index_done_at,
                            description, description_source, description_confirmed_at)
         VALUES ('/photos/p2.jpg', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z',
                 '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z',
                 'User desc', 'user', '2026-01-02T00:00:00Z')",
        [],
    )
    .unwrap();

    c.execute("INSERT INTO objects(name) VALUES ('sunset')", [])
        .unwrap();
    // Photo 1: AI object, unconfirmed
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 1, 1, "object", "sunset", &clk).unwrap();
    // Photo 2: user object — must be preserved
    user_add_facet(&c, FacetTable::PhotoObjects, 2, 1, &clk).unwrap();

    apply_reset(&c, "/photos", &[StageId::Llm]).unwrap();

    // Both photos: llm_done_at and index_done_at cleared.
    let llm_done: Vec<Option<String>> = c
        .prepare("SELECT llm_done_at FROM photos ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(
        llm_done.iter().all(|x| x.is_none()),
        "all llm_done_at should be NULL after reset"
    );

    let index_done: Vec<Option<String>> = c
        .prepare("SELECT index_done_at FROM photos ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(
        index_done.iter().all(|x| x.is_none()),
        "index_done_at should also be cleared"
    );

    // Photo 2's user description: untouched.
    let (desc, src): (String, String) = c
        .query_row(
            "SELECT description, description_source FROM photos WHERE id=2",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(desc, "User desc");
    assert_eq!(src, "user");

    // Photo 2's user object row preserved with source='user' and confirmed_at set.
    let (osrc, conf): (String, Option<String>) = c
        .query_row(
            "SELECT source, confirmed_at FROM photo_objects WHERE photo_id=2 AND object_id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(osrc, "user");
    assert!(conf.is_some());

    // Photo 1's AI-unconfirmed object row still exists; the LLM stage
    // re-running will leave it via provenance::pipeline_write_facet's
    // Idempotent path.
    let count: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM photo_objects WHERE photo_id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn reset_index_only_does_not_clear_other_stages() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute(
        "INSERT INTO photos(path, scan_done_at, exif_done_at, llm_done_at, index_done_at)
         VALUES ('/photos/p.jpg', 'x', 'x', 'x', 'x')",
        [],
    )
    .unwrap();

    apply_reset(&c, "/photos", &[StageId::Index]).unwrap();

    let (scan, exif, llm, index): (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = c
        .query_row(
            "SELECT scan_done_at, exif_done_at, llm_done_at, index_done_at FROM photos",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert!(scan.is_some());
    assert!(exif.is_some());
    assert!(llm.is_some());
    assert!(index.is_none());
}

#[test]
fn preflight_counts_match_reality() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    let clk = FixedClock::iso("2026-05-04T12:00:00Z");

    c.execute(
        "INSERT INTO photos(path, description, description_source)
         VALUES ('/p/a.jpg', 'user wrote this', 'user'),
                ('/p/b.jpg', 'ai desc', 'ai')",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO objects(name) VALUES ('sunset')", [])
        .unwrap();

    // a: user object
    user_add_facet(&c, FacetTable::PhotoObjects, 1, 1, &clk).unwrap();
    // b: ai object, then confirm it
    pipeline_write_facet(&c, FacetTable::PhotoObjects, 2, 1, "object", "sunset", &clk).unwrap();
    c.execute(
        "UPDATE photo_objects SET confirmed_at=?1 WHERE photo_id=2",
        rusqlite::params!["2026-01-02T00:00:00Z"],
    )
    .unwrap();

    // user-edited values: 1 (a's user description) + 1 (a's user object) = 2
    assert_eq!(count_user_values(&c, "/p").unwrap(), 2);
    // confirmed AI values: 1 (b's confirmed object)
    assert_eq!(count_confirmed_values(&c, "/p").unwrap(), 1);
}
