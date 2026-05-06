//! Force-reprocess primitive (spec §4.4).
//!
//! Clears `<stage>_done_at` and `<stage>_error` columns for photos under a
//! folder so the scheduler will re-run those stages. Provenance is honoured
//! at write time by the stage executors — `apply_reset` only flips the gate
//! columns and never deletes facet rows.

use crate::pipeline::stages::StageId;
use anyhow::Result;
use rusqlite::Connection;

/// Reset the chosen stages for every photo under `folder`. `index_done_at` is
/// always cleared too because index inputs may have changed. Returns the
/// total number of UPDATE row-touches across all queries (i.e. summed across
/// stages, not the count of distinct photos affected).
#[allow(dead_code)]
pub fn apply_reset(conn: &Connection, folder: &str, stages: &[StageId]) -> Result<usize> {
    let prefix = format!("{}/%", folder.trim_end_matches('/'));
    let mut total = 0usize;
    let mut needs_index_clear = false;
    for s in stages {
        let n = conn.execute(
            &format!(
                "UPDATE photos SET {}=NULL, {}=NULL WHERE path LIKE ?1",
                s.done_col(),
                s.error_col()
            ),
            rusqlite::params![prefix],
        )?;
        total += n;
        if *s != StageId::Index {
            needs_index_clear = true;
        }
    }
    if needs_index_clear {
        conn.execute(
            "UPDATE photos SET index_done_at=NULL WHERE path LIKE ?1",
            rusqlite::params![prefix],
        )?;
    }
    Ok(total)
}

/// Pre-flight count: how many user-edited values would survive a reset for
/// this folder. Surfaces in the dialog so the user knows what's at stake.
#[allow(dead_code)]
pub fn count_user_values(conn: &Connection, folder: &str) -> Result<i64> {
    let prefix = format!("{}/%", folder.trim_end_matches('/'));
    let n: i64 = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM photos WHERE path LIKE ?1
                  AND description_source IN ('user','ai_edited'))
              + (SELECT COUNT(*) FROM photo_objects po JOIN photos p ON p.id=po.photo_id
                  WHERE p.path LIKE ?1 AND po.source='user')
              + (SELECT COUNT(*) FROM photo_user_tags pt JOIN photos p ON p.id=pt.photo_id
                  WHERE p.path LIKE ?1 AND pt.source='user')",
        rusqlite::params![prefix],
        |r| r.get(0),
    )?;
    Ok(n)
}

/// Pre-flight count: how many AI-confirmed facet values would survive.
#[allow(dead_code)]
pub fn count_confirmed_values(conn: &Connection, folder: &str) -> Result<i64> {
    let prefix = format!("{}/%", folder.trim_end_matches('/'));
    let n: i64 = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM photo_objects po JOIN photos p ON p.id=po.photo_id
                  WHERE p.path LIKE ?1 AND po.source='ai' AND po.confirmed_at IS NOT NULL)
              + (SELECT COUNT(*) FROM photo_user_tags pt JOIN photos p ON p.id=pt.photo_id
                  WHERE p.path LIKE ?1 AND pt.source='ai' AND pt.confirmed_at IS NOT NULL)",
        rusqlite::params![prefix],
        |r| r.get(0),
    )?;
    Ok(n)
}
