use crate::load_config_file::ConfigFileError;
use crate::report::label;
use crate::report::symbol;
use crate::utils::file;
use crate::vendor::ascii_tree::Tree::{self, Leaf, Node};
use aureum::{ProgramPath, RequirementData, Requirements, TestEntry, TestId, ValidationError};
use colored::Colorize;
use relative_path::{RelativePath, RelativePathBuf};
use std::collections::BTreeMap;
use std::path::PathBuf;

// VALIDATION

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum ReportValidateResult {
    ParseError,
    ValidationError(usize),
    Success(usize),
}

pub fn print_invalid_paths(paths: &[PathBuf]) {
    eprintln!("{} Invalid paths to config files:", label::warning());
    for path in paths {
        eprintln!("- {}", path.display());
    }
    eprintln!();
}

pub fn print_no_config_files() {
    eprintln!(
        "{} No config files found for the given paths",
        label::error()
    );
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
            println!("{} {line}", symbol::checkmark());
        } else {
            println!("{} {}", symbol::cross(), line.red());
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
                    format!("{} {}", symbol::cross(), "Not specified".red())
                }
                ProgramPath::MissingProgram { requested_program } => {
                    format!("{} {}", symbol::cross(), requested_program.red())
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
                    format!("{} {path}", symbol::checkmark())
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

pub fn print_config_file_error(config_file_path: &RelativePath, error: &ConfigFileError) {
    let msg = match error {
        ConfigFileError::NoFileName => "Config file path has no filename",
        ConfigFileError::NoParentDirectory => "Config file path has no parent directory",
        ConfigFileError::ReadFailed(_) => "Failed to read config file",
        ConfigFileError::ParseFailed(_) => "Failed to parse config file",
    };
    let tree = Node(
        config_file_heading(config_file_path),
        vec![str_to_tree(msg)],
    );

    print_tree(tree);
}

pub fn print_config_files_contain_errors() {
    eprintln!(
        "{} Some config files contain errors (See above)",
        label::warning()
    );
}

pub fn print_run_single_program_only(test_entry_count: usize) {
    eprintln!(
        "{} `--format passthrough` supports only a single test, but found {test_entry_count} tests",
        label::error(),
    );
    eprintln!(
        "{} Use `--format toml` to run multiple tests, or run the `list` command to list all tests",
        label::hint()
    );
}

// OTHER HELPERS

fn print_tree(tree: Tree) {
    let content = tree.to_string();

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
            format!("{} {text}", symbol::checkmark())
        } else {
            format!("{} {}", symbol::cross(), text.red())
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

    format!("{} {}", symbol::cross(), msg.red())
}

fn str_to_tree(msg: &str) -> Tree {
    Leaf(vec![msg.to_owned()])
}
