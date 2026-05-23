use crate::scratch::ScratchPlan;
use crate::test_case::{PlannedTestCase, TestCase, TestCaseExpectations};
use crate::test_id::TestId;
use crate::test_outcome::{FieldOutcome, TestOutcome};
use crate::utils::string;
use rayon::prelude::*;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ProgramOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

// `Ran` is much larger than `Skipped` (it carries a `TestCase` by value).
#[allow(clippy::large_enum_variant)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum RunResult {
    Skipped {
        id: TestId,
        reason: String,
    },
    Ran {
        test_case: TestCase,
        result: Result<TestOutcome, RunError>,
    },
}

#[cfg_attr(debug_assertions, derive(Debug))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RunResultKind {
    Skipped,
    Passed,
    Failed,
}

impl RunResult {
    pub fn is_success(&self) -> bool {
        match self {
            RunResult::Skipped { .. } => true,
            RunResult::Ran { result, .. } => match result {
                Ok(test_outcome) => test_outcome.is_success(),
                Err(_) => false,
            },
        }
    }

    pub fn kind(&self) -> RunResultKind {
        match self {
            RunResult::Skipped { .. } => RunResultKind::Skipped,
            RunResult::Ran { result, .. } => match result {
                Ok(outcome) if outcome.is_success() => RunResultKind::Passed,
                _ => RunResultKind::Failed,
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("failed to decode UTF-8: {0}")]
    FailedToDecodeUtf8(#[from] std::string::FromUtf8Error),
    #[error("program terminated by signal")]
    ProgramTerminated,
    #[error("I/O error: {0}")]
    IOError(#[from] io::Error),
    #[error("timed out")]
    TimedOut,
    #[error("failed to create scratch directory `{path}`: {source}")]
    ScratchCreationFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write embed `{path}`: {source}")]
    EmbedWriteFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to copy `{from}` to `{to}`: {source}")]
    FileCopyFailed {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: io::Error,
    },
}

// RUN TEST CASES

pub fn run_test_cases(
    test_cases: &[PlannedTestCase],
    run_in_parallel: bool,
    current_dir: &Path,
    report_test_case: &(impl Fn(usize, &RunResult) + Sync),
) -> Vec<RunResult> {
    let run = |(i, test_case): (usize, &PlannedTestCase)| -> RunResult {
        let run_result = match test_case {
            PlannedTestCase::Skip { id, reason } => RunResult::Skipped {
                id: id.clone(),
                reason: reason.clone(),
            },
            PlannedTestCase::Run {
                test_case,
                expectations,
            } => RunResult::Ran {
                test_case: test_case.clone(),
                result: run_test_case(test_case, expectations, current_dir),
            },
        };
        report_test_case(i, &run_result);
        run_result
    };

    if run_in_parallel {
        test_cases.par_iter().enumerate().map(run).collect()
    } else {
        test_cases.iter().enumerate().map(run).collect()
    }
}

fn run_test_case(
    test_case: &TestCase,
    expectations: &TestCaseExpectations,
    current_dir: &Path,
) -> Result<TestOutcome, RunError> {
    let output = run_program(test_case, current_dir)?;

    Ok(TestOutcome {
        stdout: compare_result(expectations.stdout.clone(), output.stdout),
        stderr: compare_result(expectations.stderr.clone(), output.stderr),
        exit_code: compare_result(expectations.exit_code, output.exit_code),
    })
}

/// Runs `test_case`'s program and returns its captured stdout, stderr, and
/// exit code.
///
/// **Side effect:** when `test_case.scratch_plan` is `Some`, this materialises
/// the plan to disk (creates the scratch directory, copies declared input
/// files, writes embed files) before launching the program. Callers that hold
/// a `TestCase` with a scratch plan must be prepared for filesystem writes
/// under `plan.dir`.
pub fn run_program(test_case: &TestCase, current_dir: &Path) -> Result<ProgramOutput, RunError> {
    if let Some(plan) = &test_case.scratch_plan {
        materialise_scratch(plan)?;
    }
    let mut cmd = init_command(test_case, current_dir);

    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    // Spawn the child in its own process group so that on timeout we can kill
    // the entire group (child + any grandchildren it spawned).
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = cmd.spawn().map_err(RunError::IOError)?;
    let child_id = child.id();

    // Write stdin from a separate thread so a child that produces output before
    // consuming all of stdin can't deadlock us against a full pipe buffer.
    let stdin_thread = match test_case.stdin.clone() {
        Some(stdin_string) => {
            let mut stdin = child
                .stdin
                .take()
                .expect("Stdin should be configured to pipe");
            Some(thread::spawn(move || -> io::Result<()> {
                stdin.write_all(stdin_string.as_bytes())
            }))
        }
        None => {
            // Close the write end so the child sees EOF rather than blocking on input.
            drop(child.stdin.take());
            None
        }
    };

    // Read stdout and stderr from separate threads so that neither pipe can fill
    // up and deadlock the child while we wait for it to exit.
    let stdout_thread = {
        let mut pipe = child.stdout.take().expect("stdout should be piped");
        thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut buf = Vec::new();
            pipe.read_to_end(&mut buf)?;
            Ok(buf)
        })
    };
    let stderr_thread = {
        let mut pipe = child.stderr.take().expect("stderr should be piped");
        thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut buf = Vec::new();
            pipe.read_to_end(&mut buf)?;
            Ok(buf)
        })
    };

