use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub(crate) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub(crate) fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("aureum_test_{name}_{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    pub(crate) fn write(&self, name: &str, content: &str) {
        fs::write(self.path.join(name), content).unwrap();
    }

    pub(crate) fn read(&self, name: &str) -> String {
        fs::read_to_string(self.path.join(name)).unwrap()
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub(crate) fn make_test_case_root(dir: &str, file: &str) -> aureum::TestCase {
    use aureum::{SubtestPath, TestId};
    use relative_path::RelativePathBuf;
    aureum::TestCase {
        id: TestId::new(
            RelativePathBuf::from(dir),
            file.to_string(),
            SubtestPath::root(),
        ),
        program_path: PathBuf::from("/bin/echo"),
        arguments: vec![],
        stdin: None,
        timeout_seconds: u64::MAX,
        scratch_plan: None,
    }
}
