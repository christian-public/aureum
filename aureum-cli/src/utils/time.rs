use std::time::Duration;

/// Format a duration for human-readable run summaries.
///
/// One rounding rule governs the whole function: round to nearest (never
/// truncate). The duration is rounded to whole seconds up front, and that same
/// rounded value decides both the branch and the minute/second display, so the
/// threshold and the printed value can never disagree. Below a minute we add a
/// finer tenths-of-a-second view of the raw duration.
///
/// Deciding the branch from the rounded value is what avoids the boundary
/// quirk: a raw duration of, say, `59.96s` rounds to `60`, so it renders as
/// `1m 0s` rather than slipping into the sub-minute branch and printing the
/// contradictory `60.0s`.
pub fn format_duration(d: Duration) -> String {
    let rounded_secs = d.as_secs_f64().round() as u64;
    if rounded_secs < 60 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        format!("{}m {}s", rounded_secs / 60, rounded_secs % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_minute_rounds_to_nearest_tenth() {
        assert_eq!(format_duration(Duration::from_millis(1500)), "1.5s");
        assert_eq!(format_duration(Duration::from_millis(1540)), "1.5s");
        assert_eq!(format_duration(Duration::from_millis(1560)), "1.6s");
        // Sub-100ms still rounds rather than truncates.
        assert_eq!(format_duration(Duration::from_millis(40)), "0.0s");
        assert_eq!(format_duration(Duration::from_millis(60)), "0.1s");
    }

    #[test]
    fn minute_branch_rounds_seconds_instead_of_flooring() {
        // 90.7s used to floor to "1m 30s"; it now rounds to "1m 31s".
        assert_eq!(format_duration(Duration::from_millis(90_700)), "1m 31s");
        assert_eq!(format_duration(Duration::from_millis(90_400)), "1m 30s");
    }

    #[test]
    fn exact_minutes_render_cleanly() {
        assert_eq!(format_duration(Duration::from_secs(60)), "1m 0s");
        assert_eq!(format_duration(Duration::from_secs(125)), "2m 5s");
    }

    #[test]
    fn boundary_never_prints_sixty_seconds() {
        // The old code took the sub-minute branch here and printed "60.0s".
        // Rounding the branch decision sends it to the minute branch instead.
        assert_eq!(format_duration(Duration::from_millis(59_960)), "1m 0s");
        assert_eq!(format_duration(Duration::from_millis(59_500)), "1m 0s");
        // Just below the rounding boundary still shows tenths of a second.
        assert_eq!(format_duration(Duration::from_millis(59_400)), "59.4s");
        // A whole minute's worth of seconds rounds up to the next minute.
        assert_eq!(format_duration(Duration::from_millis(119_600)), "2m 0s");
    }
}
