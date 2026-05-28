use crate::args::{ScratchArgs, ScratchMode};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Owns the scratch root for the duration of a CLI run.
///
/// Cleanup on drop depends on the variant:
///
/// - `Disabled`: nothing to clean.
/// - `User { keep: false }`: the root directory is **never** deleted (it was
///   supplied by the user, so we never touch their files), but any per-test
///   subdirectories aureum created inside it (`aureum-0001--…`, …) are
///   removed. Other files in the root are preserved.
/// - `User { keep: true }`: nothing is cleaned. Root and per-test subdirs
///   both stay, supporting iterative inspection.
/// - `Temp`: the whole system-temp directory is removed by `TempDir`'s own
///   `Drop`.
pub enum ScratchSession {
    /// Isolation disabled (`--scratch in-place`).
    Disabled,
    /// User-supplied directory.
    User {
        path: PathBuf,
        /// When `true`, per-test subdirs are preserved on drop (`--keep-scratch`).
        keep: bool,
    },
    /// System-temp directory we created; cleaned up on drop. `--keep-scratch`
    /// can't apply here: clap requires it to be paired with `--scratch-root`,
    /// so a temp root is never kept.
    Temp(TempDir),
}

impl ScratchSession {
    pub fn create(args: &ScratchArgs) -> io::Result<Self> {
        if args.scratch == ScratchMode::InPlace {
            return Ok(Self::Disabled);
        }
        if let Some(path) = &args.scratch_root {
            fs::create_dir_all(path)?;
            return Ok(Self::User {
                path: path.clone(),
                keep: args.keep_scratch,
            });
        }
        // No `keep_scratch` branch: clap's `requires = "scratch_root"` means a
        // temp root is reached only when `keep` is false, so it always cleans
        // up on drop.
        let temp = tempfile::Builder::new().prefix("aureum-").tempdir()?;
        Ok(Self::Temp(temp))
    }

    /// Returns the scratch root path, or `None` if isolation is disabled.
    pub fn root(&self) -> Option<&Path> {
        match self {
            Self::Disabled => None,
            Self::User { path, .. } => Some(path),
            Self::Temp(temp) => Some(temp.path()),
        }
    }

    /// Wipe any per-test scratch subdirs left under the scratch root. Called
    /// before each test-running pass to give every pass a clean slate — fixes
    /// stale state between watch iterations and sweeps orphans left by tests
    /// that have been renamed, removed, or reindexed since the previous pass.
    /// Also sweeps leftovers from a crashed prior process when the user reuses
    /// a `--scratch-root`. Non-aureum files in the root are preserved.
    ///
    /// No-op when isolation is disabled.
    pub fn prepare_for_run(&self) {
        if let Some(root) = self.root() {
            remove_per_test_subdirs(root);
        }
    }
}

impl Drop for ScratchSession {
    fn drop(&mut self) {
        if let Self::User { path, keep: false } = self {
            remove_per_test_subdirs(path);
        }
    }
}

/// Wipe any per-test scratch subdirs left under `root`. Convenience wrapper
/// around [`ScratchSession::prepare_for_run`] for callers that only hold the
/// scratch root path (not the session itself). No-op when `root` is `None`
/// (isolation disabled).
pub fn clean_per_test_subdirs(root: Option<&Path>) {
    if let Some(root) = root {
        remove_per_test_subdirs(root);
    }
}

