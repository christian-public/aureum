use crate::test_case_id::TestCaseId;
use std::path::PathBuf;

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestCase {
    pub id: TestCaseId,
    pub program_path: PathBuf, // Expects an absolute path
    pub arguments: Vec<String>,
    pub stdin: Option<String>,
    pub timeout_seconds: u64,
}

impl TestCase {
    pub fn display_id(&self) -> String {
        self.id.display_id()
    }
}

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestCaseExpectations {
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum PlannedTestCase {
    Skip {
        id: TestCaseId,
        reason: String,
    },
    Run {
        test_case: TestCase,
        expectations: TestCaseExpectations,
    },
}