    // Start a killer thread. It waits on a channel for the process to finish;
    // if the timeout elapses first it kills the process and returns true.
    // This way the thread exits promptly when the process completes naturally
    // rather than sleeping for the full duration.
    let timeout_secs = test_case.timeout_seconds;
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let killer_handle = thread::spawn(move || -> bool {
        let timed_out = done_rx.recv_timeout(Duration::from_secs(timeout_secs))
            == Err(mpsc::RecvTimeoutError::Timeout);
        if timed_out {
            kill_timed_out_process(child_id);
        }
        timed_out
    });

    let status = child.wait().map_err(RunError::IOError)?;

    let _ = done_tx.send(()); // signal the killer thread to exit
    let timed_out = killer_handle.join().expect("killer thread panicked");
    if timed_out {
        return Err(RunError::TimedOut);
    }

    if let Some(handle) = stdin_thread {
        let result = handle.join().expect("stdin writer thread panicked");
        // BrokenPipe is expected when the child exits before consuming all input.
        if let Err(e) = result
            && e.kind() != io::ErrorKind::BrokenPipe
        {
            return Err(RunError::IOError(e));
        }
    }

    let stdout_bytes = stdout_thread
        .join()
        .expect("stdout reader thread panicked")
        .map_err(RunError::IOError)?;
    let stderr_bytes = stderr_thread
        .join()
        .expect("stderr reader thread panicked")
        .map_err(RunError::IOError)?;

    let stdout = String::from_utf8(stdout_bytes).map_err(RunError::FailedToDecodeUtf8)?;
    let stderr = String::from_utf8(stderr_bytes).map_err(RunError::FailedToDecodeUtf8)?;
    let exit_code = status.code().ok_or(RunError::ProgramTerminated)?;

    Ok(ProgramOutput {
        stdout: string::normalize_newlines(&stdout),
        stderr: string::normalize_newlines(&stderr),
        exit_code,
    })
}

/// Runs `test_case`'s program with stdout/stderr inherited from the parent
/// process and returns its exit code.
///
/// **Side effect:** see [`run_program`] — when `test_case.scratch_plan` is
/// `Some`, this materialises the plan to disk before launching the program.
pub fn run_program_passthrough(test_case: &TestCase, current_dir: &Path) -> Result<i32, RunError> {
    if let Some(plan) = &test_case.scratch_plan {
        materialise_scratch(plan)?;
    }
    let mut cmd = init_command(test_case, current_dir);

    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    if test_case.stdin.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::inherit());
    }

    let mut child = cmd.spawn().map_err(RunError::IOError)?;

    if let Some(stdin_string) = &test_case.stdin {
        let mut stdin = child
            .stdin
            .take()
            .expect("Stdin should be configured to pipe");
        stdin
            .write_all(stdin_string.as_bytes())
            .map_err(RunError::IOError)?;
    }

    let exit_status = child.wait().map_err(RunError::IOError)?;
    exit_status.code().ok_or(RunError::ProgramTerminated)
}

// HELPER FUNCTIONS

fn init_command(test_case: &TestCase, current_dir: &Path) -> Command {
    let run_dir = match &test_case.scratch_plan {
        Some(plan) => plan.dir.clone(),
        None => test_case.id.config_dir_path.to_path(current_dir),
    };

    let mut cmd = Command::new(&test_case.program_path);

    cmd.current_dir(run_dir);
    cmd.args(&test_case.arguments);

    cmd
}

fn compare_result<T: Eq>(expected: Option<T>, got: T) -> FieldOutcome<T> {
    if let Some(expected) = expected {
        if expected == got {
            FieldOutcome::Matches(got)
        } else {
            FieldOutcome::Diff { expected, got }
        }
    } else {
        FieldOutcome::NotChecked(got)
    }
}

fn materialise_scratch(plan: &ScratchPlan) -> Result<(), RunError> {
    fs::create_dir_all(&plan.dir).map_err(|source| RunError::ScratchCreationFailed {
        path: plan.dir.clone(),
        source,
    })?;
    for embed in &plan.embeds {
        let dest = plan.dir.join(&embed.dest_relative);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|source| RunError::ScratchCreationFailed {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(&dest, &embed.content)
            .map_err(|source| RunError::EmbedWriteFailed { path: dest, source })?;
    }
    for copy in &plan.copies {
        let dest = plan.dir.join(&copy.dest_relative);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|source| RunError::ScratchCreationFailed {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::copy(&copy.source, &dest).map_err(|source| RunError::FileCopyFailed {
            from: copy.source.clone(),
            to: dest,
            source,
        })?;
    }
    Ok(())
}

// KILL HELPERS

#[cfg(unix)]
fn kill_timed_out_process(child_id: u32) {
    // The child was spawned with process_group(0), so its pgid == its own pid.
    // killpg kills the entire group, including any grandchildren.
    unsafe {
        libc::killpg(child_id as libc::pid_t, libc::SIGKILL);
    }
}

#[cfg(windows)]
fn kill_timed_out_process(child_id: u32) {
    // taskkill /T kills the process tree (child + all descendants).
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &child_id.to_string()])
        .output();
}

