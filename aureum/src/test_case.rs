use crate::scratch::ScratchPlan;
use crate::test_id::TestId;
use std::path::PathBuf;

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestCase {
    pub id: TestId,
    pub program_path: PathBuf, // Expects an absolute path
    pub arguments: Vec<String>,
    pub stdin: Option<String>,
    pub timeout_seconds: u64,
    /// Per-test isolation plan. When `Some`, the runner creates this
    /// directory and materialises copies/embeds before launching the
    /// test program, and uses `plan.dir` as the program's `cwd`.
    /// When `None`, the program runs in the config file's directory.
    pub scratch_plan: Option<ScratchPlan>,
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

// `Run` is much larger than `Skip` (it carries a `TestCase` by value).
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum PlannedTestCase {
    Skip {
        id: TestId,
        reason: String,
    },
    Run {
        test_case: TestCase,
        expectations: TestCaseExpectations,
    },
}
