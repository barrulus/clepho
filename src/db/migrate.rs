//! Schema generation detection. clepho v2 (this plan) uses generation 2.
//! Anything below = legacy schema; user must explicitly opt-in to reset (Task 19).

use anyhow::Result;
use rusqlite::Connection;

#[allow(dead_code)]
pub const CURRENT_GENERATION: i64 = 2;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaState {
    Empty,           // brand-new DB, no tables yet
    Legacy,          // has tables but no 'schema_version' or version < 2
    Current,         // version = CURRENT_GENERATION
    Newer(i64),      // version > CURRENT_GENERATION (downgrade case)
}

#[allow(dead_code)]
pub fn detect_schema_state(conn: &Connection) -> Result<SchemaState> {
    let any_table: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%')",
        [],
        |r| r.get(0),
    )?;
    if !any_table {
        return Ok(SchemaState::Empty);
    }

    let has_version: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_version')",
        [],
        |r| r.get(0),
    )?;
    if !has_version {
        return Ok(SchemaState::Legacy);
    }

    let version: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .unwrap_or(0);

    Ok(match version.cmp(&CURRENT_GENERATION) {
        std::cmp::Ordering::Less => SchemaState::Legacy,
        std::cmp::Ordering::Equal => SchemaState::Current,
        std::cmp::Ordering::Greater => SchemaState::Newer(version),
    })
}

#[allow(dead_code)]
pub fn apply_v2_schema(conn: &Connection) -> Result<()> {
    use crate::db::schema_v2::SCHEMA_V2;
    conn.execute_batch(SCHEMA_V2)?;
    Ok(())
}

/// Drop ALL existing tables, then apply v2 schema. Destructive.
/// Caller must obtain user consent before invoking (see ResetDbDialog, Task 19).
#[allow(dead_code)]
pub fn reset_to_v2(conn: &Connection) -> Result<()> {
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;

    conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
    for t in &tables {
        conn.execute(&format!("DROP TABLE IF EXISTS \"{}\"", t), [])?;
    }
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    apply_v2_schema(conn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_mem() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn empty_db_detected_as_empty() {
        let c = open_mem();
        assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Empty);
    }

    #[test]
    fn legacy_db_detected_as_legacy() {
        let c = open_mem();
        c.execute_batch("CREATE TABLE photos (id INTEGER PRIMARY KEY);")
            .unwrap();
        assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Legacy);
    }

    #[test]
    fn v2_db_detected_as_current() {
        let c = open_mem();
        apply_v2_schema(&c).unwrap();
        assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Current);
    }

    #[test]
    fn newer_db_detected_as_newer() {
        let c = open_mem();
        apply_v2_schema(&c).unwrap();
        c.execute("INSERT INTO schema_version(version) VALUES (3)", [])
            .unwrap();
        assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Newer(3));
    }

    #[test]
    fn reset_drops_legacy_and_applies_v2() {
        let c = open_mem();
        c.execute_batch("CREATE TABLE photos (id INTEGER PRIMARY KEY, tags TEXT);")
            .unwrap();
        c.execute("INSERT INTO photos(tags) VALUES ('legacy')", [])
            .unwrap();

        reset_to_v2(&c).unwrap();
        assert_eq!(detect_schema_state(&c).unwrap(), SchemaState::Current);

        let cols: Vec<String> = c
            .prepare("PRAGMA table_info(photos)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(cols.contains(&"description_source".to_string()));
        assert!(!cols.contains(&"tags".to_string()));
    }
}
