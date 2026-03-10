use crate::test_id::TestId;
use crate::test_result::{TestResult, ValueComparison};
use crate::utils::file;
use relative_path::RelativePathBuf;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct TestCase {
    pub source_file: RelativePathBuf,
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
        let file_path = self.source_file.to_string();

        if self.test_id.is_root() {
            file_path
        } else {
            format!("{}:{}", file_path, self.test_id)
        }
    }
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum RunError {
    FailedToDecodeUtf8,
    MissingExitCode,
    IOError(io::Error),
}

pub fn run(test_case: &TestCase) -> Result<TestResult, RunError> {
    let current_dir = file::parent_dir(&test_case.source_file).to_logical_path(".");

    let mut cmd = Command::new(&test_case.program);
    cmd.current_dir(current_dir);
    cmd.args(&test_case.arguments);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

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
    let exit_code = exit_status.code().ok_or(RunError::MissingExitCode)?;

    let expected_stdout = test_case.expected_stdout.as_deref().map(normalize_newlines);
    let expected_stderr = test_case.expected_stderr.as_deref().map(normalize_newlines);

    Ok(TestResult {
        stdout: compare_result(expected_stdout, normalize_newlines(&stdout)),
        stderr: compare_result(expected_stderr, normalize_newlines(&stderr)),
        exit_code: compare_result(test_case.expected_exit_code, exit_code),
    })
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
    let mut buf: Vec<u8> = vec![];
    pipe.read_to_end(&mut buf).map_err(RunError::IOError)?;
    String::from_utf8(buf).map_or(Err(RunError::FailedToDecodeUtf8), Ok)
}

/// Normalize line endings to line feed (LF)
///
/// Windows uses carriage return line feed (CRLF) as line endings, thus
/// test cases may contain CRLF line endings. Additionally, the output of
/// the program under test may contain CRLF line endings.
fn normalize_newlines(content: &str) -> String {
    content.replace("\r\n", "\n")
}
