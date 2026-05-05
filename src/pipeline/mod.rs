//! Pipeline: per-photo state machine (spec §4).
//! Stages, scheduler, log appender, circuit breaker.

pub mod circuit_breaker;
pub mod log;
pub mod stages;
