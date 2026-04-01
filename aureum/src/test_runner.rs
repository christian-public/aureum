use crate::report::ReportConfig;
use crate::test_case::TestCase;
use crate::test_result::{TestResult, ValueComparison};
use rayon::prelude::*;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ProgramOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct RunResult {
    pub test_case: TestCase,
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

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum RunError {
    FailedToDecodeUtf8,
    ProgramTerminated,
    IOError(io::Error),
}

// RUN TEST CASES

pub fn run_test_cases(
    report_config: &ReportConfig,
    test_cases: &[TestCase],
    run_in_parallel: bool,
    current_dir: &Path,
    report_test_case: &(
         impl Fn(&ReportConfig, usize, &TestCase, &Result<TestResult, RunError>) -> Result<(), RunError>
         + std::marker::Sync
     ),
) -> Vec<RunResult> {
    let run = |(i, test_case)| -> RunResult {
        let run_result = run_test_case(test_case, current_dir);

        let report_result = report_test_case(report_config, i, test_case, &run_result);

        let result = match report_result {
            Ok(()) => run_result,
            Err(e) => run_result.and(Err(e)),
        };

        RunResult {
            test_case: test_case.clone(),
            result,
        }
    };

    if run_in_parallel {
        test_cases.par_iter().enumerate().map(run).collect()
    } else {
        test_cases.iter().enumerate().map(run).collect()
    }
}

pub fn run_test_case(test_case: &TestCase, current_dir: &Path) -> Result<TestResult, RunError> {
    let output = run_program(test_case, current_dir)?;

    let expected_stdout = test_case.expected_stdout.as_deref().map(normalize_newlines);
    let expected_stderr = test_case.expected_stderr.as_deref().map(normalize_newlines);

    Ok(TestResult {
        stdout: compare_result(expected_stdout, output.stdout),
        stderr: compare_result(expected_stderr, output.stderr),
        exit_code: compare_result(test_case.expected_exit_code, output.exit_code),
    })
}

pub fn run_program(test_case: &TestCase, current_dir: &Path) -> Result<ProgramOutput, RunError> {
    let mut cmd = init_command(test_case, current_dir);

    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(RunError::IOError)?;

    if let Some(stdin_string) = &test_case.stdin {
        write_stdin(&mut child, stdin_string)?;
    }

    let stdout = read_pipe_to_string(
        &mut child
            .stdout
            .take()
            .expect("Stdout should be configured to pipe"),
    )?;
    let stderr = read_pipe_to_string(
        &mut child
            .stderr
            .take()
            .expect("Stderr should be configured to pipe"),
    )?;

    let exit_status = child.wait().map_err(RunError::IOError)?;
    let exit_code = exit_status.code().ok_or(RunError::ProgramTerminated)?;

    Ok(ProgramOutput {
        stdout: normalize_newlines(&stdout),
        stderr: normalize_newlines(&stderr),
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
        write_stdin(&mut child, stdin_string)?;
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

fn write_stdin(child: &mut std::process::Child, stdin_string: &String) -> Result<(), RunError> {
    let mut stdin = child
        .stdin
        .take()
        .expect("Stdin should be configured to pipe");
    stdin
        .write_all(stdin_string.as_bytes())
        .map_err(RunError::IOError)?;

    Ok(())
}

fn compare_result<T: Eq>(expected: Option<T>, got: T) -> ValueComparison<T> {
    if let Some(expected) = expected {
        if expected == got {
            ValueComparison::Matches(got)
        } else {
            ValueComparison::Diff { expected, got }
        }
    } else {
        ValueComparison::NotChecked
    }
}

fn read_pipe_to_string<T>(pipe: &mut T) -> Result<String, RunError>
where
    T: Read,
{
    let mut buf = Vec::<u8>::new();
    pipe.read_to_end(&mut buf).map_err(RunError::IOError)?;
    let content = String::from_utf8(buf).map_err(|_| RunError::FailedToDecodeUtf8)?;

    Ok(content)
}

/// Normalize line endings to line feed (LF)
///
/// Windows uses carriage return line feed (CRLF) as line endings, thus
/// test cases may contain CRLF line endings. Additionally, the output of
/// the program under test may contain CRLF line endings.
fn normalize_newlines(content: &str) -> String {
    content.replace("\r\n", "\n")
}
