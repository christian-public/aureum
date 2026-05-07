use crate::test_case::{TestCase, TestCaseExpectations, TestCaseWithExpectations};
use crate::test_result::{TestResult, ValueComparison};
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
pub struct RunResult {
    pub test_case: TestCase,
    pub expectations: TestCaseExpectations,
    pub result: Result<TestResult, RunError>,
}

impl RunResult {
    pub fn is_success(&self) -> bool {
        match &self.result {
            Ok(test_result) => test_result.is_success(),
            Err(_) => false,
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
    test_cases: &[TestCaseWithExpectations],
    run_in_parallel: bool,
    current_dir: &Path,
    report_test_case: &(impl Fn(usize, &TestCase, &Result<TestResult, RunError>) + Sync),
) -> Vec<RunResult> {
    let run = |(i, entry): (usize, &TestCaseWithExpectations)| -> RunResult {
        let result = run_test_case(entry, current_dir);

        report_test_case(i, &entry.test_case, &result);

        RunResult {
            test_case: entry.test_case.clone(),
            expectations: entry.expectations.clone(),
            result,
        }
    };

    if run_in_parallel {
        test_cases.par_iter().enumerate().map(run).collect()
    } else {
        test_cases.iter().enumerate().map(run).collect()
    }
}

pub fn run_test_case(
    entry: &TestCaseWithExpectations,
    current_dir: &Path,
) -> Result<TestResult, RunError> {
    let output = run_program(&entry.test_case, current_dir)?;

    Ok(TestResult {
        stdout: compare_result(entry.expectations.stdout.clone(), output.stdout),
        stderr: compare_result(entry.expectations.stderr.clone(), output.stderr),
        exit_code: compare_result(entry.expectations.exit_code, output.exit_code),
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

    // If a timeout is configured, start a killer thread.  It waits on a channel
    // for the process to finish; if the timeout elapses first it kills the
    // process and returns true.  This way the thread exits promptly when the
    // process completes naturally rather than sleeping for the full duration.
    let killer_join = test_case.timeout_seconds.map(|timeout_secs| {
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let handle = thread::spawn(move || -> bool {
            let timed_out = done_rx.recv_timeout(Duration::from_secs(timeout_secs))
                == Err(mpsc::RecvTimeoutError::Timeout);
            if timed_out {
                kill_timed_out_process(child_id);
            }
            timed_out
        });
        (handle, done_tx)
    });

    let status = child.wait().map_err(RunError::IOError)?;

    let timed_out = if let Some((handle, done_tx)) = killer_join {
        let _ = done_tx.send(()); // signal the killer thread to exit
        handle.join().expect("killer thread panicked")
    } else {
        false
    };
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
    let run_dir = test_case.path_to_containing_dir.to_path(current_dir);

    let mut cmd = Command::new(&test_case.program_path);

    cmd.current_dir(run_dir);
    cmd.args(&test_case.arguments);

    cmd
}

fn compare_result<T: Eq>(expected: Option<T>, got: T) -> ValueComparison<T> {
    if let Some(expected) = expected {
        if expected == got {
            ValueComparison::Matches(got)
        } else {
            ValueComparison::Diff { expected, got }
        }
    } else {
        ValueComparison::NotChecked(got)
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
