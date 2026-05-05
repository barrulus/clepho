//! pipeline_events: DB-backed observability log (spec §8.2).
//! Companion of the JSONL log file; capped at 10k rows with auto-rotation.

use crate::db::clock::Clock;
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventLevel {
    Info,
    Warn,
    Error,
}

impl EventLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PipelineEvent {
    pub level: EventLevel,
    pub stage: Option<String>,
    pub folder: Option<String>,
    pub photo_id: Option<i64>,
    pub error_class: Option<String>,
    pub message: String,
    pub context_json: Option<String>,
}

/// Append an event. Rotates oldest rows when count exceeds 10_000.
#[allow(dead_code)]
pub fn append(conn: &Connection, ev: &PipelineEvent, clock: &dyn Clock) -> Result<i64> {
    let now = clock.now().to_rfc3339();
    conn.execute(
        "INSERT INTO pipeline_events
         (occurred_at, level, stage, folder, photo_id, error_class, message, context_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            now,
            ev.level.as_str(),
            ev.stage,
            ev.folder,
            ev.photo_id,
            ev.error_class,
            ev.message,
            ev.context_json
        ],
    )?;
    let id = conn.last_insert_rowid();
    rotate_if_needed(conn)?;
    Ok(id)
}

fn rotate_if_needed(conn: &Connection) -> Result<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM pipeline_events", [], |r| r.get(0))?;
    if count > 10_000 {
        let to_delete = count - 10_000;
        conn.execute(
            "DELETE FROM pipeline_events WHERE id IN
             (SELECT id FROM pipeline_events ORDER BY occurred_at ASC, id ASC LIMIT ?1)",
            params![to_delete],
        )?;
    }
    Ok(())
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct UnresolvedGroup {
    pub stage: Option<String>,
    pub error_class: Option<String>,
    pub count: i64,
    pub example_message: String,
}

/// Group unresolved errors by (stage, error_class) for the Failures Inbox.
#[allow(dead_code)]
pub fn unresolved_groups(conn: &Connection) -> Result<Vec<UnresolvedGroup>> {
    let mut stmt = conn.prepare(
        "SELECT stage, error_class, COUNT(*) as cnt, MIN(message) as msg
         FROM pipeline_events
         WHERE level='error' AND resolved_at IS NULL
         GROUP BY stage, error_class
         ORDER BY cnt DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(UnresolvedGroup {
            stage: r.get(0)?,
            error_class: r.get(1)?,
            count: r.get(2)?,
            example_message: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[allow(dead_code)]
pub fn resolve_group(
    conn: &Connection,
    stage: Option<&str>,
    error_class: Option<&str>,
    clock: &dyn Clock,
) -> Result<usize> {
    let now = clock.now().to_rfc3339();
    let n = conn.execute(
        "UPDATE pipeline_events SET resolved_at = ?1
         WHERE level='error' AND resolved_at IS NULL
           AND (stage IS ?2 OR stage = ?2)
           AND (error_class IS ?3 OR error_class = ?3)",
        params![now, stage, error_class],
    )?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{apply_v2_schema, FixedClock};

    fn open() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        apply_v2_schema(&c).unwrap();
        c
    }

    #[test]
    fn append_then_read_groups() {
        let c = open();
        let clk = FixedClock::iso("2026-05-04T12:00:00Z");
        for _ in 0..3 {
            append(
                &c,
                &PipelineEvent {
                    level: EventLevel::Error,
                    stage: Some("llm".into()),
                    folder: None,
                    photo_id: None,
                    error_class: Some("llm_unreachable".into()),
                    message: "connection refused".into(),
                    context_json: None,
                },
                &clk,
            )
            .unwrap();
        }
        let groups = unresolved_groups(&c).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].count, 3);
        assert_eq!(groups[0].stage.as_deref(), Some("llm"));
        assert_eq!(groups[0].error_class.as_deref(), Some("llm_unreachable"));
    }

    #[test]
    fn resolve_marks_matching_rows() {
        let c = open();
        let clk = FixedClock::iso("2026-05-04T12:00:00Z");
        for _ in 0..2 {
            append(
                &c,
                &PipelineEvent {
                    level: EventLevel::Error,
                    stage: Some("llm".into()),
                    folder: None,
                    photo_id: None,
                    error_class: Some("llm_unreachable".into()),
                    message: "connection refused".into(),
                    context_json: None,
                },
                &clk,
            )
            .unwrap();
        }
        let n = resolve_group(&c, Some("llm"), Some("llm_unreachable"), &clk).unwrap();
        assert_eq!(n, 2);
        assert_eq!(unresolved_groups(&c).unwrap().len(), 0);
    }

    #[test]
    fn rotation_caps_at_10k() {
        let c = open();
        let clk = FixedClock::iso("2026-05-04T12:00:00Z");
        for i in 0..10_005 {
            append(
                &c,
                &PipelineEvent {
                    level: EventLevel::Info,
                    stage: None,
                    folder: None,
                    photo_id: None,
                    error_class: None,
                    message: format!("event {}", i),
                    context_json: None,
                },
                &clk,
            )
            .unwrap();
        }
        let count: i64 = c
            .query_row("SELECT COUNT(*) FROM pipeline_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 10_000);
    }
}
