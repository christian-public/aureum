use aureum::RunResult;

#[derive(Clone, Copy)]
pub(crate) struct TestCounts {
    pub passed: usize,
    pub failed: usize,
}

impl TestCounts {
    pub(crate) fn from_results(results: &[RunResult]) -> Self {
        let passed = results.iter().filter(|r| r.is_success()).count();
        let failed = results.len() - passed;
        Self { passed, failed }
    }

    #[allow(dead_code)]
    pub(crate) fn total(&self) -> usize {
        self.passed + self.failed
    }
}
