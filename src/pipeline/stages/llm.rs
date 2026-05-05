//! LLM stage: ask the configured client for description + tag list,
//! write description with provenance-aware UPDATE, write each tag as a
//! `photo_objects` row via the provenance helper.
//!
//! The trait declared here is narrower than the existing `LlmClient` struct
//! by design — it lets us inject mocks in tests and defers wiring the real
//! provider (which currently bakes the custom prompt at provider-construction
//! time) to the daemon driver (Task 21).

use crate::db::clock::Clock;
use crate::db::provenance::{pipeline_write_facet, FacetTable};
use crate::pipeline::stages::{pending_default, PendingPhoto, Stage, StageId, StageOutcome};
use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Arc;

/// What the LLM stage needs from a client. Narrower than crate::llm::LlmClient
/// so tests can mock easily and so we can layer per-call prompts cleanly.
#[allow(dead_code)]
pub trait LlmDescribeClient: Send + Sync {
    fn describe_and_tag_image(
        &self,
        image_path: &Path,
        custom_prompt: Option<&str>,
    ) -> Result<(String, Vec<String>)>;

    fn text_embedding(&self, _text: &str) -> Result<Option<Vec<f32>>> {
        Ok(None)
    }

    fn embedding_model_name(&self) -> &'static str {
        ""
    }
}

#[allow(dead_code)]
pub struct LlmStage {
    pub client: Arc<dyn LlmDescribeClient>,
    pub global_prompt_override: Option<String>,
}

#[allow(dead_code)]
impl LlmStage {
    fn folder_prompt(conn: &Connection, photo_path: &Path) -> Option<String> {
        let dir = photo_path.parent()?.to_string_lossy().to_string();
        conn.query_row(
            "SELECT custom_prompt FROM folder_prompts WHERE path=?1",
            params![dir],
            |r| r.get(0),
        )
        .ok()
    }

    fn description_writable(conn: &Connection, photo_id: i64) -> Result<bool> {
        let row: Option<(Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT description_source, description_confirmed_at FROM photos WHERE id=?1",
                params![photo_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        let Some((src, conf)) = row else {
            return Ok(true);
        };
        let writable = match src.as_deref() {
            None | Some("ai") => conf.is_none(),
            _ => false,
        };
        Ok(writable)
    }

    fn classify_error(msg: &str) -> &'static str {
        let lower = msg.to_lowercase();
        if lower.contains("connection") || lower.contains("refused") {
            "llm_unreachable"
        } else if lower.contains("timeout") {
            "llm_timeout"
        } else if lower.contains("json") {
            "llm_bad_json"
        } else {
            "llm_unknown"
        }
    }
}

impl Stage for LlmStage {
    fn id(&self) -> StageId {
        StageId::Llm
    }

    fn pending(
        &self,
        conn: &Connection,
        folder: Option<&str>,
        limit: usize,
    ) -> Result<Vec<PendingPhoto>> {
        pending_default(conn, "llm_done_at", Some("exif_done_at"), folder, limit)
    }

    fn process_one(
        &self,
        conn: &Connection,
        photo: &PendingPhoto,
        clock: &dyn Clock,
    ) -> Result<StageOutcome> {
        let prompt = Self::folder_prompt(conn, &photo.path)
            .or_else(|| self.global_prompt_override.clone());

        let (description, tags) = match self
            .client
            .describe_and_tag_image(&photo.path, prompt.as_deref())
        {
            Ok((d, t)) => (d, t),
            Err(e) => {
                let msg = format!("{:#}", e);
                let class = Self::classify_error(&msg);
                return Ok(StageOutcome::Err {
                    error_class: class.into(),
                    message: msg,
                });
            }
        };

        let tx = conn.unchecked_transaction()?;

        if Self::description_writable(&tx, photo.id)? {
            tx.execute(
                "UPDATE photos SET description=?1, description_source='ai',
                                  description_confirmed_at=NULL,
                                  updated_at=CURRENT_TIMESTAMP
                 WHERE id=?2",
                params![description, photo.id],
            )?;
        }

        for tag in &tags {
            let tag = tag.trim().to_lowercase();
            if tag.is_empty() {
                continue;
            }
            tx.execute(
                "INSERT OR IGNORE INTO objects(name) VALUES (?1)",
                params![tag],
            )?;
            let object_id: i64 = tx.query_row(
                "SELECT id FROM objects WHERE name=?1",
                params![tag],
                |r| r.get(0),
            )?;
            pipeline_write_facet(
                &tx,
                FacetTable::PhotoObjects,
                photo.id,
                object_id,
                "object",
                &tag,
                clock,
            )?;
        }

        if let Ok(Some(emb)) = self.client.text_embedding(&description) {
            let bytes = embedding_to_bytes(&emb);
            tx.execute(
                "INSERT OR REPLACE INTO embeddings(photo_id, kind, model, dims, vector)
                 VALUES (?1, 'text', ?2, ?3, ?4)",
                params![
                    photo.id,
                    self.client.embedding_model_name(),
                    emb.len() as i64,
                    bytes
                ],
            )?;
        }

        tx.commit()?;
        Ok(StageOutcome::Ok)
    }
}

fn embedding_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(v.len() * 4);
    for f in v {
        buf.extend_from_slice(&f.to_le_bytes());
    }
    buf
}
