use aureum::string;
use aureum::{FieldOutcome, TestOutcome};
use yaml_serde::{Mapping, Number, Value};

pub fn print_version() {
    println!("TAP version 14")
}

pub fn print_plan(start: usize, end: usize) {
    println!("{start}..{end}")
}

pub fn print_ok(test_number: usize, max_width: usize, message: &str) {
    let message = escape_message(message);
    println!("ok     {test_number:>max_width$} - {message}")
}

pub fn print_ok_skip(test_number: usize, max_width: usize, message: &str, reason: &str) {
    let message = escape_message(message);
    println!("ok     {test_number:>max_width$} - {message} # SKIP {reason}")
}

pub fn print_not_ok(
    test_number: usize,
    max_width: usize,
    message: &str,
    diagnostic: Option<&Value>,
) {
    let message = escape_message(message);
    println!("not ok {test_number:>max_width$} - {message}");

    if let Some(diagnostic) = diagnostic {
        print_diagnostic(diagnostic)
    }
}

// DIAGNOSTIC

pub fn test_outcome_diagnostic(test_outcome: &TestOutcome) -> Value {
    let mut map = Mapping::new();

    if let FieldOutcome::Diff { expected, got } = &test_outcome.stdout {
        map.insert(
            Value::String(String::from("stdout")),
            string_diff_diagnostic(expected, got),
        );
    }

    if let FieldOutcome::Diff { expected, got } = &test_outcome.stderr {
        map.insert(
            Value::String(String::from("stderr")),
            string_diff_diagnostic(expected, got),
        );
    }

    if let FieldOutcome::Diff { expected, got } = test_outcome.exit_code {
        map.insert(
            Value::String(String::from("exit-code")),
            i32_diff_diagnostic(expected, got),
        );
    }

    Value::Mapping(map)
}

pub fn message_diagnostic(message: &str) -> Value {
    let mut map = Mapping::new();
    map.insert(
        Value::String(String::from("message")),
        Value::String(message.to_owned()),
    );
    Value::Mapping(map)
}

fn string_diff_diagnostic(expected: &str, got: &str) -> Value {
    diff_diagnostic(
        Value::String(expected.to_owned()),
        Value::String(got.to_owned()),
    )
}

fn i32_diff_diagnostic(expected: i32, got: i32) -> Value {
    diff_diagnostic(
        Value::Number(Number::from(expected)),
        Value::Number(Number::from(got)),
    )
}

fn diff_diagnostic(expected: Value, got: Value) -> Value {
    let mut map = Mapping::new();
    map.insert(Value::String(String::from("expected")), expected);
    map.insert(Value::String(String::from("got")), got);
    Value::Mapping(map)
}

// HELPERS

fn print_diagnostic(value: &Value) {
    if let Ok(yaml) = yaml_serde::to_string(value) {
        let yaml = yaml.trim_end_matches('\n');
        let code_block = format!("---\n{yaml}\n...");
        println!("{}", string::indent_by(&code_block, 2));
    } else {
        println!("# error: failed to serialize YAML");
    }
}

fn escape_message(message: &str) -> String {
    message.replace('\\', "\\\\").replace('#', "\\#")
}
