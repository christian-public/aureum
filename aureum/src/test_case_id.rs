use crate::test_id::TestId;
use relative_path::RelativePathBuf;
use std::fmt;

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestCaseId {
    pub config_dir_path: RelativePathBuf,
    pub file_name: String,
    pub test_id: TestId,
}

impl TestCaseId {
    pub fn new(config_dir_path: RelativePathBuf, file_name: String, test_id: TestId) -> Self {
        Self {
            config_dir_path,
            file_name,
            test_id,
        }
    }

    pub fn config_file_path(&self) -> RelativePathBuf {
        self.config_dir_path.join(&self.file_name)
    }

    /// Formats the ID as `path` (root test) or `path:test_id` (subtest).
    pub fn display_id(&self) -> String {
        if self.test_id.is_root() {
            self.config_file_path().to_string()
        } else {
            format!("{}:{}", self.config_file_path(), self.test_id)
        }
    }
}

impl fmt::Display for TestCaseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_id())
    }
}
