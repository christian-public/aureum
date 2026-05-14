use crate::counts::{ConfigStats, TestCounts};
use crate::report::formats::summary;
use crate::report::formats::tap;
use crate::report::theme;
use crate::vendor::ascii_tree::Tree::{Leaf, Node};
use aureum::{RunError, RunResult, TestCase, TestOutcome};
use colored::Colorize;
use std::io;

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ReportFormat {
    Summary,
    Tap,
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ReportConfig {
    pub number_of_tests: usize,
    pub format: ReportFormat,
    pub verbose: bool,
}

pub fn print_watch_started(count: usize) {
    let paths = if count == 1 { "path" } else { "paths" };
    eprintln!("{} watching {count} {paths} for changes...", theme::watch());
}

pub fn print_watch_detected_file_changes() {
    eprintln!();
    eprintln!("{} changes detected", theme::watch());
}

pub fn print_interactive_mode_requires_a_terminal_error() {
    eprintln!("{} `--interactive` requires a terminal", theme::error());
}

pub fn print_interactive_watch_session_failed(error: &io::Error) {
    eprintln!(
        "{} `--interactive` + `--watch` session failed: {error}",
        theme::error()
    );
}

pub fn print_interactive_session_failed(error: &io::Error) {
    eprintln!("{} `--interactive` session failed: {error}", theme::error());
}

pub fn print_watch_record_session_failed(error: &io::Error) {
    eprintln!(
        "{} `--watch` + `--record` session failed: {error}",
        theme::error()
    );
}

pub fn print_watch_session_failed(error: &io::Error) {
    eprintln!("{} `--watch` session failed: {error}", theme::error());
}

pub fn print_record_session_failed(error: &io::Error) {
    eprintln!("{} `--record` session failed: {error}", theme::error());
}

pub fn print_test_cases_start(report_config: &ReportConfig) {
    match report_config.format {
        ReportFormat::Summary => {
            summary_print_test_cases_start(report_config.number_of_tests);
        }
        ReportFormat::Tap => {
            tap_print_test_cases_start(report_config.number_of_tests);
        }
    }
}

pub fn print_test_case(
    report_config: &ReportConfig,
    index: usize,
    test_case: &TestCase,
    result: &Result<TestOutcome, RunError>,
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

pub fn print_test_cases_end(
    report_config: &ReportConfig,
    run_results: &[RunResult],
    config_stats: ConfigStats,
) {
    match report_config.format {
        ReportFormat::Summary => {
            summary_print_test_cases_end(run_results, report_config.verbose, config_stats);
        }
        ReportFormat::Tap => {
            tap_print_test_cases_end();
        }
    }
}

// SUMMARY HELPERS

fn summary_print_test_cases_start(number_of_tests: usize) {
    let label = if number_of_tests == 1 {
        "test"
    } else {
        "tests"
    };
    println!("🚀 Running {number_of_tests} {label}:")
}

fn summary_print_test_case(result: &Result<TestOutcome, RunError>) {
    match result {
        Ok(test_outcome) => {
            if test_outcome.is_success() {
                print!(".");
            } else {
                print!("F");
            }
        }
        Err(_) => {
            print!("F");
        }
    }

    let _ = io::Write::flush(&mut io::stdout());
}

fn summary_print_test_cases_end(
    run_results: &[RunResult],
    verbose: bool,
    config_stats: ConfigStats,
) {
    println!(); // Print newline after dots

    let (passed_tests, failed_tests): (Vec<_>, Vec<_>) =
        run_results.iter().partition(|x| x.is_success());

    if verbose && !passed_tests.is_empty() {
        println!(); // Print newline before all the tests

        for passed_test in passed_tests {
            let RunResult::Ran { test_case, .. } = passed_test;
            println!("{}", format_test_success(test_case));
        }
    }

    for failed_test in failed_tests {
        let RunResult::Ran { test_case, result } = failed_test;
        println!(); // Print newline before each failed test
        println!("{}", format_test_failure(test_case, result));
    }

    let counts = TestCounts::from_results(run_results, config_stats);

    println!();
    println!("{}", format_summary_line(counts));
}

fn format_test_success(test_case: &TestCase) -> String {
    format!("{} {}", theme::checkmark(), test_case.display_id())
}

fn format_test_failure(test_case: &TestCase, result: &Result<TestOutcome, RunError>) -> String {
    let nodes = match result {
        Ok(test_outcome) => summary::nodes_from_test_outcome(test_outcome),
        Err(error) => {
            vec![Leaf(vec![format!("Run error: {error}")])]
        }
    };

    let heading = format!("❌ {}", test_case.display_id());
    let tree = Node(heading, nodes);
    tree.to_string().trim_end().to_owned()
}

fn format_summary_line(counts: TestCounts) -> String {
    let config_error_count = counts.config_stats.config_errors;

    let status = if counts.failed == 0 {
        if config_error_count == 0 {
            "OK".green().bold()
        } else {
            "OK*".yellow().bold()
        }
    } else {
        "FAIL".red().bold()
    };

    let mut count_components = vec![];

    if counts.config_stats.config_errors > 0 {
        let errors = if config_error_count == 1 {
            "error"
        } else {
            "errors"
        };
        count_components.push(format!("{config_error_count} config {errors}"));
    }

    count_components.push(format!("{} passed", counts.passed));

    if counts.failed > 0 {
        count_components.push(format!("{} failed", counts.failed));
    }

    format!("Test result: {status} ({})", count_components.join(", "))
}

// TAP HELPERS

fn tap_print_test_cases_start(number_of_tests: usize) {
    tap::print_version();
    tap::print_plan(1, number_of_tests);
}

fn tap_print_test_case(
    test_number: usize,
    test_case: &TestCase,
    result: &Result<TestOutcome, RunError>,
    max_width: usize,
) {
    let message = test_case.display_id();

    match result {
        Ok(test_outcome) => {
            if test_outcome.is_success() {
                tap::print_ok(test_number, max_width, &message)
            } else {
                tap::print_not_ok(
                    test_number,
                    max_width,
                    &message,
                    Some(&tap::test_outcome_diagnostic(test_outcome)),
                )
            }
        }
        Err(error) => tap::print_not_ok(
            test_number,
            max_width,
            &message,
            Some(&tap::message_diagnostic(&error.to_string())),
        ),
    }
}

fn tap_print_test_cases_end() {
    // Do nothing
}
