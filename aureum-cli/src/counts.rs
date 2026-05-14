use aureum::RunResult;

#[derive(Clone, Copy, Default)]
pub(crate) struct ConfigStats {
    pub config_errors: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct TestCounts {
    pub config_stats: ConfigStats,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}

impl TestCounts {
    pub(crate) fn from_results(results: &[RunResult], config_stats: ConfigStats) -> Self {
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;
        for r in results {
            match r {
                RunResult::Skipped { .. } => skipped += 1,
                RunResult::Ran { .. } if r.is_success() => passed += 1,
                RunResult::Ran { .. } => failed += 1,
            }
        }
        Self {
            config_stats,
            passed,
            failed,
            skipped,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn total(&self) -> usize {
        self.passed + self.failed + self.skipped
    }
}
