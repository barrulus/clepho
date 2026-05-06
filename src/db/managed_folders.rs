//! CRUD for managed_folders + folder_prompts (spec §3.6).

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedFolder {
    pub id: i64,
    pub path: String,
    pub schedule_cron: Option<String>,
    pub paused: bool,
    pub last_run_at: Option<String>,
    pub faces_disabled: bool,
}

#[allow(dead_code)]
pub fn list(conn: &Connection) -> Result<Vec<ManagedFolder>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, schedule_cron, paused, last_run_at, faces_disabled
         FROM managed_folders ORDER BY path",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ManagedFolder {
            id: r.get(0)?,
            path: r.get(1)?,
            schedule_cron: r.get(2)?,
            paused: r.get::<_, i64>(3)? != 0,
            last_run_at: r.get(4)?,
            faces_disabled: r.get::<_, i64>(5)? != 0,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[allow(dead_code)]
pub fn add(conn: &Connection, path: &str, schedule_cron: Option<&str>) -> Result<i64> {
    conn.execute(
        "INSERT OR IGNORE INTO managed_folders(path, schedule_cron) VALUES (?1, ?2)",
        params![path, schedule_cron],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM managed_folders WHERE path=?1",
        params![path],
        |r| r.get(0),
    )?;
    Ok(id)
}

#[allow(dead_code)]
pub fn remove(conn: &Connection, path: &str) -> Result<()> {
    conn.execute("DELETE FROM managed_folders WHERE path=?1", params![path])?;
    Ok(())
}

#[allow(dead_code)]
pub fn set_paused(conn: &Connection, path: &str, paused: bool) -> Result<()> {
    conn.execute(
        "UPDATE managed_folders SET paused=?1, updated_at=CURRENT_TIMESTAMP WHERE path=?2",
        params![if paused { 1 } else { 0 }, path],
    )?;
    Ok(())
}

#[allow(dead_code)]
pub fn set_last_run(conn: &Connection, path: &str, ts: &str) -> Result<()> {
    conn.execute(
        "UPDATE managed_folders SET last_run_at=?1, updated_at=CURRENT_TIMESTAMP WHERE path=?2",
        params![ts, path],
    )?;
    Ok(())
}

#[allow(dead_code)]
pub fn is_managed(conn: &Connection, path: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM managed_folders WHERE path=?1)",
        params![path],
        |r| r.get(0),
    )?)
}

// folder_prompts (works for managed AND ad-hoc folders)

#[allow(dead_code)]
pub fn get_prompt(conn: &Connection, path: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT custom_prompt FROM folder_prompts WHERE path=?1",
            params![path],
            |r| r.get(0),
        )
        .optional()?)
}

#[allow(dead_code)]
pub fn set_prompt(conn: &Connection, path: &str, prompt: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO folder_prompts(path, custom_prompt) VALUES (?1, ?2)
         ON CONFLICT(path) DO UPDATE SET custom_prompt=?2, updated_at=CURRENT_TIMESTAMP",
        params![path, prompt],
    )?;
    Ok(())
}

#[allow(dead_code)]
pub fn delete_prompt(conn: &Connection, path: &str) -> Result<()> {
    conn.execute("DELETE FROM folder_prompts WHERE path=?1", params![path])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::apply_v2_schema;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        apply_v2_schema(&c).unwrap();
        c
    }

    #[test]
    fn add_then_list() {
        let c = db();
        add(&c, "/photos/2024", Some("0 5 * * *")).unwrap();
        add(&c, "/photos/2023", None).unwrap();
        let xs = list(&c).unwrap();
        assert_eq!(xs.len(), 2);
        assert_eq!(xs[0].path, "/photos/2023");
        assert!(xs[1].schedule_cron.is_some());
    }

    #[test]
    fn add_is_idempotent_on_path() {
        let c = db();
        let id1 = add(&c, "/photos", None).unwrap();
        let id2 = add(&c, "/photos", None).unwrap();
        assert_eq!(id1, id2, "second add should return same id");
        assert_eq!(list(&c).unwrap().len(), 1);
    }

    #[test]
    fn remove_drops_row() {
        let c = db();
        add(&c, "/photos", None).unwrap();
        remove(&c, "/photos").unwrap();
        assert_eq!(list(&c).unwrap().len(), 0);
    }

    #[test]
    fn pause_and_unpause() {
        let c = db();
        add(&c, "/p", None).unwrap();
        set_paused(&c, "/p", true).unwrap();
        let xs = list(&c).unwrap();
        assert!(xs[0].paused);
        set_paused(&c, "/p", false).unwrap();
        assert!(!list(&c).unwrap()[0].paused);
    }

    #[test]
    fn set_last_run_updates_timestamp() {
        let c = db();
        add(&c, "/p", None).unwrap();
        set_last_run(&c, "/p", "2026-05-04T12:00:00Z").unwrap();
        let xs = list(&c).unwrap();
        assert_eq!(xs[0].last_run_at.as_deref(), Some("2026-05-04T12:00:00Z"));
    }

    #[test]
    fn is_managed_reports_correctly() {
        let c = db();
        assert!(!is_managed(&c, "/p").unwrap());
        add(&c, "/p", None).unwrap();
        assert!(is_managed(&c, "/p").unwrap());
    }

    #[test]
    fn folder_prompt_set_get_delete() {
        let c = db();
        assert!(get_prompt(&c, "/p").unwrap().is_none());
        set_prompt(&c, "/p", "wedding photos").unwrap();
        assert_eq!(
            get_prompt(&c, "/p").unwrap().as_deref(),
            Some("wedding photos")
        );
        set_prompt(&c, "/p", "updated").unwrap();
        assert_eq!(get_prompt(&c, "/p").unwrap().as_deref(), Some("updated"));
        delete_prompt(&c, "/p").unwrap();
        assert!(get_prompt(&c, "/p").unwrap().is_none());
    }

    #[test]
    fn folder_prompt_works_without_managed_folder_row() {
        // Spec: folder_prompts works for both managed AND ad-hoc folders.
        let c = db();
        set_prompt(&c, "/adhoc", "ad-hoc context").unwrap();
        assert!(!is_managed(&c, "/adhoc").unwrap());
        assert_eq!(
            get_prompt(&c, "/adhoc").unwrap().as_deref(),
            Some("ad-hoc context")
        );
    }
}
