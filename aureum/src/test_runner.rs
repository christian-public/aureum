use crate::formats::tree::{Leaf, Node};
use crate::formats::{tap, tree};
use crate::test_case::TestCase;
use crate::test_result::{TestResult, ValueComparison};
use rayon::prelude::*;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ReportConfig {
    pub number_of_tests: usize,
    pub format: ReportFormat,
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ReportFormat {
    Summary,
    Tap,
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

// RUN TEST CASES

pub fn run_test_cases(
    report_config: &ReportConfig,
    test_cases: &[TestCase],
    run_in_parallel: bool,
    current_dir: &Path,
) -> Vec<RunResult> {
    let run = |(i, test_case)| -> Vec<RunResult> {
        let result = run(test_case, current_dir);

        report_test_case(report_config, i, test_case, &result);

        vec![RunResult {
            test_case: test_case.clone(),
            result,
        }]
    };

    report_start(report_config);

    let run_results = if run_in_parallel {
        test_cases
            .par_iter()
            .enumerate()
            .map(run)
            .reduce(Vec::new, |x, y| x.into_iter().chain(y).collect())
    } else {
        test_cases
            .iter()
            .enumerate()
            .map(run)
            .fold(vec![], |x, y| x.into_iter().chain(y).collect())
    };

    report_summary(report_config, &run_results);

    run_results
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum RunError {
    FailedToDecodeUtf8,
    MissingExitCode,
    IOError(io::Error),
}

pub fn run(test_case: &TestCase, current_dir: &Path) -> Result<TestResult, RunError> {
    let run_test_in_config_dir = &test_case.path_to_containing_dir.to_path(current_dir);

    let mut cmd = Command::new(&test_case.program);
    cmd.current_dir(run_test_in_config_dir);
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

// REPORTING

fn report_start(report_config: &ReportConfig) {
    match report_config.format {
        ReportFormat::Summary => {
            summary_print_start(report_config.number_of_tests);
        }
        ReportFormat::Tap => {
            tap_print_start(report_config.number_of_tests);
        }
    }
}

fn report_test_case(
    report_config: &ReportConfig,
    index: usize,
    test_case: &TestCase,
    result: &Result<TestResult, RunError>,
) {
    match report_config.format {
        ReportFormat::Summary => {
            summary_print_test_case(result);
        }
        ReportFormat::Tap => {
            let test_number_indent_level = report_config.number_of_tests.to_string().len();
            tap_print_test_case(index + 1, test_case, result, test_number_indent_level);
        }
    }
}

fn report_summary(report_config: &ReportConfig, run_results: &[RunResult]) {
    match report_config.format {
        ReportFormat::Summary => {
            summary_print_summary(report_config.number_of_tests, run_results);
        }
        ReportFormat::Tap => {
            tap_print_summary();
        }
    }
}

// SUMMARY HELPERS

fn summary_print_start(number_of_tests: usize) {
    println!("🚀 Running {} tests:", number_of_tests)
}

fn summary_print_test_case(result: &Result<TestResult, RunError>) {
    match result {
        Ok(test_result) => {
            if test_result.is_success() {
                print!(".")
            } else {
                print!("F")
            }
        }
        Err(_) => {
            print!("F")
        }
    }
}

fn summary_print_summary(number_of_tests: usize, run_results: &[RunResult]) {
    println!(); // Add newline to dots

    let mut is_any_test_cases_printed = false;

    for run_result in run_results {
        let test_failed = !run_result.is_success();
        if test_failed {
            if !is_any_test_cases_printed {
                println!();
                is_any_test_cases_printed = true;
            }

            summary_print_result(run_result);
        }
    }

    let number_of_passed_tests = run_results.iter().filter(|t| t.is_success()).count();
    let number_of_failed_tests = number_of_tests - number_of_passed_tests;

    let status = if number_of_failed_tests == 0 {
        "OK"
    } else {
        "FAIL"
    };

    println!();
    println!(
        "Test result: {} ({} passed, {} failed)",
        status, number_of_passed_tests, number_of_failed_tests,
    );
}

fn summary_print_result(run_result: &RunResult) {
    let test_id = run_result.test_case.id();

    let message: String;
    if let Some(description) = &run_result.test_case.description {
        message = format!("{} - {}", test_id, description);
    } else {
        message = test_id;
    }

    if run_result.is_success() {
        println!("✅ {}", message)
    } else {
        let nodes = match &run_result.result {
            Ok(result) => tree::nodes_from_test_result(result),
            Err(_) => {
                vec![Leaf(vec![String::from("Failed to run test")])]
            }
        };

        let test_heading = format!("❌ {}", message);
        let tree = Node(test_heading, nodes);
        let content = tree::draw_tree(&tree).unwrap_or(String::from("Failed to draw tree\n"));
        print!("{}", content); // Already contains newline
    }
}

// TAP HELPERS

fn tap_print_start(number_of_tests: usize) {
    tap::print_version();
    tap::print_plan(1, number_of_tests);
}

fn tap_print_test_case(
    test_number: usize,
    test_case: &TestCase,
    result: &Result<TestResult, RunError>,
    indent_level: usize,
) {
    let message: String;
    if let Some(description) = &test_case.description {
        message = format!("{} # {}", test_case.id(), description);
    } else {
        message = test_case.id();
    }

    match result {
        Ok(test_result) => {
            if test_result.is_success() {
                tap::print_ok(test_number, &message, indent_level)
            } else {
                tap::print_not_ok(test_number, &message, test_result, indent_level)
            }
        }
        Err(_) => {
            tap::print_not_ok_diagnostics(test_number, &message, "Failed to run test", indent_level)
        }
    }
}

fn tap_print_summary() {}
