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
}

impl TestCounts {
    pub(crate) fn from_results(results: &[RunResult], config_stats: ConfigStats) -> Self {
        let passed = results.iter().filter(|r| r.is_success()).count();
        let failed = results.len() - passed;
        Self {
            config_stats,
            passed,
            failed,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn total(&self) -> usize {
        self.passed + self.failed
    }
}
