use crate::subtest_path::SubtestPath;
use relative_path::RelativePathBuf;
use std::fmt;

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestId {
    pub config_dir_path: RelativePathBuf,
    pub file_name: String,
    pub subtest_path: SubtestPath,
}

impl TestId {
    pub fn new(
        config_dir_path: RelativePathBuf,
        file_name: String,
        subtest_path: SubtestPath,
    ) -> Self {
        Self {
            config_dir_path,
            file_name,
            subtest_path,
        }
    }

    pub fn config_file_path(&self) -> RelativePathBuf {
        self.config_dir_path.join(&self.file_name)
    }

    /// Formats the ID as `path` (root test) or `path:subtest_path` (subtest).
    pub fn display_id(&self) -> String {
        if self.subtest_path.is_root() {
            self.config_file_path().to_string()
        } else {
            format!("{}:{}", self.config_file_path(), self.subtest_path)
        }
    }
}

impl fmt::Display for TestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_id())
    }
}
