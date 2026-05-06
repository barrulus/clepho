use clepho::db::{apply_v2_schema, FixedClock};
use clepho::pipeline::stages::{index::IndexStage, Stage};
use rusqlite::Connection;

fn setup() -> (Connection, i64) {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute(
        "INSERT INTO photos(path, scan_done_at, exif_done_at, thumb_done_at, llm_done_at, taken_at)
         VALUES ('p.jpg', '2026-05-04T00:00:00Z', '2026-05-04T00:00:00Z',
                 '2026-05-04T00:00:00Z', '2026-05-04T00:00:00Z', '2024-06-12T00:00:00Z')",
        [],
    )
    .unwrap();
    let id = c.last_insert_rowid();
    c.execute("INSERT INTO objects(name) VALUES ('sunset')", [])
        .unwrap();
    c.execute(
        "INSERT INTO photo_objects(photo_id, object_id, source) VALUES (?1, 1, 'ai')",
        rusqlite::params![id],
    )
    .unwrap();
    (c, id)
}

#[test]
fn index_adds_to_matching_smart_album() {
    let (c, id) = setup();
    let filter = r#"{"combinator":"and","clauses":[
        {"facet":"objects","op":"any_of","values":["sunset"]}
    ]}"#;
    c.execute(
        "INSERT INTO albums(name, kind, filter_json) VALUES ('Sunsets','smart',?1)",
        rusqlite::params![filter],
    )
    .unwrap();
    let album_id = c.last_insert_rowid();

    let stage = IndexStage;
    let pending = stage.pending(&c, None, 10).unwrap();
    assert_eq!(pending.len(), 1);
    stage
        .process_one(&c, &pending[0], &FixedClock::iso("2026-05-04T12:00:00Z"))
        .unwrap();

    let cnt: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM album_photos WHERE album_id=?1 AND photo_id=?2",
            rusqlite::params![album_id, id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cnt, 1);
}

#[test]
fn index_removes_from_non_matching_smart_album() {
    let (c, id) = setup();
    let filter = r#"{"combinator":"and","clauses":[
        {"facet":"objects","op":"any_of","values":["snow"]}
    ]}"#;
    c.execute(
        "INSERT INTO albums(name, kind, filter_json) VALUES ('Snowy','smart',?1)",
        rusqlite::params![filter],
    )
    .unwrap();
    let album_id = c.last_insert_rowid();
    c.execute(
        "INSERT INTO album_photos(album_id, photo_id) VALUES (?1, ?2)",
        rusqlite::params![album_id, id],
    )
    .unwrap();

    let stage = IndexStage;
    let pending = stage.pending(&c, None, 10).unwrap();
    stage
        .process_one(&c, &pending[0], &FixedClock::iso("2026-05-04T12:00:00Z"))
        .unwrap();

    let cnt: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM album_photos WHERE album_id=?1 AND photo_id=?2",
            rusqlite::params![album_id, id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cnt, 0);
}

#[test]
fn index_pending_requires_thumb_and_llm_done() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    // photo with exif done but neither thumb nor llm
    c.execute(
        "INSERT INTO photos(path, exif_done_at) VALUES ('a.jpg','2026-05-04T00:00:00Z')",
        [],
    )
    .unwrap();
    // photo with everything done — should be pending
    c.execute(
        "INSERT INTO photos(path, exif_done_at, thumb_done_at, llm_done_at)
         VALUES ('b.jpg','x','x','x')",
        [],
    )
    .unwrap();

    let pending = IndexStage.pending(&c, None, 10).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].path, std::path::PathBuf::from("b.jpg"));
}

#[test]
fn index_skips_albums_with_invalid_filter_json() {
    let (c, _id) = setup();
    c.execute(
        "INSERT INTO albums(name, kind, filter_json) VALUES ('Broken','smart','{not valid')",
        [],
    )
    .unwrap();
    // Also a valid one, so we know the loop continues
    let good = r#"{"combinator":"and","clauses":[
        {"facet":"objects","op":"any_of","values":["sunset"]}
    ]}"#;
    c.execute(
        "INSERT INTO albums(name, kind, filter_json) VALUES ('Good','smart',?1)",
        rusqlite::params![good],
    )
    .unwrap();

    let stage = IndexStage;
    let pending = stage.pending(&c, None, 10).unwrap();
    // Should not panic or error on the broken filter; should still index Good.
    stage
        .process_one(&c, &pending[0], &FixedClock::iso("2026-05-04T12:00:00Z"))
        .unwrap();

    let good_album: i64 = c
        .query_row("SELECT id FROM albums WHERE name='Good'", [], |r| r.get(0))
        .unwrap();
    let cnt: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM album_photos WHERE album_id=?1",
            rusqlite::params![good_album],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cnt, 1);
}
