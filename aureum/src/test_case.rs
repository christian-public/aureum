use crate::test_id::{self, TestId};
use relative_path::RelativePathBuf;
use std::path::PathBuf;

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestCase {
    pub path_to_containing_dir: RelativePathBuf,
    pub file_name: String,
    pub test_id: TestId,
    pub program_path: PathBuf, // Expects an absolute path
    pub arguments: Vec<String>,
    pub stdin: Option<String>,
    pub timeout_seconds: Option<u64>,
}

impl TestCase {
    pub fn id(&self) -> String {
        test_id::format_test_id(self.path_to_config_file(), &self.test_id)
    }

    pub fn path_to_config_file(&self) -> RelativePathBuf {
        self.path_to_containing_dir.join(&self.file_name)
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
pub struct TestCaseWithExpectations {
    pub test_case: TestCase,
    pub expectations: TestCaseExpectations,
}