#[cfg(not(any(unix, windows)))]
fn kill_timed_out_process(_child_id: u32) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::{EmbedWrite, FileCopy};

    fn plan_in(dir: PathBuf) -> ScratchPlan {
        ScratchPlan {
            dir,
            copies: vec![],
            embeds: vec![],
        }
    }

    #[test]
    fn materialise_creates_scratch_dir() {
        let parent = tempfile::TempDir::new().unwrap();
        let scratch = parent.path().join("nested/0001-test");
        let plan = plan_in(scratch.clone());
        materialise_scratch(&plan).unwrap();
        assert!(scratch.is_dir());
    }

    #[test]
    fn materialise_writes_embed_at_root() {
        let scratch = tempfile::TempDir::new().unwrap();
        let plan = ScratchPlan {
            dir: scratch.path().to_path_buf(),
            copies: vec![],
            embeds: vec![EmbedWrite {
                dest_relative: "inline.txt".to_owned(),
                content: "hello embed".to_owned(),
            }],
        };
        materialise_scratch(&plan).unwrap();
        assert_eq!(
            fs::read_to_string(scratch.path().join("inline.txt")).unwrap(),
            "hello embed"
        );
    }

    #[test]
    fn materialise_writes_embed_with_nested_parent() {
        let scratch = tempfile::TempDir::new().unwrap();
        let plan = ScratchPlan {
            dir: scratch.path().to_path_buf(),
            copies: vec![],
            embeds: vec![EmbedWrite {
                dest_relative: "sub/dir/inline.txt".to_owned(),
                content: "nested embed".to_owned(),
            }],
        };
        materialise_scratch(&plan).unwrap();
        let dest = scratch.path().join("sub/dir/inline.txt");
        assert!(dest.parent().unwrap().is_dir());
        assert_eq!(fs::read_to_string(dest).unwrap(), "nested embed");
    }

    #[test]
    fn materialise_copies_file_at_root() {
        let source_dir = tempfile::TempDir::new().unwrap();
        let source = source_dir.path().join("src.txt");
        fs::write(&source, "src body").unwrap();
        let scratch = tempfile::TempDir::new().unwrap();
        let plan = ScratchPlan {
            dir: scratch.path().to_path_buf(),
            copies: vec![FileCopy {
                source: source.clone(),
                dest_relative: "src.txt".to_owned(),
            }],
            embeds: vec![],
        };
        materialise_scratch(&plan).unwrap();
        assert_eq!(
            fs::read_to_string(scratch.path().join("src.txt")).unwrap(),
            "src body"
        );
    }

    #[test]
    fn materialise_copies_file_with_nested_parent() {
        let source_dir = tempfile::TempDir::new().unwrap();
        let source = source_dir.path().join("src.txt");
        fs::write(&source, "deep").unwrap();
        let scratch = tempfile::TempDir::new().unwrap();
        let plan = ScratchPlan {
            dir: scratch.path().to_path_buf(),
            copies: vec![FileCopy {
                source: source.clone(),
                dest_relative: "a/b/c.txt".to_owned(),
            }],
            embeds: vec![],
        };
        materialise_scratch(&plan).unwrap();
        assert_eq!(
            fs::read_to_string(scratch.path().join("a/b/c.txt")).unwrap(),
            "deep"
        );
    }

    #[test]
    fn materialise_is_idempotent_and_overwrites_embeds() {
        let scratch = tempfile::TempDir::new().unwrap();
        let plan = ScratchPlan {
            dir: scratch.path().to_path_buf(),
            copies: vec![],
            embeds: vec![EmbedWrite {
                dest_relative: "inline.txt".to_owned(),
                content: "first".to_owned(),
            }],
        };
        materialise_scratch(&plan).unwrap();
        let plan2 = ScratchPlan {
            embeds: vec![EmbedWrite {
                dest_relative: "inline.txt".to_owned(),
                content: "second".to_owned(),
            }],
            ..plan
        };
        materialise_scratch(&plan2).unwrap();
        assert_eq!(
            fs::read_to_string(scratch.path().join("inline.txt")).unwrap(),
            "second",
            "re-materialising should overwrite embeds"
        );
    }

    #[test]
    fn materialise_errors_when_copy_source_disappears() {
        let source_dir = tempfile::TempDir::new().unwrap();
        let source = source_dir.path().join("vanishing.txt");
        // Note: never created.
        let scratch = tempfile::TempDir::new().unwrap();
        let plan = ScratchPlan {
            dir: scratch.path().to_path_buf(),
            copies: vec![FileCopy {
                source: source.clone(),
                dest_relative: "x.txt".to_owned(),
            }],
            embeds: vec![],
        };
        let err = materialise_scratch(&plan).unwrap_err();
        match err {
            RunError::FileCopyFailed { from, .. } => assert_eq!(from, source),
            other => panic!("expected FileCopyFailed, got {other:?}"),
        }
    }
}
