//! Scheduler: pulls pending photos per stage, dispatches to bounded worker pools.
//! Single-process. Owned by the daemon (in daemon mode) or by the TUI (standalone).

use crate::config::PipelineConfig;
use crate::db::clock::Clock;
use crate::db::pipeline_events::{append as ev_append, EventLevel, PipelineEvent};
use crate::pipeline::circuit_breaker::CircuitBreaker;
use crate::pipeline::log::{JsonlAppender, LogRecord};
use crate::pipeline::stages::{mark_done, mark_error, Stage, StageId, StageOutcome};
use anyhow::Result;
use rusqlite::Connection;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[allow(dead_code)]
pub struct Scheduler {
    pub stages: Vec<Arc<dyn Stage>>,
    pub breaker: Arc<CircuitBreaker>,
    pub log: Arc<JsonlAppender>,
    pub config: PipelineConfig,
    pub clock: Arc<dyn Clock>,
    pub cancel: Arc<AtomicBool>,
}

#[allow(dead_code)]
#[derive(Debug, Default, Clone)]
pub struct RunReport {
    pub processed: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub skipped_paused: u64,
}

#[allow(dead_code)]
impl Scheduler {
    /// Run one pass: each stage drains pending photos in its folder up to its worker
    /// budget. Returns aggregate report. Designed to be called repeatedly (daemon ticks)
    /// or once (ad-hoc TUI run).
    pub fn run_pass(&self, conn: &Connection, folder: Option<&str>) -> Result<RunReport> {
        let mut report = RunReport::default();

        for stage in &self.stages {
            if self.cancel.load(Ordering::Relaxed) {
                break;
            }
            if self.breaker.is_paused(stage.id()) {
                report.skipped_paused += 1;
                continue;
            }
            let workers = self.workers_for(stage.id());
            let pending = stage.pending(conn, folder, workers as usize * 4)?;
            if pending.is_empty() {
                continue;
            }

            // Sequential per stage in v1 (rayon-parallel within stages is a Plan 2 polish).
            // The "concurrency" config is honoured by max-per-pass batch size.
            for photo in pending {
                if self.cancel.load(Ordering::Relaxed) {
                    break;
                }
                let start = Instant::now();
                let result = stage.process_one(conn, &photo, &*self.clock);
                let elapsed = start.elapsed().as_millis() as u64;
                report.processed += 1;

                match result {
                    Ok(StageOutcome::Ok) => {
                        mark_done(conn, stage.id(), photo.id, &*self.clock)?;
                        self.breaker.record_success(stage.id());
                        report.succeeded += 1;
                        let _ = self.log.append(&LogRecord {
                            ts: self.clock.now().to_rfc3339(),
                            level: "info",
                            component: "pipeline.scheduler",
                            folder,
                            photo_id: Some(photo.id),
                            photo_path: photo.path.to_str(),
                            stage: Some(stage.id().name()),
                            error_class: None,
                            message: "stage ok",
                            context: None,
                            duration_ms: Some(elapsed),
                        });
                    }
                    Ok(StageOutcome::Err {
                        error_class,
                        message,
                    }) => {
                        let tripped = self.record_failure(
                            conn,
                            stage.id(),
                            &photo,
                            folder,
                            &error_class,
                            &message,
                            elapsed,
                        )?;
                        report.failed += 1;
                        if tripped {
                            break;
                        }
                    }
                    Err(e) => {
                        let cls = "unhandled".to_string();
                        let msg = format!("{:#}", e);
                        let tripped = self.record_failure(
                            conn,
                            stage.id(),
                            &photo,
                            folder,
                            &cls,
                            &msg,
                            elapsed,
                        )?;
                        report.failed += 1;
                        if tripped {
                            break;
                        }
                    }
                }
            }
        }
        Ok(report)
    }

    /// Record one stage failure: writes photos.<stage>_error, appends a
    /// pipeline_events row, emits a JSONL log entry, and bumps the breaker.
    /// Returns true iff this failure caused the breaker to trip *now* (so the
    /// caller can stop hammering the stage on this pass).
    fn record_failure(
        &self,
        conn: &Connection,
        stage_id: StageId,
        photo: &crate::pipeline::stages::PendingPhoto,
        folder: Option<&str>,
        error_class: &str,
        message: &str,
        elapsed_ms: u64,
    ) -> Result<bool> {
        mark_error(conn, stage_id, photo.id, message)?;
        ev_append(
            conn,
            &PipelineEvent {
                level: EventLevel::Error,
                stage: Some(stage_id.name().to_string()),
                folder: folder.map(String::from),
                photo_id: Some(photo.id),
                error_class: Some(error_class.to_string()),
                message: message.to_string(),
                context_json: None,
            },
            &*self.clock,
        )?;
        let _ = self.log.append(&LogRecord {
            ts: self.clock.now().to_rfc3339(),
            level: "error",
            component: "pipeline.scheduler",
            folder,
            photo_id: Some(photo.id),
            photo_path: photo.path.to_str(),
            stage: Some(stage_id.name()),
            error_class: Some(error_class),
            message,
            context: None,
            duration_ms: Some(elapsed_ms),
        });
        let tripped = self.breaker.record_failure(stage_id, error_class);
        if tripped {
            ev_append(
                conn,
                &PipelineEvent {
                    level: EventLevel::Error,
                    stage: Some(stage_id.name().to_string()),
                    folder: folder.map(String::from),
                    photo_id: None,
                    error_class: Some(error_class.to_string()),
                    message: format!(
                        "stage paused after {} consecutive {} failures",
                        self.config.circuit_breaker_threshold, error_class
                    ),
                    context_json: None,
                },
                &*self.clock,
            )?;
        }
        Ok(tripped)
    }

    fn workers_for(&self, stage: StageId) -> u32 {
        match stage {
            StageId::Scan => self.config.scan_workers,
            StageId::Exif => self.config.exif_workers,
            StageId::Thumb => self.config.thumb_workers,
            StageId::Llm => self.config.llm_workers,
            StageId::Faces => self.config.faces_workers,
            StageId::Index => self.config.index_workers,
        }
    }
}
