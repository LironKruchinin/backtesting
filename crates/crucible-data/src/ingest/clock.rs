//! The wall clock, as an injectable seam.
//!
//! The execute path needs a clock for two things: stamping
//! [`Acquisition::acquired_ts`](crate::catalog::Acquisition) and pacing the
//! poll loop. Neither may be read from inside this crate. D-0015 pinned
//! `acquired_ts` as caller-supplied precisely so library code never reads a
//! clock, and CLAUDE.md §2.2 bans `SystemTime::now` from result-affecting
//! code; a trait keeps both promises while still letting the state machine ask
//! what time it is.
//!
//! **There is no `SystemClock` in this crate.** The only implementation that
//! touches the OS lives in the `crucible-cli` bin target, next to the other
//! environment resolution it already owns (D-0022). That is not ceremony: it
//! is what makes the whole execute state machine — including poll intervals,
//! timeouts, and the 30-day expiry arithmetic — testable offline, instantly,
//! with no sleeping and no dependence on what day it happens to be.

use crucible_core::types::Ts;

/// Wall-clock access for the execute path.
///
/// Implementations live outside this crate (bin targets) and in tests.
pub trait Clock {
    /// Current UTC time in nanoseconds since the Unix epoch.
    fn now_ts(&self) -> Ts;

    /// Blocks for approximately `secs` seconds.
    ///
    /// Used only to pace polling and submission; nothing about a result
    /// depends on how long it actually sleeps.
    fn sleep_secs(&self, secs: u64);
}

/// Nanoseconds in one second, for callers doing deadline arithmetic.
pub const NANOS_PER_SEC: i64 = 1_000_000_000;

#[cfg(test)]
pub(crate) mod fake {
    //! A clock that never sleeps and advances only when asked.

    use super::{Clock, NANOS_PER_SEC};
    use crucible_core::types::Ts;
    use std::cell::Cell;

    /// Deterministic clock: reading it never moves it, and `sleep_secs`
    /// advances it instead of blocking — so a poll loop with a 15-second
    /// interval and a 1-hour deadline runs in microseconds and the elapsed
    /// time it observes is exactly the time it asked to wait.
    pub(crate) struct FakeClock {
        now: Cell<i64>,
        /// Total seconds requested, so a test can assert the loop actually
        /// paced itself rather than spinning.
        slept: Cell<u64>,
    }

    impl FakeClock {
        /// A clock starting at `start`.
        pub(crate) fn new(start: Ts) -> FakeClock {
            FakeClock {
                now: Cell::new(start.0),
                slept: Cell::new(0),
            }
        }

        /// Total seconds this clock was asked to sleep.
        pub(crate) fn slept_secs(&self) -> u64 {
            self.slept.get()
        }
    }

    impl Clock for FakeClock {
        fn now_ts(&self) -> Ts {
            Ts(self.now.get())
        }

        fn sleep_secs(&self, secs: u64) {
            self.slept.set(self.slept.get() + secs);
            let advance = i64::try_from(secs).unwrap_or(i64::MAX / NANOS_PER_SEC);
            self.now.set(self.now.get() + advance * NANOS_PER_SEC);
        }
    }
}
