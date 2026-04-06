use crate::formats::tap;
use crate::formats::tree;
use crate::test_case::TestCase;
use crate::test_id::TestId;
use crate::test_result::TestResult;
use crate::test_runner::{ProgramOutput, RunError, RunResult};
use crate::toml::{
    ProgramPath, RequirementData, Requirements, TestEntry, TomlConfigError, ValidationError,
};
use crate::utils::file;
use crate::vendor::ascii_tree::Tree::{self, Leaf, Node};
use colored::Colorize;
use relative_path::{RelativePath, RelativePathBuf};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

// INIT

pub fn print_file_already_exists(path: &Path) {
    eprintln!("{} file already exists: {}", error(), path.display());
}

pub fn print_failed_to_write_file(path: &Path) {
    eprintln!("{} failed to write file: {}", error(), path.display());
}

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
) -> Result<(), RunError> {
    match report_config.format {
        ReportFormat::Summary => {
            summary_print_test_case(result)?;
        }
        ReportFormat::Tap => {
            let test_number_indent_level = report_config.number_of_tests.to_string().len();
            tap_print_test_case(index + 1, test_case, result, test_number_indent_level);
        }
    }

    Ok(())
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

// LIST

pub fn print_test_list_as_tree(test_cases: &[TestCase]) {
    let tree = build_test_list_tree(test_cases);
    let output = tree::draw_tree(&tree).unwrap_or(String::from("Failed to draw tree\n"));

    print!("{}", output);
}

fn build_test_list_tree(test_cases: &[TestCase]) -> Tree {
    let mut by_file: BTreeMap<Vec<String>, Vec<String>> = BTreeMap::new();

    for test_case in test_cases {
        let segments: Vec<String> = test_case
            .path_to_config_file()
            .components()
            .map(|c| c.as_str().to_string())
            .collect();
        let subtests = by_file.entry(segments).or_default();
        if !test_case.test_id.is_root() {
            subtests.push(format!(":{}", test_case.test_id));
        }
    }

    build_tree_node("/", &by_file, &[])
}

fn build_tree_node(
    label: &str,
    entries: &BTreeMap<Vec<String>, Vec<String>>,
    prefix: &[String],
) -> Tree {
    let mut children: BTreeMap<String, Tree> = BTreeMap::new();

    for (segments, subtests) in entries {
        if !segments.starts_with(prefix) {
            continue;
        }
        match &segments[prefix.len()..] {
            [file] => {
                let child = if subtests.is_empty() {
                    Leaf(vec![file.clone()])
                } else {
                    let leaves = subtests.iter().map(|s| Leaf(vec![s.clone()])).collect();
                    Node(file.clone(), leaves)
                };
                children.insert(file.clone(), child);
            }
            [dir, ..] => {
                if !children.contains_key(dir.as_str()) {
                    let mut child_prefix = prefix.to_vec();
                    child_prefix.push(dir.clone());
                    children.insert(
                        dir.clone(),
                        build_tree_node(&format!("{dir}/"), entries, &child_prefix),
                    );
                }
            }
            [] => {}
        }
    }

    Node(label.to_string(), children.into_values().collect())
}

// RUN PROGRAM

pub fn print_verbose_is_not_supported_in_passthrough() {
    eprintln!(
        "{} `--verbose` is not supported in passthrough mode",
        error()
    );
    eprintln!(
        "{} You may want to use `--output-format toml` instead",
        hint()
    );
}

pub fn print_failed_to_run_program() {
    eprintln!("{} Failed to run program", error());
}

pub fn print_one_or_more_programs_failed_to_run() {
    eprintln!("{} One or more programs failed to run", error());
}

pub fn print_test_case_id_as_toml_comment(test_case: &TestCase) {
    println!("# TEST: {}", test_case.id());
}

pub fn print_failed_to_run_program_as_toml() {
    println!("# ERROR: Failed to run program");
}

pub fn print_output_as_toml(output: &ProgramOutput) {
    println!("expected_stdout = {}", format_toml_string(&output.stdout));
    println!("expected_stderr = {}", format_toml_string(&output.stderr));
    println!("expected_exit_code = {}", output.exit_code);
}

fn format_toml_string(s: &str) -> String {
    if s.contains('\n') {
        let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"\"\"\n{escaped}\"\"\"")
    } else {
        let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    }
}