/// Walk `root` non-recursively and remove every entry whose name matches the
/// per-test directory pattern. Other entries (the user's own files) are left
/// untouched. Errors are intentionally swallowed: cleanup is best-effort, and
/// the user's data has already been the test target — failing here can't
/// undo what just ran, and panicking from `Drop` would hide test results.
fn remove_per_test_subdirs(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if let Some(name_str) = name.to_str()
            && aureum::is_per_test_dir_name(name_str)
        {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_session(path: PathBuf, keep: bool) -> ScratchSession {
        ScratchSession::User { path, keep }
    }

    fn make_per_test_dir(root: &Path, name: &str) -> PathBuf {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("marker.txt"), "x").unwrap();
        dir
    }

    #[test]
    fn disabled_has_no_root() {
        let session = ScratchSession::Disabled;
        assert!(session.root().is_none());
    }

    #[test]
    fn temp_cleans_up_on_drop() {
        let path = {
            let temp = tempfile::Builder::new()
                .prefix("aureum-test-")
                .tempdir()
                .unwrap();
            let path = temp.path().to_path_buf();
            let session = ScratchSession::Temp(temp);
            assert_eq!(session.root(), Some(path.as_path()));
            path
            // session drops here
        };
        assert!(!path.exists(), "Temp variant should clean up on drop");
    }

    #[test]
    fn user_creates_missing_dir_via_create() {
        let parent = tempfile::TempDir::new().unwrap();
        let missing = parent.path().join("not-yet-there");
        let args = ScratchArgs {
            scratch: ScratchMode::PerTest,
            scratch_root: Some(missing.clone()),
            keep_scratch: true, // preserve so the assertion below makes sense
        };
        let _session = ScratchSession::create(&args).unwrap();
        assert!(missing.exists());
    }

    #[test]
    fn user_never_deletes_root_on_drop() {
        let root = tempfile::TempDir::new().unwrap();
        let root_path = root.path().to_path_buf();
        {
            let _session = user_session(root_path.clone(), false);
        }
        assert!(
            root_path.exists(),
            "User variant must never delete the root"
        );
    }

    #[test]
    fn user_cleans_per_test_subdirs_when_keep_false() {
        let root = tempfile::TempDir::new().unwrap();
        let per_test_a = make_per_test_dir(root.path(), "aureum-0001--test_a");
        let per_test_b = make_per_test_dir(root.path(), "aureum-0042--test_b");
        {
            let _session = user_session(root.path().to_path_buf(), false);
        }
        assert!(!per_test_a.exists());
        assert!(!per_test_b.exists());
    }

    #[test]
    fn user_preserves_per_test_subdirs_when_keep_true() {
        let root = tempfile::TempDir::new().unwrap();
        let per_test = make_per_test_dir(root.path(), "aureum-0001--test");
        {
            let _session = user_session(root.path().to_path_buf(), true);
        }
        assert!(per_test.exists());
        assert!(per_test.join("marker.txt").exists());
    }

    #[test]
    fn prepare_for_run_wipes_per_test_subdirs_when_keep_true() {
        // Pre-pass cleanup runs regardless of `keep`. `keep` only controls
        // what happens at session drop, not between passes — without this,
        // watch iterations would accumulate stale state under `--keep-scratch`.
        let root = tempfile::TempDir::new().unwrap();
        let per_test = make_per_test_dir(root.path(), "aureum-0001--stale");
        let session = user_session(root.path().to_path_buf(), true);

        session.prepare_for_run();

        assert!(!per_test.exists());
    }

    #[test]
    fn prepare_for_run_preserves_user_files() {
        let root = tempfile::TempDir::new().unwrap();
        let user_file = root.path().join("user-data.txt");
        fs::write(&user_file, "keep me").unwrap();
        let per_test = make_per_test_dir(root.path(), "aureum-0001--victim");
        let session = user_session(root.path().to_path_buf(), false);

        session.prepare_for_run();

        assert!(!per_test.exists());
        assert!(user_file.exists());
    }

    #[test]
    fn prepare_for_run_is_noop_when_disabled() {
        // Disabled has no root; cleanup must not panic or touch anything.
        let session = ScratchSession::Disabled;
        session.prepare_for_run();
    }

    #[test]
    fn clean_per_test_subdirs_none_is_noop() {
        clean_per_test_subdirs(None);
    }

    #[test]
    fn user_preserves_non_per_test_files_when_keep_false() {
        let root = tempfile::TempDir::new().unwrap();
        let user_file = root.path().join("user-data.txt");
        let user_dir = root.path().join("user-dir");
        let nested = user_dir.join("inside.txt");
        fs::write(&user_file, "important").unwrap();
        fs::create_dir(&user_dir).unwrap();
        fs::write(&nested, "also important").unwrap();
        // A per-test subdir to confirm cleanup still runs.
        let per_test = make_per_test_dir(root.path(), "aureum-0001--victim");
        let lookalike_single_dash = root.path().join("aureum-0001-user-data");
        fs::create_dir(&lookalike_single_dash).unwrap();
        fs::write(lookalike_single_dash.join("inside.txt"), "user").unwrap();
        let lookalike = root.path().join("0001-user-data");
        fs::create_dir(&lookalike).unwrap();
        fs::write(lookalike.join("inside.txt"), "user").unwrap();
        {
            let _session = user_session(root.path().to_path_buf(), false);
        }
        assert!(!per_test.exists(), "per-test subdir should be cleaned");
        assert!(user_file.exists(), "user file must survive");
        assert!(user_dir.exists(), "user dir must survive");
        assert!(nested.exists(), "user dir contents must survive");
        assert!(
            lookalike_single_dash.exists(),
            "user dir with `aureum-NNNN-...` single-dash name must survive"
        );
        assert!(
            lookalike.exists(),
            "user dir with `NNNN-...` name (no `aureum-` prefix) must survive"
        );
    }
}
