use chrono::NaiveTime;
use std::time::Duration;

#[derive(Clone, Copy)]
pub struct StableOutput {
    /// Substituted into the non-interactive "Finished in" summary line.
    pub finished_in: Duration,
    /// Substituted into the progress view's elapsed counter.
    pub elapsed: Duration,
    /// Substituted into the watch idle "Run time" row.
    pub run_time: Duration,
    /// Substituted into the watch idle "Last run" wall-clock value.
    pub finished_at: NaiveTime,
}

impl Default for StableOutput {
    fn default() -> Self {
        Self {
            finished_in: Duration::from_millis(100),
            elapsed: Duration::from_millis(200),
            run_time: Duration::from_millis(300),
            finished_at: NaiveTime::from_hms_opt(12, 0, 0).expect("12:00:00 is a valid time"),
        }
    }
}
