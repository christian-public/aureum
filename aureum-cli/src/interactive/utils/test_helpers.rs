use std::fs;
use std::path::Path;
use std::path::PathBuf;

/// Thin test-only wrapper around `tempfile::TempDir` that adds `write`/`read`
/// sugar so test bodies don't have to repeat `fs::write(td.path().join(...))`.
/// Cleanup happens automatically when dropped (inherited from `tempfile`).
pub(crate) struct TempDir {
    inner: tempfile::TempDir,
}

impl TempDir {
    pub(crate) fn new(prefix: &str) -> Self {
        let inner = tempfile::Builder::new()
            .prefix(&format!("aureum_test_{prefix}_"))
            .tempdir()
            .unwrap();
        Self { inner }
    }

    pub(crate) fn write(&self, name: &str, content: &str) {
        fs::write(self.inner.path().join(name), content).unwrap();
    }

    pub(crate) fn read(&self, name: &str) -> String {
        fs::read_to_string(self.inner.path().join(name)).unwrap()
    }

    pub(crate) fn path(&self) -> &Path {
        self.inner.path()
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
