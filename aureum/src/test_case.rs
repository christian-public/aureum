use crate::test_id::TestId;
use relative_path::RelativePathBuf;
use std::path::PathBuf;

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestCase {
    pub path_to_containing_dir: RelativePathBuf,
    pub file_name: String,
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
        let path = self.path_to_config_file();

        if self.test_id.is_root() {
            path.to_string()
        } else {
            format!("{}:{}", path, self.test_id)
        }
    }

    pub fn path_to_config_file(&self) -> RelativePathBuf {
        self.path_to_containing_dir.join(&self.file_name)
    }
}
