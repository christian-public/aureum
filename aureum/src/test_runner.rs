use crate::test_case::{PlannedTestCase, TestCase, TestCaseExpectations};
use crate::test_case_id::TestCaseId;
use crate::test_outcome::{FieldOutcome, TestOutcome};
use crate::utils::string;
use rayon::prelude::*;
use std::io::{self, Read, Write};
use std::path::Path;
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

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum RunResult {
    Skipped {
        id: TestCaseId,
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

pub fn run_program(test_case: &TestCase, current_dir: &Path) -> Result<ProgramOutput, RunError> {
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

pub fn run_program_passthrough(test_case: &TestCase, current_dir: &Path) -> Result<i32, RunError> {
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
    let run_dir = test_case.id.config_dir_path.to_path(current_dir);

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
