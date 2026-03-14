use crate::test_result::{TestResult, ValueComparison};
use crate::utils::string;
use std::collections::BTreeMap;
use yaml_serde::{Number, Value};

pub fn print_version() {
    println!("TAP version 14")
}

pub fn print_plan(start: usize, end: usize) {
    println!("{}..{}", start, end)
}

pub fn print_ok(test_number: usize, message: &str, indent_level: usize) {
    println!(
        "ok     {:>indent$} - {}",
        test_number,
        message,
        indent = indent_level
    )
}

pub fn print_not_ok(
    test_number: usize,
    message: &str,
    test_result: &TestResult,
    indent_level: usize,
) {
    let diagnostics = format_test_result(test_result);
    print_not_ok_diagnostics(test_number, message, &diagnostics, indent_level);
}

pub fn print_not_ok_diagnostics(
    test_number: usize,
    message: &str,
    diagnostics: &str,
    indent_level: usize,
) {
    println!(
        "not ok {:>indent$} - {}",
        test_number,
        message,
        indent = indent_level
    );

    if !diagnostics.is_empty() {
        print_diagnostics(diagnostics)
    }
}

pub fn print_diagnostics(diagnostics: &str) {
    let code_block = format!("---\n{}...", diagnostics);
    println!("{}", string::indent_by(2, &code_block));
}

#[allow(dead_code)]
pub fn print_bail_out(message: &str) {
    println!("Bail out! {}", message)
}

// ERROR FORMATTING

fn format_test_result(test_result: &TestResult) -> String {
    let mut diagnostics = BTreeMap::new();

    if let ValueComparison::Diff { expected, got } = &test_result.stdout {
        diagnostics.insert("stdout", show_string_diff(expected, got));
    }

    if let ValueComparison::Diff { expected, got } = &test_result.stderr {
        diagnostics.insert("stderr", show_string_diff(expected, got));
    }

    if let ValueComparison::Diff { expected, got } = test_result.exit_code {
        diagnostics.insert("exit-code", show_i32_diff(expected, got));
    }

    yaml_serde::to_string(&diagnostics).unwrap_or(String::from("Failed to convert to YAML\n"))
}

fn show_string_diff(expected: &String, got: &String) -> BTreeMap<&'static str, Value> {
    show_diff(
        Value::String(expected.to_owned()),
        Value::String(got.to_owned()),
    )
}

fn show_i32_diff(expected: i32, got: i32) -> BTreeMap<&'static str, Value> {
    show_diff(
        Value::Number(Number::from(expected)),
        Value::Number(Number::from(got)),
    )
}

fn show_diff(expected: Value, got: Value) -> BTreeMap<&'static str, Value> {
    BTreeMap::from([("expected", expected), ("got", got)])
}
