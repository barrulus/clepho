//! Pipeline: per-photo state machine (spec §4).
//! Stages, scheduler, log appender, circuit breaker.

pub mod circuit_breaker;
pub mod llm_adapter;
pub mod log;
pub mod reprocess;
pub mod scheduler;
pub mod stages;
pub mod watcher;
