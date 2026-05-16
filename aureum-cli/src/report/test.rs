use crate::counts::{ConfigStats, PlannedCounts, TestCounts};
use crate::report::formats::summary;
use crate::report::formats::tap;
use crate::report::theme;
use crate::utils::time;
use crate::vendor::ascii_tree::Tree::{Leaf, Node};
use aureum::{RunError, RunResult, TestCase, TestId, TestOutcome};
use colored::Colorize;
use std::io;
use std::time::Duration;

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ReportFormat {
    Summary,
    Tap,
}

#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ReportConfig {
    pub planned_counts: PlannedCounts,
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
            summary_print_test_cases_start(report_config.planned_counts);
        }
        ReportFormat::Tap => {
            tap_print_test_cases_start(report_config.planned_counts.total());
        }
    }
}

pub fn print_test_case(report_config: &ReportConfig, index: usize, run_result: &RunResult) {
    match report_config.format {
        ReportFormat::Summary => {
            summary_print_test_case(run_result);
        }
        ReportFormat::Tap => {
            let test_number_indent_level = report_config.planned_counts.total().to_string().len();
            tap_print_test_case(index + 1, run_result, test_number_indent_level);
        }
    }
}

pub fn print_test_cases_end(
    report_config: &ReportConfig,
    run_results: &[RunResult],
    config_stats: ConfigStats,
    elapsed: Duration,
) {
    match report_config.format {
        ReportFormat::Summary => {
            summary_print_test_cases_end(run_results, report_config.verbose, config_stats, elapsed);
        }
        ReportFormat::Tap => {
            tap_print_test_cases_end();
        }
    }
}

// SUMMARY HELPERS

fn summary_print_test_cases_start(counts: PlannedCounts) {
    let total = counts.total();
    let label = if total == 1 { "test" } else { "tests" };
    if counts.runnable == total {
        println!("🚀 Running {total} {label}:")
    } else {
        println!("🚀 Running {} of {total} {label}:", counts.runnable)
    }
}

fn summary_print_test_case(run_result: &RunResult) {
    match run_result {
        RunResult::Skipped { .. } => print!("s"),
        RunResult::Ran { .. } if run_result.is_success() => print!("."),
        RunResult::Ran { .. } => print!("F"),
    }

    let _ = io::Write::flush(&mut io::stdout());
}

fn summary_print_test_cases_end(
    run_results: &[RunResult],
    verbose: bool,
    config_stats: ConfigStats,
    elapsed: Duration,
) {
    println!(); // Print newline after dots

    let mut skipped_tests: Vec<(&TestId, &String)> = vec![];
    let mut passed_tests: Vec<&TestCase> = vec![];
    let mut failed_tests: Vec<(&TestCase, &Result<TestOutcome, RunError>)> = vec![];

    for run_result in run_results {
        match run_result {
            RunResult::Skipped { id, reason } => {
                skipped_tests.push((id, reason));
            }
            RunResult::Ran {
                test_case,
                result: Ok(test_outcome),
            } if run_result.is_success() => passed_tests.push(test_case),
            RunResult::Ran { test_case, result } => {
                failed_tests.push((test_case, result));
            }
        }
    }

    if verbose && !skipped_tests.is_empty() {
        println!(); // Print newline before list of tests

        let max_width = skipped_tests
            .iter()
            .map(|(id, _)| id.display_id().len())
            .max()
            .unwrap_or(0);
        for (id, reason) in skipped_tests {
            println!("{}", format_test_skipped(id, max_width, reason));
        }
    }

    if verbose && !passed_tests.is_empty() {
        println!(); // Print newline before list of tests

        for test_case in passed_tests {
            println!("{}", format_test_success(test_case));
        }
    }

    for (test_case, result) in failed_tests {
        println!(); // Print newline before each failed test
        println!("{}", format_test_failure(test_case, result));
    }

    let counts = TestCounts::from_results(run_results, config_stats);

    println!();
    println!("{}", format_summary_line(counts, elapsed));
}

fn format_test_success(test_case: &TestCase) -> String {
    format!("{} {}", theme::checkmark(), test_case.display_id())
}

fn format_test_skipped(id: &TestId, max_width: usize, reason: &str) -> String {
    format!(
        "{} {:<max_width$}  {reason}",
        theme::skip(),
        id.display_id()
    )
}

fn format_test_failure(test_case: &TestCase, result: &Result<TestOutcome, RunError>) -> String {
    let nodes = match result {
        Ok(test_outcome) => summary::nodes_from_test_outcome(test_outcome),
        Err(error) => {
            vec![Leaf(vec![format!("Run error: {error}")])]
        }
    };

    let heading = format!("{} {}", theme::cross(), test_case.display_id())
        .red()
        .to_string();
    let tree = Node(heading, nodes);
    tree.to_string().trim_end().to_owned()
}

fn format_summary_line(counts: TestCounts, elapsed: Duration) -> String {
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

    if counts.failed > 0 {
        count_components.push(format!("{} failed", counts.failed));
    }

    count_components.push(format!("{} passed", counts.passed));

    if counts.skipped > 0 {
        count_components.push(format!("{} skipped", counts.skipped));
    }

    if config_error_count > 0 {
        let errors = if config_error_count == 1 {
            "error"
        } else {
            "errors"
        };
        count_components.push(format!("{config_error_count} config {errors}"));
    }

    let suffix = format!(" — Finished in {}", time::format_duration(elapsed))
        .dimmed()
        .to_string();

    format!(
        "Test result: {status} ({}){suffix}",
        count_components.join(", ")
    )
}

// TAP HELPERS

fn tap_print_test_cases_start(number_of_tests: usize) {
    tap::print_version();
    tap::print_plan(1, number_of_tests);
}

fn tap_print_test_case(test_number: usize, run_result: &RunResult, max_width: usize) {
    match run_result {
        RunResult::Skipped { id, reason } => {
            tap::print_ok_skip(test_number, max_width, &id.display_id(), reason);
        }
        RunResult::Ran { test_case, result } => {
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
    }
}

fn tap_print_test_cases_end() {
    // Do nothing
}
