use crate::Tree::{self, Leaf, Node};
use crate::formats::tap;
use crate::formats::tree;
use crate::test_result::TestResult;
use crate::test_runner::{RunError, RunResult};
use crate::utils::file;
use crate::{
    ParsedTomlConfig, ProgramPath, RequirementData, Requirements, TestCase, TestId,
    TomlConfigError, ValidationError,
};
use colored::Colorize;
use relative_path::RelativePathBuf;
use std::collections::BTreeMap;
use std::path::PathBuf;

// TEST CASE

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

pub fn print_start_test_cases(report_config: &ReportConfig) {
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

pub fn print_summary(report_config: &ReportConfig, run_results: &[RunResult]) {
    match report_config.format {
        ReportFormat::Summary => {
            summary_print_summary(report_config.number_of_tests, run_results);
        }
        ReportFormat::Tap => {
            tap_print_summary();
        }
    }
}

// VALIDATION

pub fn print_invalid_paths(paths: Vec<PathBuf>) {
    eprintln!(
        "{} Invalid paths to config files:",
        "warning:".yellow().bold(),
    );
    for path in paths {
        eprintln!("- {}", path.display());
    }
    eprintln!();
}

pub fn print_no_config_files() {
    eprintln!(
        "{} No config files found for the given paths",
        "error:".red().bold(),
    );
}

pub fn print_files_found(source_files: &[RelativePathBuf]) {
    let heading = format!("🔍 Found {} config files", source_files.len());
    let tree = Node(
        heading,
        source_files
            .iter()
            .map(|x| str_to_tree(x.as_ref()))
            .collect(),
    );

    print_tree(tree);
}

pub fn print_config_details(
    source_file: RelativePathBuf,
    parsed_toml_configs: &BTreeMap<TestId, ParsedTomlConfig>,
    requirement_data: &RequirementData,
    verbose: bool,
    hide_absolute_paths: bool,
) {
    let mut tests = vec![];

    for (test_id, parsed_toml_config) in parsed_toml_configs {
        let mut categories = vec![];

        if verbose {
            // Program to run
            let program_to_run = match &parsed_toml_config.program_path {
                ProgramPath::NotSpecified => String::from("❌ Not specified"),
                ProgramPath::MissingProgram { requested_path: _ } => String::from("❌ Not found"),
                ProgramPath::ResolvedPath {
                    requested_path: _,
                    resolved_path,
                } => {
                    let path = if hide_absolute_paths {
                        file::display_path(resolved_path)
                    } else {
                        resolved_path.display().to_string()
                    };
                    format!("✅ {}", path)
                }
            };

            let nodes = vec![str_to_tree(&program_to_run)];

            let heading = String::from("Program to run");
            categories.push(Node(heading, nodes));

            // Requirements
            let requirements = requirements_map(&parsed_toml_config.requirements, requirement_data);
            if !requirements.is_empty() {
                let heading = String::from("Requirements");
                categories.push(Node(heading, requirements));
            }
        }

        // Validation errors
        let heading = String::from("Validation errors");
        if let Err(validation_errors) = &parsed_toml_config.test_cases {
            let nodes = validation_errors
                .iter()
                .map(|err| str_to_tree(&show_validation_error(err)))
                .collect();

            categories.push(Node(heading, nodes));
        }

        tests.push((test_id, categories))
    }

    let is_root = tests.len() == 1 && tests[0].0.is_root();
    let nodes: Vec<Tree> = if is_root {
        tests.into_iter().next().unwrap().1
    } else {
        tests
            .into_iter()
            .map(|(test_id, children)| Node(format!(":{}", test_id), children))
            .collect()
    };

    let tree = Node(config_heading(source_file), nodes);

    print_tree(tree);
}

pub fn print_toml_config_error(source_file: RelativePathBuf, error: TomlConfigError) {
    let msg = match error {
        TomlConfigError::InvalidTomlSyntax(_) => "Failed to parse config file",
        TomlConfigError::ParseErrors(_) => "Failed to parse config file",
    };
    let tree = Node(config_heading(source_file), vec![str_to_tree(msg)]);

    print_tree(tree);
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

// OTHER HELPERS

fn print_tree(tree: Tree) {
    let content = tree::draw_tree(&tree).unwrap_or(String::from("Failed to draw tree\n"));

    eprint!("{}", content); // Already contains newline
    eprintln!()
}

fn config_heading(source_file: RelativePathBuf) -> String {
    format!("📋 {}", source_file)
}

fn requirements_map(requirements: &Requirements, requirement_data: &RequirementData) -> Vec<Tree> {
    let mut categories = vec![];

    if !requirements.files.is_empty() {
        categories.push(Node(
            String::from("Files"),
            requirements
                .files
                .iter()
                .map(|file| {
                    let is_present = requirement_data.get_file(file).is_some();
                    str_to_tree(&format!("{} {}", show_presence(is_present), file))
                })
                .collect(),
        ));
    }

    if !requirements.env_vars.is_empty() {
        categories.push(Node(
            String::from("Environment variables"),
            requirements
                .env_vars
                .iter()
                .map(|env_var| {
                    let is_present = requirement_data.get_env_var(env_var).is_some();
                    str_to_tree(&format!("{} {}", show_presence(is_present), env_var))
                })
                .collect(),
        ));
    }

    categories
}

fn show_validation_error(validation_error: &ValidationError) -> String {
    let msg = match validation_error {
        ValidationError::MissingExternalFile(file_path) => {
            format!("Missing external file '{}'", file_path)
        }
        ValidationError::MissingEnvVar(var_name) => {
            format!("Missing environment variable '{}'", var_name)
        }
        ValidationError::FailedToParseString => String::from("Failed to parse string"),
        ValidationError::ProgramRequired => String::from("The field 'program' is required"),
        ValidationError::ProgramNotFound(program) => {
            format!("The program '{}' was not found", program)
        }
        ValidationError::ExpectationRequired => {
            String::from("At least one expectation is required")
        }
    };

    format!("❌ {}", msg)
}

fn show_presence(value: bool) -> String {
    String::from(if value { "✅" } else { "❌" })
}

fn str_to_tree(msg: &str) -> Tree {
    Leaf(vec![msg.to_owned()])
}
