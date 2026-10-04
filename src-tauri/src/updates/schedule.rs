//! When the next update check is due.
//!
//! Quota is a tray application that can run for weeks, so one check at start is
//! not enough: another is due every [`CHECK_INTERVAL`] for as long as it runs.
//!
//! The schedule never sleeps for the whole interval. It is asked every
//! [`WAKE_POLL`] whether a check is due, and answers from a [`Clock`]. That is
//! what lets it survive a suspended machine: the operating systems disagree on
//! whether their monotonic clock counts time spent asleep, but the wall clock
//! always does, so elapsed time is the larger of the two readings. A machine
//! that sleeps for three days is due on the first poll after it wakes, and is
//! asked once, not three times, because the next interval starts when that
//! check finishes. A wall clock set backwards cannot postpone a check by more
//! than the monotonic clock allows.

use std::time::{Duration, Instant, SystemTime};

/// How long after one check the next is due. The one place this is decided.
pub(crate) const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// How often a running application asks whether a check is due.
pub(crate) const WAKE_POLL: Duration = Duration::from_secs(60);

/// One reading of both clocks.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Moment {
    /// Time on the monotonic clock since an arbitrary fixed start.
    pub monotonic: Duration,
    /// The wall clock.
    pub wall: SystemTime,
}

impl Moment {
    /// Time that has passed since `earlier`, by whichever clock saw more.
    fn since(self, earlier: Self) -> Duration {
        let monotonic = self.monotonic.saturating_sub(earlier.monotonic);
        let wall = self.wall.duration_since(earlier.wall).unwrap_or_default();
        monotonic.max(wall)
    }
}

/// A source of [`Moment`]s, so a test can move time by hand.
pub(crate) trait Clock: Send + Sync {
    /// The current reading of both clocks.
    fn now(&self) -> Moment;
}

/// The real clocks.
pub(crate) struct SystemClock {
    start: Instant,
}

impl SystemClock {
    /// A clock whose monotonic reading starts at zero now.
    pub(crate) fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Moment {
        Moment {
            monotonic: self.start.elapsed(),
            wall: SystemTime::now(),
        }
    }
}

/// Remembers when the last check finished.
#[derive(Debug, Default)]
pub(crate) struct Schedule {
    last_finished: Option<Moment>,
}

impl Schedule {
    /// Whether a check is due at `now`: always the first time, then once
    /// [`CHECK_INTERVAL`] has passed since the last one finished.
    pub(crate) fn is_due(&self, now: Moment) -> bool {
        self.last_finished
            .is_none_or(|finished| now.since(finished) >= CHECK_INTERVAL)
    }

    /// Records that a check finished at `now`, which starts the next interval.
    pub(crate) fn finished(&mut self, now: Moment) {
        self.last_finished = Some(now);
    }
}

#[cfg(test)]
pub(crate) mod fake {
    //! A clock a test moves by hand.

    use std::sync::Mutex;

    use super::{Clock, Duration, Moment, SystemTime};

    /// Both clocks, advanced together or apart.
    pub(crate) struct FakeClock {
        state: Mutex<Moment>,
    }

    impl FakeClock {
        pub(crate) fn new() -> Self {
            Self {
                state: Mutex::new(Moment {
                    monotonic: Duration::ZERO,
                    wall: SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000),
                }),
            }
        }

        /// Time passes on both clocks.
        pub(crate) fn advance(&self, by: Duration) {
            let mut state = self.state.lock().unwrap();
            state.monotonic += by;
            state.wall += by;
        }

        /// The machine sleeps: only the wall clock sees the time.
        pub(crate) fn sleep(&self, by: Duration) {
            self.state.lock().unwrap().wall += by;
        }

        /// Someone sets the wall clock back.
        pub(crate) fn set_wall_back(&self, by: Duration) {
            self.state.lock().unwrap().wall -= by;
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Moment {
            *self.state.lock().unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeClock;
    use super::*;

    const HOUR: Duration = Duration::from_secs(60 * 60);
    const ONE_SECOND_SHORT_OF_A_DAY: Duration = Duration::from_secs(24 * 60 * 60 - 1);
    const AN_HOUR_SHORT_OF_A_DAY: Duration = Duration::from_secs(23 * 60 * 60);

    #[test]
    fn the_interval_is_one_day() {
        assert_eq!(CHECK_INTERVAL, 24 * HOUR);
    }

    #[test]
    fn the_first_check_is_due_at_once() {
        let clock = FakeClock::new();
        assert!(Schedule::default().is_due(clock.now()));
    }

    #[test]
    fn nothing_is_due_before_the_interval_has_passed() {
        let clock = FakeClock::new();
        let mut schedule = Schedule::default();
        schedule.finished(clock.now());
        clock.advance(ONE_SECOND_SHORT_OF_A_DAY);
        assert!(!schedule.is_due(clock.now()));
    }

    #[test]
    fn a_check_is_due_when_the_interval_has_passed() {
        let clock = FakeClock::new();
        let mut schedule = Schedule::default();
        schedule.finished(clock.now());
        clock.advance(CHECK_INTERVAL);
        assert!(schedule.is_due(clock.now()));
    }

    #[test]
    fn the_next_interval_starts_when_a_check_finishes() {
        let clock = FakeClock::new();
        let mut schedule = Schedule::default();
        schedule.finished(clock.now());
        clock.advance(CHECK_INTERVAL + 3 * HOUR);
        assert!(schedule.is_due(clock.now()));
        schedule.finished(clock.now());
        clock.advance(AN_HOUR_SHORT_OF_A_DAY);
        assert!(!schedule.is_due(clock.now()));
        clock.advance(HOUR);
        assert!(schedule.is_due(clock.now()));
    }

    #[test]
    fn time_slept_through_counts_towards_the_interval() {
        // The monotonic clock stood still while the machine was suspended.
        let clock = FakeClock::new();
        let mut schedule = Schedule::default();
        schedule.finished(clock.now());
        clock.advance(HOUR);
        clock.sleep(CHECK_INTERVAL);
        assert!(schedule.is_due(clock.now()));
    }

    #[test]
    fn a_wall_clock_set_back_does_not_postpone_a_check() {
        let clock = FakeClock::new();
        let mut schedule = Schedule::default();
        schedule.finished(clock.now());
        clock.advance(CHECK_INTERVAL);
        clock.set_wall_back(30 * 24 * HOUR);
        assert!(schedule.is_due(clock.now()));
    }
}
