//! Stage trait + registry. Each stage owns one column in `photos` (e.g. `llm_done_at`)
//! and one error column (e.g. `llm_error`). The scheduler pulls pending photos via
//! `pending`, dispatches `process_one`, and records success/failure.

use crate::db::clock::Clock;
use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;

pub mod exif;
pub mod index;
pub mod llm;
pub mod scan;
pub mod thumb;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StageId {
    Scan,
    Exif,
    Thumb,
    Llm,
    Faces,
    Index,
}

impl StageId {
    #[allow(dead_code)]
    pub fn name(self) -> &'static str {
        match self {
            Self::Scan => "scan",
            Self::Exif => "exif",
            Self::Thumb => "thumb",
            Self::Llm => "llm",
            Self::Faces => "faces",
            Self::Index => "index",
        }
    }

    #[allow(dead_code)]
    pub fn done_col(self) -> &'static str {
        match self {
            Self::Scan => "scan_done_at",
            Self::Exif => "exif_done_at",
            Self::Thumb => "thumb_done_at",
            Self::Llm => "llm_done_at",
            Self::Faces => "faces_done_at",
            Self::Index => "index_done_at",
        }
    }

    #[allow(dead_code)]
    pub fn error_col(self) -> &'static str {
        match self {
            Self::Scan => "scan_error",
            Self::Exif => "exif_error",
            Self::Thumb => "thumb_error",
            Self::Llm => "llm_error",
            Self::Faces => "faces_error",
            Self::Index => "index_error",
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PendingPhoto {
    pub id: i64,
    pub path: PathBuf,
}

/// One stage's outcome on one photo.
#[allow(dead_code)]
#[derive(Debug)]
pub enum StageOutcome {
    Ok,
    Err {
        error_class: String,
        message: String,
    },
}

/// Stage trait. Implementations are stateless; they receive the connection
/// and clock from the scheduler. Heavy resources (LLM client, face engine)
/// are owned by the impl as `Arc`-ed fields.
#[allow(dead_code)]
pub trait Stage: Send + Sync {
    fn id(&self) -> StageId;

    /// Returns photos pending this stage in the given folder (or all folders if None).
    /// Must respect the stage DAG (e.g. exif requires scan_done_at IS NOT NULL).
    fn pending(
        &self,
        conn: &Connection,
        folder: Option<&str>,
        limit: usize,
    ) -> Result<Vec<PendingPhoto>>;

    /// Process one photo. Implementations MUST set <stage>_done_at and outputs in
    /// the same transaction. On error, return Err variant with classification.
    fn process_one(
        &self,
        conn: &Connection,
        photo: &PendingPhoto,
        clock: &dyn Clock,
    ) -> Result<StageOutcome>;
}

/// Default `pending` query for stages whose only prerequisite is one earlier stage.
/// Used by exif (requires scan), thumb/llm/faces (require exif), index (no prereq).
#[allow(dead_code)]
pub fn pending_default(
    conn: &Connection,
    done_col: &str,
    prereq_col: Option<&str>,
    folder: Option<&str>,
    limit: usize,
) -> Result<Vec<PendingPhoto>> {
    let prereq_clause = match prereq_col {
        Some(p) => format!(" AND {} IS NOT NULL", p),
        None => String::new(),
    };

    let sql = match folder {
        Some(_) => format!(
            "SELECT id, path FROM photos
             WHERE {} IS NULL{} AND path LIKE ?1
             ORDER BY id ASC LIMIT ?2",
            done_col, prereq_clause
        ),
        None => format!(
            "SELECT id, path FROM photos
             WHERE {} IS NULL{}
             ORDER BY id ASC LIMIT ?1",
            done_col, prereq_clause
        ),
    };

    let mut stmt = conn.prepare(&sql)?;
    let rows = match folder {
        Some(f) => {
            let prefix = format!("{}/%", f.trim_end_matches('/'));
            stmt.query_map(rusqlite::params![prefix, limit as i64], |r| {
                Ok(PendingPhoto {
                    id: r.get(0)?,
                    path: PathBuf::from(r.get::<_, String>(1)?),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        }
        None => stmt
            .query_map(rusqlite::params![limit as i64], |r| {
                Ok(PendingPhoto {
                    id: r.get(0)?,
                    path: PathBuf::from(r.get::<_, String>(1)?),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?,
    };
    Ok(rows)
}

/// Helper for stage workers: write success (sets done_at, clears error).
#[allow(dead_code)]
pub fn mark_done(
    conn: &Connection,
    stage: StageId,
    photo_id: i64,
    clock: &dyn Clock,
) -> Result<()> {
    let now = clock.now().to_rfc3339();
    conn.execute(
        &format!(
            "UPDATE photos SET {}=?1, {}=NULL WHERE id=?2",
            stage.done_col(),
            stage.error_col()
        ),
        rusqlite::params![now, photo_id],
    )?;
    Ok(())
}

/// Helper for stage workers: write failure (sets error, leaves done_at NULL).
#[allow(dead_code)]
pub fn mark_error(conn: &Connection, stage: StageId, photo_id: i64, message: &str) -> Result<()> {
    conn.execute(
        &format!("UPDATE photos SET {}=?1 WHERE id=?2", stage.error_col()),
        rusqlite::params![message, photo_id],
    )?;
    Ok(())
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
    fn pending_default_filters_by_done_col_and_prereq() {
        let c = open();
        // 3 photos: A (scan_done, exif_done), B (scan_done, no exif), C (no scan)
        c.execute(
            "INSERT INTO photos(path, scan_done_at, exif_done_at) VALUES
             ('a.jpg','2026-01-01T00:00:00Z','2026-01-01T00:00:01Z'),
             ('b.jpg','2026-01-01T00:00:00Z',NULL),
             ('c.jpg',NULL,NULL)",
            [],
        )
        .unwrap();

        // exif: pending = scan_done_at IS NOT NULL AND exif_done_at IS NULL → just B
        let r = pending_default(&c, "exif_done_at", Some("scan_done_at"), None, 10).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].path, PathBuf::from("b.jpg"));

        // scan: pending = scan_done_at IS NULL → just C
        let r = pending_default(&c, "scan_done_at", None, None, 10).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].path, PathBuf::from("c.jpg"));
    }

    #[test]
    fn pending_default_filters_by_folder_prefix() {
        let c = open();
        c.execute(
            "INSERT INTO photos(path) VALUES
             ('/a/x.jpg'),('/a/sub/y.jpg'),('/b/z.jpg')",
            [],
        )
        .unwrap();
        let r = pending_default(&c, "scan_done_at", None, Some("/a"), 10).unwrap();
        let paths: Vec<_> = r.iter().map(|p| p.path.clone()).collect();
        assert!(paths.contains(&PathBuf::from("/a/x.jpg")));
        assert!(paths.contains(&PathBuf::from("/a/sub/y.jpg")));
        assert!(!paths.contains(&PathBuf::from("/b/z.jpg")));
    }

    #[test]
    fn mark_done_sets_timestamp_and_clears_error() {
        let c = open();
        c.execute(
            "INSERT INTO photos(path, scan_error) VALUES ('p.jpg','prior failure')",
            [],
        )
        .unwrap();
        let id: i64 = c
            .query_row("SELECT id FROM photos WHERE path='p.jpg'", [], |r| r.get(0))
            .unwrap();

        let clk = FixedClock::iso("2026-05-04T12:00:00Z");
        mark_done(&c, StageId::Scan, id, &clk).unwrap();

        let (done, err): (Option<String>, Option<String>) = c
            .query_row(
                "SELECT scan_done_at, scan_error FROM photos WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(done.unwrap(), "2026-05-04T12:00:00+00:00");
        assert!(err.is_none());
    }

    #[test]
    fn mark_error_sets_message_and_leaves_done_null() {
        let c = open();
        c.execute("INSERT INTO photos(path) VALUES ('p.jpg')", [])
            .unwrap();
        let id: i64 = c
            .query_row("SELECT id FROM photos WHERE path='p.jpg'", [], |r| r.get(0))
            .unwrap();
        mark_error(&c, StageId::Llm, id, "boom").unwrap();
        let (done, err): (Option<String>, Option<String>) = c
            .query_row(
                "SELECT llm_done_at, llm_error FROM photos WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(done.is_none());
        assert_eq!(err.as_deref(), Some("boom"));
    }
}
