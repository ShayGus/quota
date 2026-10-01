//! Injected time.
//!
//! Scheduling code never calls `Utc::now()` directly, so a test can drive a
//! clock and observe the decisions a policy makes.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};

/// A source of both wall-clock instants and elapsed time.
pub trait Clock: Send + Sync + std::fmt::Debug {
    /// The current UTC instant, used for persisted boundaries.
    fn now(&self) -> DateTime<Utc>;

    /// Time elapsed inside this process, used for scheduling arithmetic.
    ///
    /// A wall-clock jump must not change the result.
    fn elapsed(&self) -> StdDuration;
}

/// The production clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn elapsed(&self) -> StdDuration {
        PROCESS_START.elapsed()
    }
}

static PROCESS_START: std::sync::LazyLock<std::time::Instant> =
    std::sync::LazyLock::new(std::time::Instant::now);

#[derive(Debug)]
struct ClockState {
    wall: DateTime<Utc>,
    elapsed: StdDuration,
}

/// A clock a test drives by hand.
#[derive(Clone, Debug)]
pub struct TestClock {
    instant: Arc<std::sync::Mutex<ClockState>>,
}

impl TestClock {
    /// Creates a clock starting at `start`.
    #[must_use]
    pub fn new(start: DateTime<Utc>) -> Self {
        Self {
            instant: Arc::new(std::sync::Mutex::new(ClockState {
                wall: start,
                elapsed: StdDuration::ZERO,
            })),
        }
    }

    /// Moves wall time and elapsed time forward together.
    ///
    /// # Panics
    /// Panics only if the internal mutex is poisoned, which cannot happen while
    /// this type is only reached through `&self`.
    pub fn advance(&self, by: Duration) {
        let mut state = self
            .instant
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.wall += by;
        state.elapsed += StdDuration::from_secs(by.num_seconds().unsigned_abs());
    }

    /// Moves only wall time, simulating a clock jump or a timezone change.
    ///
    /// # Panics
    /// Panics only if the internal mutex is poisoned.
    pub fn jump_wall(&self, by: Duration) {
        let mut state = self
            .instant
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.wall += by;
    }
}

impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        self.instant
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .wall
    }

    fn elapsed(&self) -> StdDuration {
        self.instant
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .elapsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_clock_only_moves_when_told() {
        let start = DateTime::UNIX_EPOCH;
        let clock = TestClock::new(start);
        assert_eq!(clock.now(), start);
        clock.advance(Duration::minutes(5));
        assert_eq!(clock.now(), start + Duration::minutes(5));
        assert_eq!(clock.elapsed(), StdDuration::from_secs(300));
    }

    #[test]
    fn a_wall_clock_jump_does_not_move_elapsed_time() {
        let clock = TestClock::new(DateTime::UNIX_EPOCH);
        clock.jump_wall(Duration::hours(3));
        assert_eq!(clock.elapsed(), StdDuration::ZERO);
        assert_eq!(clock.now(), DateTime::UNIX_EPOCH + Duration::hours(3));
    }
}
