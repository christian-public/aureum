use crate::test_id::TestId;
use relative_path::RelativePathBuf;
use std::path::PathBuf;

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestCase {
    pub source_file: RelativePathBuf,
    pub test_id: TestId,
    pub description: Option<String>,
    pub program: PathBuf, // Expects an absolute path
    pub arguments: Vec<String>,
    pub stdin: Option<String>,
    pub expected_stdout: Option<String>,
    pub expected_stderr: Option<String>,
    pub expected_exit_code: Option<i32>,
}

impl TestCase {
    pub fn id(&self) -> String {
        let file_path = self.source_file.to_string();

        if self.test_id.is_root() {
            file_path
        } else {
            format!("{}:{}", file_path, self.test_id)
        }
    }
}
