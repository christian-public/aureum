//! End-to-end coverage for the `--scratch-root` / `--keep-scratch` wiring:
//! verifies that CLI flags actually produce the expected on-disk cleanup
//! behavior. The unit tests in `scratch_session.rs` test `ScratchSession`'s
//! `Drop` contract directly — these tests close the gap between *that*
//! contract and what happens when the user invokes `aureum test` with the
//! real flags.
//!
//! Unix-only: the inner config uses `true` from PATH, which isn't available
//! on Windows by default. The rest of the golden suite is Unix-only for the
//! same reason.

#![cfg(unix)]

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_aureum");

/// Minimal config that runs `/usr/bin/true` and asserts nothing. Triggers
/// `materialise_scratch` (which always creates the per-test dir even with no
/// inputs to copy) so each run produces exactly one `aureum-NNNN--…` subdir.
const MINIMAL_CONFIG: &str = "program = \"true\"\nexpected_exit_code = 0\n";

fn per_test_subdirs(root: &Path) -> Vec<OsString> {
    fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(aureum::is_per_test_dir_name)
        })
        .map(|entry| entry.file_name())
        .collect()
}

/// Capture stdout/stderr so the subprocess doesn't pollute `cargo test`
/// output. The captured bytes are surfaced in assertion failures via
/// `assert_success`.
fn run_aureum_test(scratch_root: &Path, extra_args: &[&str]) -> Output {
    let config_dir = tempfile::TempDir::new().unwrap();
    let config_path = config_dir.path().join("test.au.toml");
    fs::write(&config_path, MINIMAL_CONFIG).unwrap();

    Command::new(BIN)
        .arg("test")
        .arg("--scratch-root")
        .arg(scratch_root)
        .args(extra_args)
        .arg(&config_path)
        .output()
        .expect("failed to spawn aureum binary")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "aureum exited non-zero: {status:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        status = output.status,
        stdout = String::from_utf8_lossy(&output.stdout),
        stderr = String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn scratch_root_per_test_subdirs_are_cleaned_after_run() {
    let scratch_root = tempfile::TempDir::new().unwrap();
    let user_file = scratch_root.path().join("untouched.txt");
    fs::write(&user_file, "user data").unwrap();

    let output = run_aureum_test(scratch_root.path(), &[]);
    assert_success(&output);

    assert!(
        scratch_root.path().exists(),
        "the user-supplied root must never be deleted"
    );
    assert!(
        user_file.exists(),
        "files the user kept in the root must survive cleanup"
    );
    let remaining = per_test_subdirs(scratch_root.path());
    assert!(
        remaining.is_empty(),
        "per-test subdirs should have been cleaned up, but found: {remaining:?}"
    );
}

#[test]
fn keep_scratch_preserves_per_test_subdirs() {
    let scratch_root = tempfile::TempDir::new().unwrap();
    let output = run_aureum_test(scratch_root.path(), &["--keep-scratch"]);
    assert_success(&output);

    let preserved = per_test_subdirs(scratch_root.path());
    assert_eq!(
        preserved.len(),
        1,
        "expected exactly one per-test subdir under --keep-scratch, got: {preserved:?}"
    );
}

#[test]
fn pre_pass_cleanup_sweeps_orphans_from_prior_run() {
    // Simulate a crashed prior process (or `--keep-scratch` leftover) by
    // dropping a per-test-shaped subdir into the root before invoking
    // aureum. The pre-pass cleanup should sweep it before this run starts,
    // and the post-pass cleanup should clear this run's own subdirs too.
    let scratch_root = tempfile::TempDir::new().unwrap();
    let orphan = scratch_root.path().join("aureum-9999--orphan");
    fs::create_dir(&orphan).unwrap();
    fs::write(orphan.join("stale.txt"), "leftover").unwrap();

    let output = run_aureum_test(scratch_root.path(), &[]);
    assert_success(&output);

    assert!(
        !orphan.exists(),
        "orphan per-test dir from prior run should have been swept"
    );
    let remaining = per_test_subdirs(scratch_root.path());
    assert!(
        remaining.is_empty(),
        "post-run cleanup should leave nothing behind, got: {remaining:?}"
    );
}

#[test]
fn keep_scratch_still_sweeps_prior_run_orphans() {
    // `--keep-scratch` is about preserving *this* run's output, not about
    // accumulating across runs. The pre-pass cleanup runs regardless, so a
    // stale orphan from a previous session disappears even under
    // `--keep-scratch`; only the current run's subdirs survive.
    let scratch_root = tempfile::TempDir::new().unwrap();
    let orphan = scratch_root.path().join("aureum-9999--orphan");
    fs::create_dir(&orphan).unwrap();

    let output = run_aureum_test(scratch_root.path(), &["--keep-scratch"]);
    assert_success(&output);

    assert!(
        !orphan.exists(),
        "pre-pass cleanup must run even with --keep-scratch"
    );
    let preserved = per_test_subdirs(scratch_root.path());
    assert_eq!(
        preserved.len(),
        1,
        "this run's per-test subdir should be preserved, got: {preserved:?}"
    );
}

#[test]
fn keep_scratch_leaves_a_runnable_rerun_script() {
    let scratch_root = tempfile::TempDir::new().unwrap();
    let config_dir = tempfile::TempDir::new().unwrap();
    let config_path = config_dir.path().join("echo.au.toml");
    // stdin exercises the sidecar path; `cat` echoes it straight back.
    fs::write(
        &config_path,
        "program = \"cat\"\nstdin = \"ping\"\nexpected_stdout = \"ping\"\n",
    )
    .unwrap();

    let output = Command::new(BIN)
        .args(["test", "--scratch-root"])
        .arg(scratch_root.path())
        .arg("--keep-scratch")
        .arg(&config_path)
        .output()
        .expect("failed to spawn aureum binary");
    assert_success(&output);

    let dirs = per_test_subdirs(scratch_root.path());
    assert_eq!(
        dirs.len(),
        1,
        "expected one preserved per-test dir, got {dirs:?}"
    );
    let dir = scratch_root.path().join(&dirs[0]);
    let script = dir.join(aureum::RERUN_SCRIPT_NAME);
    assert!(
        script.exists(),
        "--keep-scratch should leave a rerun script"
    );

    // Re-running the left-behind script must reproduce the original output,
    // stdin sidecar and all — the whole point of the artifact.
    let rerun = Command::new("sh")
        .arg(&script)
        .output()
        .expect("failed to execute rerun script");
    assert!(
        rerun.status.success(),
        "rerun script exited non-zero: {status:?}\nstderr:\n{stderr}",
        status = rerun.status,
        stderr = String::from_utf8_lossy(&rerun.stderr),
    );
    assert_eq!(String::from_utf8_lossy(&rerun.stdout), "ping");
}

#[test]
fn no_rerun_script_without_keep_scratch() {
    // Without `--keep-scratch` the script must not be written at all. The
    // per-test dir is swept after the run, so we can't inspect it afterwards;
    // instead the program probes its own cwd (the scratch dir) for the script
    // while it runs. The script, if written, is created before the program is
    // spawned, so "absent" proves the flag never reached the plan.
    let scratch_root = tempfile::TempDir::new().unwrap();
    let config_dir = tempfile::TempDir::new().unwrap();
    let config_path = config_dir.path().join("probe.au.toml");
    fs::write(
        &config_path,
        "program = \"sh\"\n\
         program_arguments = [\"-c\", \"test -e aureum-rerun.sh && printf present || printf absent\"]\n\
         expected_stdout = \"absent\"\n",
    )
    .unwrap();

    // `--scratch-root` (so a scratch dir exists) but no `--keep-scratch`.
    let output = Command::new(BIN)
        .args(["test", "--scratch-root"])
        .arg(scratch_root.path())
        .arg(&config_path)
        .output()
        .expect("failed to spawn aureum binary");
    assert_success(&output);
}
