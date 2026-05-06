use clepho::db::{apply_v2_schema, detect_schema_state, reset_to_v2, SchemaState};
use rusqlite::Connection;

#[test]
fn legacy_schema_triggers_reset_path() {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch(
        "CREATE TABLE photos (id INTEGER PRIMARY KEY, tags TEXT);
         INSERT INTO photos(tags) VALUES ('legacy');",
    )
    .unwrap();
    assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Legacy);

    reset_to_v2(&c).unwrap();
    assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Current);

    // Legacy 'tags' column gone, v2 'description_source' present
    let cols: Vec<String> = c
        .prepare("PRAGMA table_info(photos)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(!cols.contains(&"tags".to_string()));
    assert!(cols.contains(&"description_source".to_string()));
}

#[test]
fn empty_db_apply_v2_then_current() {
    let c = Connection::open_in_memory().unwrap();
    assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Empty);
    apply_v2_schema(&c).unwrap();
    assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Current);
}

#[test]
fn newer_schema_remains_newer_after_apply() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    c.execute("INSERT INTO schema_version(version) VALUES (3)", [])
        .unwrap();
    assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Newer(3));
}
