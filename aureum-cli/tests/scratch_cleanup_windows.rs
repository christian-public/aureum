//! End-to-end coverage for the Windows PowerShell rerun script left by
//! `--keep-scratch`. The Unix counterpart lives in `scratch_cleanup.rs`
//! (`#![cfg(unix)]`); this file is the `.ps1` analogue and never touches that
//! one. It runs a test under `--keep-scratch`, then executes the left-behind
//! `aureum-rerun.ps1` through PowerShell and checks it reproduces the original
//! invocation.
//!
//! Windows-only. The inner program is `cmd /c exit 3`, so the assertion is
//! exit-code forwarding (`exit $LASTEXITCODE`) — deterministic, with no
//! newline/encoding matching. Arguments deliberately contain no spaces or
//! quotes: Windows PowerShell 5.1 (what `powershell.exe` is) can mis-marshal
//! those to native programs, which is a documented limitation of the `.ps1`
//! artifact, not a behaviour this test asserts.

#![cfg(windows)]

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_aureum");

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
fn keep_scratch_leaves_a_runnable_ps1_rerun_script() {
    let scratch_root = tempfile::TempDir::new().unwrap();
    let config_dir = tempfile::TempDir::new().unwrap();
    let config_path = config_dir.path().join("exit.au.toml");
    // `cmd /c exit 3`: the rerun must forward this exit code. Splitting `exit`
    // and `3` into separate args keeps every token space-free.
    fs::write(
        &config_path,
        "program = \"cmd\"\nprogram_arguments = [\"/c\", \"exit\", \"3\"]\nexpected_exit_code = 3\n",
    )
    .unwrap();

    // Run from the config dir and pass the config by name. aureum stores config
    // paths relative to its cwd (via `pathdiff::diff_paths`), and on Windows a
    // temp file on C: has no relative form against a checkout on D: — cross-drive
    // paths can't be relativised, so an absolute temp path is rejected as
    // "invalid". A bare name takes the `is_relative()` branch and avoids the
    // diff entirely. (`scratch_root` may stay absolute; it is never relativised.)
    let output = Command::new(BIN)
        .current_dir(config_dir.path())
        .args(["test", "--scratch-root"])
        .arg(scratch_root.path())
        .arg("--keep-scratch")
        .arg("exit.au.toml")
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
        "--keep-scratch should leave a rerun script ({})",
        aureum::RERUN_SCRIPT_NAME,
    );

    // Execute the left-behind .ps1 and confirm PowerShell parses it, runs the
    // program, and forwards the child's exit code (3) via `exit $LASTEXITCODE`.
    // A broken script would exit 0 or a PowerShell error code (1), not 3.
    let rerun = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .output()
        .expect("failed to execute rerun script via powershell");
    assert_eq!(
        rerun.status.code(),
        Some(3),
        "rerun script should forward the child's exit code 3\nstdout:\n{stdout}\nstderr:\n{stderr}",
        stdout = String::from_utf8_lossy(&rerun.stdout),
        stderr = String::from_utf8_lossy(&rerun.stderr),
    );
}
