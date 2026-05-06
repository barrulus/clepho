//! End-to-end: scheduler with a deliberately-failing stage trips the breaker
//! after the configured threshold of same-class failures, leaves other stages
//! running, and resumes after `breaker.reset` (spec §8.4).

use clepho::config::PipelineConfig;
use clepho::db::{apply_v2_schema, SystemClock};
use clepho::pipeline::circuit_breaker::CircuitBreaker;
use clepho::pipeline::log::JsonlAppender;
use clepho::pipeline::scheduler::Scheduler;
use clepho::pipeline::stages::{pending_default, PendingPhoto, Stage, StageId, StageOutcome};
use rusqlite::Connection;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

struct AlwaysFailLlm;
impl Stage for AlwaysFailLlm {
    fn id(&self) -> StageId {
        StageId::Llm
    }
    fn pending(
        &self,
        c: &Connection,
        f: Option<&str>,
        n: usize,
    ) -> anyhow::Result<Vec<PendingPhoto>> {
        pending_default(c, "llm_done_at", Some("exif_done_at"), f, n)
    }
    fn process_one(
        &self,
        _c: &Connection,
        _p: &PendingPhoto,
        _clk: &dyn clepho::db::Clock,
    ) -> anyhow::Result<StageOutcome> {
        Ok(StageOutcome::Err {
            error_class: "llm_unreachable".into(),
            message: "connection refused".into(),
        })
    }
}

struct AlwaysOkExif;
impl Stage for AlwaysOkExif {
    fn id(&self) -> StageId {
        StageId::Exif
    }
    fn pending(
        &self,
        c: &Connection,
        f: Option<&str>,
        n: usize,
    ) -> anyhow::Result<Vec<PendingPhoto>> {
        pending_default(c, "exif_done_at", Some("scan_done_at"), f, n)
    }
    fn process_one(
        &self,
        _c: &Connection,
        _p: &PendingPhoto,
        _clk: &dyn clepho::db::Clock,
    ) -> anyhow::Result<StageOutcome> {
        Ok(StageOutcome::Ok)
    }
}

fn seed(c: &Connection, n: usize) {
    for i in 1..=n {
        c.execute(
            &format!(
                "INSERT INTO photos(path, scan_done_at, exif_done_at)
                 VALUES ('/p/p{}.jpg', '2026-01-01', '2026-01-01')",
                i
            ),
            [],
        )
        .unwrap();
    }
}

fn build_scheduler(stages: Vec<Arc<dyn Stage>>, breaker: Arc<CircuitBreaker>) -> Scheduler {
    let log_dir = tempfile::tempdir().unwrap().keep();
    Scheduler {
        stages,
        breaker,
        log: Arc::new(JsonlAppender::new(log_dir).unwrap()),
        config: PipelineConfig::default(),
        clock: Arc::new(SystemClock),
        cancel: Arc::new(AtomicBool::new(false)),
    }
}

#[test]
fn three_same_class_failures_pause_stage() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    seed(&c, 10);

    let breaker = Arc::new(CircuitBreaker::new(3));
    let sched = build_scheduler(vec![Arc::new(AlwaysFailLlm)], breaker.clone());

    sched.run_pass(&c, None).unwrap();

    assert!(
        breaker.is_paused(StageId::Llm),
        "breaker should be tripped after >=3 failures"
    );

    let error_events: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM pipeline_events WHERE level='error' AND stage='llm'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        error_events >= 3,
        "expected at least 3 error events, got {}",
        error_events
    );

    // The trip itself appends an extra "stage paused" event with photo_id=NULL.
    let paused_events: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM pipeline_events
             WHERE level='error' AND stage='llm' AND photo_id IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        paused_events, 1,
        "exactly one 'stage paused' event should have been logged"
    );
}

#[test]
fn breaker_reset_resumes_stage() {
    let breaker = CircuitBreaker::new(2);
    breaker.record_failure(StageId::Llm, "x");
    breaker.record_failure(StageId::Llm, "x");
    assert!(breaker.is_paused(StageId::Llm));
    breaker.reset(StageId::Llm);
    assert!(!breaker.is_paused(StageId::Llm));
}

#[test]
fn paused_stage_skipped_other_stages_continue() {
    let c = Connection::open_in_memory().unwrap();
    apply_v2_schema(&c).unwrap();
    // Photos pending exif (scan done, exif NOT done).
    for i in 1..=5 {
        c.execute(
            &format!(
                "INSERT INTO photos(path, scan_done_at) VALUES ('/p/p{}.jpg', '2026-01-01')",
                i
            ),
            [],
        )
        .unwrap();
    }
    // Photos pending llm (everything but llm done).
    for i in 6..=10 {
        c.execute(
            &format!(
                "INSERT INTO photos(path, scan_done_at, exif_done_at)
                 VALUES ('/p/p{}.jpg', '2026-01-01', '2026-01-01')",
                i
            ),
            [],
        )
        .unwrap();
    }

    let breaker = Arc::new(CircuitBreaker::new(3));
    // Pre-trip the LLM breaker so the scheduler skips it from the start.
    breaker.record_failure(StageId::Llm, "llm_unreachable");
    breaker.record_failure(StageId::Llm, "llm_unreachable");
    breaker.record_failure(StageId::Llm, "llm_unreachable");
    assert!(breaker.is_paused(StageId::Llm));

    let sched = build_scheduler(
        vec![Arc::new(AlwaysOkExif), Arc::new(AlwaysFailLlm)],
        breaker.clone(),
    );

    let report = sched.run_pass(&c, None).unwrap();
    assert!(report.skipped_paused >= 1, "Llm should have been skipped");
    // Exif still ran on the 5 pending photos.
    assert!(
        report.succeeded >= 5,
        "exif should have processed the pending photos, got succeeded={}",
        report.succeeded
    );
}

#[test]
fn breaker_recovers_after_a_single_success() {
    let breaker = CircuitBreaker::new(3);
    breaker.record_failure(StageId::Llm, "x");
    breaker.record_failure(StageId::Llm, "x");
    breaker.record_success(StageId::Llm);
    breaker.record_failure(StageId::Llm, "x");
    breaker.record_failure(StageId::Llm, "x");
    assert!(
        !breaker.is_paused(StageId::Llm),
        "success between failures should reset the consecutive counter"
    );
}
