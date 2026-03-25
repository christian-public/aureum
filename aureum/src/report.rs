use crate::TestCase;
use crate::Tree::{Leaf, Node};
use crate::formats::tap;
use crate::formats::tree;
use crate::test_result::TestResult;
use crate::test_runner::{RunError, RunResult};

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

pub fn report_start(report_config: &ReportConfig) {
    match report_config.format {
        ReportFormat::Summary => {
            summary_print_start(report_config.number_of_tests);
        }
        ReportFormat::Tap => {
            tap_print_start(report_config.number_of_tests);
        }
    }
}

pub fn report_test_case(
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

pub fn report_summary(report_config: &ReportConfig, run_results: &[RunResult]) {
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
