//! Midnight and exit purge of in-memory usage history.
//!
//! Local time comes from `jiff`, which reads the system's IANA time zone
//! database itself, so the date change is correct across DST transitions
//! (see the tests below).

use std::sync::Mutex;

use jiff::Zoned;
use jiff::civil::Date;

/// Source of the current local time. Injected so tests can move the clock.
pub trait Clock: Send + Sync {
    fn now(&self) -> Zoned;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Zoned {
        Zoned::now()
    }
}

/// A clock that only moves when told to.
pub struct FakeClock(Mutex<Zoned>);

impl FakeClock {
    pub fn new(start: Zoned) -> Self {
        Self(Mutex::new(start))
    }

    pub fn set(&self, to: Zoned) {
        match self.0.lock() {
            Ok(mut guard) => *guard = to,
            Err(poisoned) => *poisoned.into_inner() = to,
        }
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Zoned {
        match self.0.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

/// Remembers the local date of the last check and reports when it changes.
/// Checking on every poller tick (rather than sleeping until midnight) also
/// catches a machine that was asleep at midnight.
pub struct MidnightWiper {
    last_date: Date,
}

impl MidnightWiper {
    pub fn new(now: &Zoned) -> Self {
        Self { last_date: now.date() }
    }

    /// True when the local date differs from the last check, meaning history
    /// must be wiped.
    pub fn should_wipe(&mut self, now: &Zoned) -> bool {
        let today = now.date();
        if today != self.last_date {
            self.last_date = today;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> Zoned {
        s.parse().unwrap()
    }

    #[test]
    fn fires_once_per_date_change() {
        let mut w = MidnightWiper::new(&at("2026-10-16T23:59:00-04:00[America/New_York]"));
        assert!(!w.should_wipe(&at("2026-10-16T23:59:59-04:00[America/New_York]")));
        assert!(w.should_wipe(&at("2026-10-17T00:00:00-04:00[America/New_York]")));
        assert!(!w.should_wipe(&at("2026-10-17T08:00:00-04:00[America/New_York]")));
    }

    #[test]
    fn fires_after_sleeping_through_midnight() {
        let mut w = MidnightWiper::new(&at("2026-10-16T22:00:00+00:00[UTC]"));
        assert!(w.should_wipe(&at("2026-10-19T09:00:00+00:00[UTC]")));
    }

    #[test]
    fn handles_dst_fall_back() {
        // 2026-11-01 01:30 happens twice in New York; the date does not change.
        let mut w = MidnightWiper::new(&at("2026-11-01T01:30:00-04:00[America/New_York]"));
        assert!(!w.should_wipe(&at("2026-11-01T01:30:00-05:00[America/New_York]")));
    }
}
