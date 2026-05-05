//! The provenance contract (spec §6).
//!
//! The pipeline writes only where `source = 'ai' AND confirmed_at IS NULL`.
//! Users write anywhere; their writes set `source = 'user'` and `confirmed_at = now()`.
//!
//! This module is the SINGLE place that translates write intents into SQL effects.
//! All facet-write paths (LLM stage, user edits, bulk ops) MUST go through here.

use crate::db::clock::Clock;
use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actor {
    Pipeline,
    User,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FacetTable {
    PhotoObjects,   // photo_objects
    PhotoUserTags,  // photo_user_tags
    Faces,          // faces (for Plan 2; included now for completeness)
}

impl FacetTable {
    fn name(self) -> &'static str {
        match self {
            FacetTable::PhotoObjects => "photo_objects",
            FacetTable::PhotoUserTags => "photo_user_tags",
            FacetTable::Faces => "faces",
        }
    }

    fn id_col(self) -> &'static str {
        match self {
            FacetTable::PhotoObjects => "object_id",
            FacetTable::PhotoUserTags => "tag_id",
            FacetTable::Faces => "id",
        }
    }
}

/// One write outcome — what happened, for callers that want to log/test.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOutcome {
    Inserted,         // new row written
    UpdatedConfirmed, // row existed (ai, unconfirmed) and was confirmed by user
    SkippedExisting,  // row exists with confirmed/user provenance, no-op
    SkippedRejected,  // value is in rejected_suggestions, no-op
    Idempotent,       // row exists with same source, no-op
}

/// Pipeline-side write of an AI suggestion. Honours rejected_suggestions and
/// the contract: never overwrites confirmed/user rows.
#[allow(dead_code)]
pub fn pipeline_write_facet(
    conn: &Connection,
    table: FacetTable,
    photo_id: i64,
    value_id: i64,
    rejected_facet_key: &str,
    rejected_facet_value: &str,
    _clock: &dyn Clock,
) -> Result<WriteOutcome> {
    let is_rejected: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM rejected_suggestions
                        WHERE photo_id = ?1 AND facet = ?2 AND value = ?3)",
        params![photo_id, rejected_facet_key, rejected_facet_value],
        |r| r.get(0),
    )?;
    if is_rejected {
        return Ok(WriteOutcome::SkippedRejected);
    }

    let existing: Option<(String, Option<String>)> = conn
        .query_row(
            &format!(
                "SELECT source, confirmed_at FROM {} WHERE photo_id = ?1 AND {} = ?2",
                table.name(),
                table.id_col()
            ),
            params![photo_id, value_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    match existing {
        Some((source, confirmed_at)) => {
            if source == "user" || confirmed_at.is_some() {
                Ok(WriteOutcome::SkippedExisting)
            } else {
                Ok(WriteOutcome::Idempotent)
            }
        }
        None => {
            conn.execute(
                &format!(
                    "INSERT INTO {} (photo_id, {}, source, confirmed_at) VALUES (?1, ?2, 'ai', NULL)",
                    table.name(),
                    table.id_col()
                ),
                params![photo_id, value_id],
            )?;
            Ok(WriteOutcome::Inserted)
        }
    }
}

/// User adds a value manually. Sets source='user', confirmed_at=now().
#[allow(dead_code)]
pub fn user_add_facet(
    conn: &Connection,
    table: FacetTable,
    photo_id: i64,
    value_id: i64,
    clock: &dyn Clock,
) -> Result<WriteOutcome> {
    let now = clock.now().to_rfc3339();

    let existing: Option<String> = conn
        .query_row(
            &format!(
                "SELECT source FROM {} WHERE photo_id = ?1 AND {} = ?2",
                table.name(),
                table.id_col()
            ),
            params![photo_id, value_id],
            |r| r.get(0),
        )
        .optional()?;

    match existing {
        Some(_) => {
            conn.execute(
                &format!(
                    "UPDATE {} SET source='user', confirmed_at=?3
                     WHERE photo_id=?1 AND {}=?2",
                    table.name(),
                    table.id_col()
                ),
                params![photo_id, value_id, now],
            )?;
            Ok(WriteOutcome::UpdatedConfirmed)
        }
        None => {
            conn.execute(
                &format!(
                    "INSERT INTO {} (photo_id, {}, source, confirmed_at) VALUES (?1, ?2, 'user', ?3)",
                    table.name(),
                    table.id_col()
                ),
                params![photo_id, value_id, now],
            )?;
            Ok(WriteOutcome::Inserted)
        }
    }
}

/// User accepts an AI suggestion: keeps source='ai' but sets confirmed_at.
#[allow(dead_code)]
pub fn user_confirm_facet(
    conn: &Connection,
    table: FacetTable,
    photo_id: i64,
    value_id: i64,
    clock: &dyn Clock,
) -> Result<WriteOutcome> {
    let now = clock.now().to_rfc3339();
    let updated = conn.execute(
        &format!(
            "UPDATE {} SET confirmed_at=?3
             WHERE photo_id=?1 AND {}=?2 AND source='ai' AND confirmed_at IS NULL",
            table.name(),
            table.id_col()
        ),
        params![photo_id, value_id, now],
    )?;
    if updated == 0 {
        bail!("nothing to confirm: row absent or not in (ai,NULL) state");
    }
    Ok(WriteOutcome::UpdatedConfirmed)
}

/// User removes a value (DELETE). Re-suggestable next pipeline run.
#[allow(dead_code)]
pub fn user_remove_facet(
    conn: &Connection,
    table: FacetTable,
    photo_id: i64,
    value_id: i64,
) -> Result<()> {
    conn.execute(
        &format!(
            "DELETE FROM {} WHERE photo_id=?1 AND {}=?2",
            table.name(),
            table.id_col()
        ),
        params![photo_id, value_id],
    )?;
    Ok(())
}

/// User rejects an AI suggestion: DELETE + INSERT into rejected_suggestions.
/// `value` is the human-readable name (object name, tag name, person name).
#[allow(dead_code)]
pub fn user_reject_facet(
    conn: &Connection,
    table: FacetTable,
    photo_id: i64,
    value_id: i64,
    rejected_facet_key: &str,
    rejected_facet_value: &str,
    clock: &dyn Clock,
) -> Result<()> {
    let now = clock.now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        &format!(
            "DELETE FROM {} WHERE photo_id=?1 AND {}=?2",
            table.name(),
            table.id_col()
        ),
        params![photo_id, value_id],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO rejected_suggestions(photo_id, facet, value, rejected_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![photo_id, rejected_facet_key, rejected_facet_value, now],
    )?;
    tx.commit()?;
    Ok(())
}
