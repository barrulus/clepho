//! Per-stage circuit breaker (spec §8.4).
//! Tripped when N consecutive same-class failures occur.
//! Tripping does NOT cascade to other stages.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::pipeline::stages::StageId;

#[derive(Debug, Default)]
struct State {
    last_class: Option<String>,
    consecutive: u32,
    paused: bool,
}

#[allow(dead_code)]
pub struct CircuitBreaker {
    threshold: u32,
    inner: Mutex<HashMap<StageId, State>>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    Open,
    Closed,
    Tripped,
}

#[allow(dead_code)]
impl CircuitBreaker {
    pub fn new(threshold: u32) -> Self {
        Self {
            threshold,
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn record_success(&self, stage: StageId) {
        let mut g = self.inner.lock().unwrap();
        let s = g.entry(stage).or_default();
        s.consecutive = 0;
        s.last_class = None;
        s.paused = false;
    }

    /// Returns true if the breaker just tripped on this failure.
    pub fn record_failure(&self, stage: StageId, error_class: &str) -> bool {
        let mut g = self.inner.lock().unwrap();
        let s = g.entry(stage).or_default();
        if s.last_class.as_deref() == Some(error_class) {
            s.consecutive += 1;
        } else {
            s.last_class = Some(error_class.to_string());
            s.consecutive = 1;
        }
        if !s.paused && s.consecutive >= self.threshold {
            s.paused = true;
            return true;
        }
        false
    }

    pub fn is_paused(&self, stage: StageId) -> bool {
        self.inner
            .lock()
            .unwrap()
            .get(&stage)
            .map(|s| s.paused)
            .unwrap_or(false)
    }

    /// Manually clear the breaker (user pressed Retry in Pipeline Status).
    pub fn reset(&self, stage: StageId) {
        let mut g = self.inner.lock().unwrap();
        if let Some(s) = g.get_mut(&stage) {
            s.consecutive = 0;
            s.last_class = None;
            s.paused = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_consecutive_same_class_trips() {
        let cb = CircuitBreaker::new(3);
        assert!(!cb.record_failure(StageId::Llm, "llm_unreachable"));
        assert!(!cb.record_failure(StageId::Llm, "llm_unreachable"));
        assert!(cb.record_failure(StageId::Llm, "llm_unreachable"));
        assert!(cb.is_paused(StageId::Llm));
    }

    #[test]
    fn different_classes_dont_accumulate() {
        let cb = CircuitBreaker::new(3);
        cb.record_failure(StageId::Llm, "llm_unreachable");
        cb.record_failure(StageId::Llm, "image_decode");
        cb.record_failure(StageId::Llm, "oom");
        assert!(!cb.is_paused(StageId::Llm));
    }

    #[test]
    fn success_resets_counter() {
        let cb = CircuitBreaker::new(3);
        cb.record_failure(StageId::Llm, "llm_unreachable");
        cb.record_failure(StageId::Llm, "llm_unreachable");
        cb.record_success(StageId::Llm);
        cb.record_failure(StageId::Llm, "llm_unreachable");
        assert!(!cb.is_paused(StageId::Llm));
    }

    #[test]
    fn reset_unpauses() {
        let cb = CircuitBreaker::new(2);
        cb.record_failure(StageId::Llm, "x");
        cb.record_failure(StageId::Llm, "x");
        assert!(cb.is_paused(StageId::Llm));
        cb.reset(StageId::Llm);
        assert!(!cb.is_paused(StageId::Llm));
    }

    #[test]
    fn breaker_is_per_stage() {
        let cb = CircuitBreaker::new(2);
        cb.record_failure(StageId::Llm, "x");
        cb.record_failure(StageId::Llm, "x");
        assert!(cb.is_paused(StageId::Llm));
        assert!(!cb.is_paused(StageId::Faces));
    }
}
