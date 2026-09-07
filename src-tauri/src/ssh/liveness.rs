//! Noticing that a session's connection has died.
//!
//! A TCP connection that was open when the machine suspended is almost never still there
//! when it comes back, and nothing involved says so: the socket still looks open to the
//! kernel, and the peer is not going to volunteer that it has gone. The only reliable test
//! is a round trip.
//!
//! Two things make this more than a plain timer.
//!
//! Suspend does not advance [`Instant`]. `CLOCK_MONOTONIC` on Linux, `mach_absolute_time`
//! on macOS and the performance counter on Windows all stop with the machine, while the
//! wall clock does not - so russh's own keepalive, which counts in monotonic time, restarts
//! its couple of minutes of counting from scratch on wake. Comparing the two clocks turns
//! that liability into the signal: the difference between them *is* the time spent
//! suspended, which is the exact moment a connection most likely died.

use std::time::{Duration, Instant, SystemTime};

/// How long a suspend has to be before it is worth re-testing the connection.
///
/// Comfortably longer than any scheduling hiccup, short enough that a lid closed over
/// lunch is caught. A wall clock that merely stepped - an NTP correction, a time zone
/// change - is not a suspend, so only forward jumps of real size count.
pub const SUSPEND_THRESHOLD: Duration = Duration::from_secs(20);

/// Granularity of the suspend check. Also the longest a wake goes unnoticed.
const STEP: Duration = Duration::from_secs(2);

/// How much wall clock is allowed to exceed monotonic time before it reads as a suspend.
///
/// The two clocks disagree slightly all the time, and `sleep` overshoots under load.
fn suspended_for(monotonic: Duration, wall: Duration) -> Option<Duration> {
    let drift = wall.checked_sub(monotonic)?;
    (drift >= SUSPEND_THRESHOLD).then_some(drift)
}

/// Why [`sleep_or_suspend`] returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wakeup {
    /// The full duration elapsed with the machine running.
    Elapsed,
    /// The machine was suspended for this long, so the wait was cut short.
    Suspended(Duration),
}

/// Wait for `total`, returning early if the machine suspends.
///
/// Sleeps in short steps and compares elapsed monotonic time against elapsed wall clock
/// after each one. A suspend shows up as wall clock having run on while monotonic time
/// stood still, and there is no reason to keep waiting once that has happened - the
/// connection is very likely already dead.
pub async fn sleep_or_suspend(total: Duration) -> Wakeup {
    let start = Instant::now();
    let wall_start = SystemTime::now();

    loop {
        let elapsed = start.elapsed();
        let Some(remaining) = total.checked_sub(elapsed) else {
            return Wakeup::Elapsed;
        };

        tokio::time::sleep(remaining.min(STEP)).await;

        // `duration_since` fails only if the clock went backwards, which is not a suspend.
        let wall = wall_start.elapsed().unwrap_or_default();
        if let Some(gap) = suspended_for(start.elapsed(), wall) {
            return Wakeup::Suspended(gap);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_drift_is_not_a_suspend() {
        // The clocks always disagree a little, and `sleep` overshoots under load.
        assert_eq!(
            suspended_for(Duration::from_secs(2), Duration::from_millis(2_050)),
            None
        );
    }

    #[test]
    fn a_long_wall_clock_jump_is_a_suspend() {
        let gap = suspended_for(Duration::from_secs(2), Duration::from_secs(3_600));
        assert_eq!(gap, Some(Duration::from_secs(3_598)));
    }

    #[test]
    fn the_threshold_is_inclusive() {
        let wall = Duration::from_secs(1) + SUSPEND_THRESHOLD;
        assert!(suspended_for(Duration::from_secs(1), wall).is_some());
    }

    #[test]
    fn a_clock_that_went_backwards_is_not_a_suspend() {
        // NTP corrections and time zone changes move the wall clock without the machine
        // having stopped. Reporting those as a wake would tear down healthy sessions.
        assert_eq!(
            suspended_for(Duration::from_secs(10), Duration::from_secs(5)),
            None
        );
    }

    #[tokio::test]
    async fn a_quiet_wait_runs_to_completion() {
        // A real, short wait: both clocks run, so this is a machine that stayed awake.
        // Tokio's paused clock is no use here - the check reads the system clocks on
        // purpose, and paused time moves only tokio's.
        assert_eq!(
            sleep_or_suspend(Duration::from_millis(50)).await,
            Wakeup::Elapsed
        );
    }

    #[tokio::test]
    async fn a_wait_shorter_than_a_step_still_ends() {
        assert_eq!(
            sleep_or_suspend(Duration::from_millis(1)).await,
            Wakeup::Elapsed
        );
    }
}
