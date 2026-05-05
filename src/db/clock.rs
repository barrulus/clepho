//! Clock trait: every place that stamps `now()` (confirmed_at, taken_at fallbacks,
//! pipeline_events occurred_at, etc.) takes a `&dyn Clock` so tests are deterministic.

use chrono::{DateTime, Utc};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub struct FixedClock(DateTime<Utc>);

impl FixedClock {
    pub fn new(ts: DateTime<Utc>) -> Self { Self(ts) }
    pub fn iso(s: &str) -> Self {
        Self(DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc))
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> { self.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_returns_set_time() {
        let c = FixedClock::iso("2026-05-04T12:00:00Z");
        assert_eq!(c.now().to_rfc3339(), "2026-05-04T12:00:00+00:00");
    }

    #[test]
    fn system_clock_advances() {
        let c = SystemClock;
        let a = c.now();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = c.now();
        assert!(b > a);
    }
}