// VALIDATION

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ReportValidateResult {
    ParseError,
    ValidationError(usize),
    Success(usize),
}

pub fn print_invalid_paths(paths: &[PathBuf]) {
    eprintln!("{} Invalid paths to config files:", warning());
    for path in paths {
        eprintln!("- {}", path.display());
    }
    eprintln!();
}

pub fn print_no_config_files() {
    eprintln!("{} No config files found for the given paths", error());
}

pub fn print_validate_table(entries: &BTreeMap<RelativePathBuf, ReportValidateResult>) {
    let max_len = entries
        .iter()
        .map(|(file, ..)| file.as_str().len())
        .max()
        .unwrap_or(0);

    for (file, result) in entries {
        let count_str = match result {
            ReportValidateResult::ParseError => String::from("Parse error"),
            ReportValidateResult::ValidationError(test_count)
            | ReportValidateResult::Success(test_count) => {
                if *test_count == 1 {
                    String::from("1 test")
                } else {
                    format!("{test_count} tests")
                }
            }
        };

        let line = format!(
            "{file:<width$}  {count_str}",
            file = file.as_str(),
            width = max_len,
        );

        let is_valid = matches!(result, ReportValidateResult::Success(_));
        if is_valid {
            println!("{} {line}", checkmark());
        } else {
            println!("{} {}", cross(), line.red());
        }
    }
}

pub fn print_config_files_found(config_file_paths: &[RelativePathBuf]) {
    let heading = format!("🔍 Found {} config files", config_file_paths.len());
    let tree = Node(
        heading,
        config_file_paths
            .iter()
            .map(|x| str_to_tree(x.as_ref()))
            .collect(),
    );

    print_tree(tree);
}

