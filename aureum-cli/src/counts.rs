use aureum::{PlannedTestCase, RunResult, RunResultKind};

#[derive(Clone, Copy, Default)]
pub(crate) struct ConfigStats {
    pub config_errors: usize,
}

#[derive(Clone, Copy)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub(crate) struct PlannedCounts {
    pub runnable: usize,
    pub skipped: usize,
}

impl PlannedCounts {
    pub(crate) fn from_planned(cases: &[PlannedTestCase]) -> Self {
        let mut runnable = 0;
        let mut skipped = 0;
        for case in cases {
            match case {
                PlannedTestCase::Skip { .. } => skipped += 1,
                PlannedTestCase::Run { .. } => runnable += 1,
            }
        }
        Self { runnable, skipped }
    }

    pub(crate) fn total(&self) -> usize {
        self.runnable + self.skipped
    }
}

#[derive(Clone, Copy)]
pub(crate) struct TestCounts {
    pub config_stats: ConfigStats,
    pub skipped: usize,
    pub passed: usize,
    pub failed: usize,
}

impl TestCounts {
    pub(crate) fn from_results(results: &[RunResult], config_stats: ConfigStats) -> Self {
        let mut skipped = 0;
        let mut passed = 0;
        let mut failed = 0;
        for result in results {
            match result.kind() {
                RunResultKind::Skipped => skipped += 1,
                RunResultKind::Passed => passed += 1,
                RunResultKind::Failed => failed += 1,
            }
        }
        Self {
            config_stats,
            skipped,
            passed,
            failed,
        }
    }
}
