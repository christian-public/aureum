use crate::report::formats::summary;
use crate::report::formats::tap;
use crate::report::label;
use crate::report::symbol;
use crate::vendor::ascii_tree::Tree::{Leaf, Node};
use aureum::{RunError, RunResult, TestCase, TestResult};
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
}

pub fn print_watch_detected_file_changes(count: usize) {
    let msg = format!("Detected changes in {count} file(s)");
    eprintln!();
    eprintln!("{} {}", label::watch(), msg.dimmed());
}

pub fn print_interactive_mode_requires_a_terminal_error() {
    eprintln!("{} --interactive requires a terminal", label::error());
}

pub fn print_test_cases_start(report_config: &ReportConfig) {
    match report_config.format {
        ReportFormat::Summary => {
            summary_print_start(report_config.number_of_tests);
        }
        ReportFormat::Tap => {
            tap_print_start(report_config.number_of_tests);
        }
    }
}

pub fn print_test_case(
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

pub fn print_test_cases_end(report_config: &ReportConfig, run_results: &[RunResult]) {
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
    println!("🚀 Running {number_of_tests} tests:")
}

fn summary_print_test_case(result: &Result<TestResult, RunError>) {
    match result {
        Ok(test_result) => {
            if test_result.is_success() {
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
        "OK".green().bold()
    } else {
        "FAIL".red().bold()
    };

    println!();
    println!(
        "Test result: {status} ({number_of_passed_tests} passed, {number_of_failed_tests} failed)",
    );
}

fn summary_print_result(run_result: &RunResult) {
    let test_id = run_result.test_case.id();

    let message = if let Some(description) = &run_result.test_case.description {
        format!("{test_id} - {description}")
    } else {
        test_id
    };

    if run_result.is_success() {
        println!("{} {message}", symbol::checkmark());
    } else {
        let nodes = match &run_result.result {
            Ok(result) => summary::nodes_from_test_result(result),
            Err(_) => {
                vec![Leaf(vec![String::from("Failed to run test")])]
            }
        };

        let test_heading = format!("❌ {message}");
        let tree = Node(test_heading, nodes);
        let content = tree.to_string();
        print!("{content}"); // Already contains newline
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
    let message: String = if let Some(description) = &test_case.description {
        format!("{} # {description}", test_case.id())
    } else {
        test_case.id()
    };

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

fn tap_print_summary() {
    // Do nothing
}