pub fn print_config_details(
    config_file_path: &RelativePath,
    test_entries: &BTreeMap<TestId, TestEntry>,
    requirement_data: &RequirementData,
    verbose: bool,
    hide_absolute_paths: bool,
) {
    let mut tests = vec![];

    for (test_id, test_entry) in test_entries {
        let mut categories = vec![];

        if verbose {
            // Program to run
            let program_to_run = match &test_entry.program_path {
                ProgramPath::NotSpecified => {
                    format!("{} {}", cross(), "Not specified".red())
                }
                ProgramPath::MissingProgram { requested_program } => {
                    format!("{} {}", cross(), requested_program.red())
                }
                ProgramPath::ResolvedPath {
                    requested_program: _,
                    resolved_path,
                } => {
                    let path = if hide_absolute_paths {
                        file::display_path(resolved_path)
                    } else {
                        resolved_path.display().to_string()
                    };
                    format!("{} {path}", checkmark())
                }
            };

            let nodes = vec![str_to_tree(&program_to_run)];

            let heading = String::from("Program to run");
            categories.push(Node(heading, nodes));

            // Requirements
            let requirements = requirements_map(&test_entry.requirements, requirement_data);
            if !requirements.is_empty() {
                let heading = String::from("Requirements");
                categories.push(Node(heading, requirements));
            }
        }

        // Validation errors
        let validation_errors = test_entry
            .test_case_with_expectations()
            .err()
            .unwrap_or_default();
        if !validation_errors.is_empty() {
            let nodes = validation_errors
                .iter()
                .map(|err| str_to_tree(&format_validation_error(err)))
                .collect();

            let heading = String::from("Validation errors");
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
            .map(|(test_id, children)| Node(format!(":{test_id}"), children))
            .collect()
    };

    let tree = Node(config_file_heading(config_file_path), nodes);

    print_tree(tree);
}

pub fn print_config_file_error(config_file_path: &RelativePath, error: &TomlConfigError) {
    let msg = match error {
        TomlConfigError::InvalidTomlSyntax(_) => "Failed to parse config file",
        TomlConfigError::ParseErrors(_) => "Failed to parse config file",
    };
    let tree = Node(
        config_file_heading(config_file_path),
        vec![str_to_tree(msg)],
    );

    print_tree(tree);
}

pub fn print_config_files_contain_errors() {
    eprintln!("{} Some config files contain errors (See above)", warning());
}

pub fn print_run_single_program_only(test_entry_count: usize) {
    eprintln!(
        "{} `--output-format passthrough` supports only a single test, but found {test_entry_count} tests",
        error(),
    );
    eprintln!(
        "{} Use `--output-format toml` to run multiple tests, or run the `list` command to list all tests",
        hint()
    );
}

// SUMMARY HELPERS

fn summary_print_start(number_of_tests: usize) {
    println!("🚀 Running {number_of_tests} tests:")
}

fn summary_print_test_case(result: &Result<TestResult, RunError>) -> Result<(), RunError> {
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

    io::Write::flush(&mut io::stdout()).map_err(RunError::IOError)?;

    Ok(())
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

    let message: String;
    if let Some(description) = &run_result.test_case.description {
        message = format!("{test_id} - {description}");
    } else {
        message = test_id;
    }

    if run_result.is_success() {
        println!("{} {message}", checkmark());
    } else {
        let nodes = match &run_result.result {
            Ok(result) => tree::nodes_from_test_result(result),
            Err(_) => {
                vec![Leaf(vec![String::from("Failed to run test")])]
            }
        };

        let test_heading = format!("❌ {message}");
        let tree = Node(test_heading, nodes);
        let content = tree::draw_tree(&tree).unwrap_or(String::from("Failed to draw tree\n"));
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
    let message: String;
    if let Some(description) = &test_case.description {
        message = format!("{} # {description}", test_case.id());
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

fn tap_print_summary() {
    // Do nothing
}

// OTHER HELPERS

fn print_tree(tree: Tree) {
    let content = tree::draw_tree(&tree).unwrap_or(String::from("Failed to draw tree\n"));

    eprint!("{content}"); // Already contains newline
    eprintln!()
}

fn config_file_heading(config_file_path: &RelativePath) -> String {
    format!("📋 {config_file_path}")
}

fn requirements_map(requirements: &Requirements, requirement_data: &RequirementData) -> Vec<Tree> {
    let mut categories = vec![];

    let format_requirement = |is_present: bool, text: &str| {
        if is_present {
            format!("{} {text}", checkmark())
        } else {
            format!("{} {}", cross(), text.red())
        }
    };

    if !requirements.files.is_empty() {
        categories.push(Node(
            String::from("Files"),
            requirements
                .files
                .iter()
                .map(|file| {
                    let is_present = requirement_data.get_file(file).is_some();
                    str_to_tree(&format_requirement(is_present, file))
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
                    str_to_tree(&format_requirement(is_present, env_var))
                })
                .collect(),
        ));
    }

    categories
}

fn format_validation_error(validation_error: &ValidationError) -> String {
    let msg = match validation_error {
        ValidationError::MissingExternalFile(file_path) => {
            format!("Missing external file '{file_path}'")
        }
        ValidationError::MissingEnvVar(var_name) => {
            format!("Missing environment variable '{var_name}'")
        }
        ValidationError::FailedToParseString => String::from("Failed to parse string"),
        ValidationError::ProgramRequired => String::from("The field 'program' is required"),
        ValidationError::ProgramNotFound(program) => {
            format!("The program '{program}' was not found")
        }
        ValidationError::ExpectationRequired => {
            String::from("At least one expectation is required")
        }
        ValidationError::InvalidExitCode => String::from(
            "Exit code must be a value between -2147483648 to 2147483647 (On POSIX/Unix/Linux: Between 0 and 255)",
        ),
    };

    format!("{} {}", cross(), msg.red())
}

fn str_to_tree(msg: &str) -> Tree {
    Leaf(vec![msg.to_owned()])
}

// SYMBOLS

fn checkmark() -> String {
    "✔".green().bold().to_string() // U+2714 HEAVY CHECK MARK
}

fn cross() -> String {
    "✘".red().bold().to_string() // U+2718 HEAVY BALLOT X
}

// LABELS

fn error() -> String {
    "error:".red().bold().to_string()
}

fn warning() -> String {
    "warning:".yellow().bold().to_string()
}

fn hint() -> String {
    "hint:".cyan().bold().to_string()
}
