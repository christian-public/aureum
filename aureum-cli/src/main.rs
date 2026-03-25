mod utils {
    pub mod file;
}
mod args;
mod config_file;
mod report;

use crate::args::{Cli, Command, ListArgs, OutputFormat, TestArgs};
use aureum::report_test_case;
use aureum::{ReportConfig, ReportFormat, RequirementData, Requirements};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use utils::file;

const TEST_FAILURE_EXIT_CODE: i32 = 1;
const INVALID_USER_INPUT_EXIT_CODE: i32 = 2;

fn main() {
    let current_dir = env::current_dir().expect("Current directory must be available");

    let cli: Cli = args::parse();
    match cli.command {
        Command::List(args) => {
            list_tests(current_dir, args);
        }
        Command::Test(args) => {
            run_tests(current_dir, args);
        }
    }
}

fn list_tests(current_dir: PathBuf, args: ListArgs) {
    let find_config_files_result = config_file::find_config_files(args.paths, &current_dir);

    let source_files = find_config_files_result
        .found_config_files
        .keys()
        .cloned()
        .collect::<Vec<_>>();

    if !find_config_files_result.errors.is_empty() {
        let paths = find_config_files_result
            .errors
            .into_iter()
            .map(|(path, _err)| path)
            .collect::<Vec<_>>();

        report::print_invalid_paths(paths);
    }

    if source_files.is_empty() {
        report::print_no_config_files();
        process::exit(INVALID_USER_INPUT_EXIT_CODE);
    }

    if args.verbose {
        report::print_files_found(&source_files);
    }

    let mut all_test_cases = vec![];
    let mut any_failed_configs = false;

    for source_file in source_files {
        let Some(file_name) = source_file.file_name() else {
            // TODO: Show error
            continue;
        };

        let Some(path_to_containing_dir) = source_file.parent() else {
            // TODO: Show error
            continue;
        };

        let Ok(source) = fs::read_to_string(source_file.to_path(&current_dir)) else {
            any_failed_configs = true; // TODO: Should save the error and display it
            continue;
        };

        match aureum::parse_toml_config(&source) {
            Ok(config) => {
                let requirements = aureum::get_requirements(&config.clone());
                let requirement_data = retrieve_requirement_data(
                    &path_to_containing_dir.to_path(&current_dir),
                    requirements,
                );

                let parsed_toml_config = aureum::build_test_cases(
                    path_to_containing_dir,
                    file_name,
                    config,
                    &requirement_data,
                    &|name, dir| file::find_executable_path(name, dir).ok(),
                );

                let any_issues = parsed_toml_config.values().any(|x| x.test_cases.is_err());
                if any_issues || args.verbose {
                    report::print_config_details(
                        source_file,
                        &parsed_toml_config,
                        &requirement_data,
                        args.verbose,
                        args.hide_absolute_paths,
                    );

                    if any_issues {
                        any_failed_configs = true;
                    }
                }

                all_test_cases.extend(
                    parsed_toml_config
                        .into_values()
                        .filter_map(|x| x.test_cases.ok()),
                );
            }
            Err(error) => {
                report::print_toml_config_error(source_file, error);
                any_failed_configs = true;
            }
        }
    }

    for test_case in all_test_cases {
        println!("{}", test_case.id())
    }

    if any_failed_configs {
        eprintln!("Some config files contain errors (See above)");
    }
}

fn run_tests(current_dir: PathBuf, args: TestArgs) {
    let find_config_files_result = config_file::find_config_files(args.paths, &current_dir);

    let source_files = find_config_files_result
        .found_config_files
        .keys()
        .cloned()
        .collect::<Vec<_>>();

    if !find_config_files_result.errors.is_empty() {
        let paths = find_config_files_result
            .errors
            .into_iter()
            .map(|(path, _err)| path)
            .collect::<Vec<_>>();

        report::print_invalid_paths(paths);
    }

    if source_files.is_empty() {
        report::print_no_config_files();
        process::exit(INVALID_USER_INPUT_EXIT_CODE);
    }

    if args.verbose {
        report::print_files_found(&source_files);
    }

    let mut all_test_cases = vec![];
    let mut any_failed_configs = false;

    for source_file in source_files {
        let Some(file_name) = source_file.file_name() else {
            // TODO: Show error
            continue;
        };

        let Some(path_to_containing_dir) = source_file.parent() else {
            // TODO: Show error
            continue;
        };

        let Ok(source) = fs::read_to_string(source_file.to_path(&current_dir)) else {
            any_failed_configs = true; // TODO: Should save the error and display it
            continue;
        };

        match aureum::parse_toml_config(&source) {
            Ok(config) => {
                let requirements = aureum::get_requirements(&config.clone());
                let requirement_data = retrieve_requirement_data(
                    &path_to_containing_dir.to_path(&current_dir),
                    requirements,
                );

                let parsed_toml_config = aureum::build_test_cases(
                    path_to_containing_dir,
                    file_name,
                    config,
                    &requirement_data,
                    &|name, dir| file::find_executable_path(name, dir).ok(),
                );

                let any_issues = parsed_toml_config.values().any(|x| x.test_cases.is_err());
                if any_issues || args.verbose {
                    report::print_config_details(
                        source_file,
                        &parsed_toml_config,
                        &requirement_data,
                        args.verbose,
                        args.hide_absolute_paths,
                    );

                    if any_issues {
                        any_failed_configs = true;
                    }
                }

                all_test_cases.extend(
                    parsed_toml_config
                        .into_values()
                        .filter_map(|x| x.test_cases.ok()),
                );
            }
            Err(error) => {
                report::print_toml_config_error(source_file, error);
                any_failed_configs = true;
            }
        }
    }

    let report_config = ReportConfig {
        number_of_tests: all_test_cases.len(),
        format: get_report_format(&args.output_format),
    };

    aureum::report_start(&report_config);

    let run_results = aureum::run_test_cases(
        &report_config,
        &all_test_cases,
        args.run_tests_in_parallel,
        &current_dir,
        &report_test_case,
    );

    aureum::report_summary(&report_config, &run_results);

    if any_failed_configs {
        eprintln!("Some config files contain errors (See above)");
    }

    let all_tests_passed = run_results.iter().all(|t| t.is_success());

    if any_failed_configs || !all_tests_passed {
        process::exit(TEST_FAILURE_EXIT_CODE)
    }
}

fn get_report_format(output_format: &OutputFormat) -> ReportFormat {
    match output_format {
        OutputFormat::Summary => ReportFormat::Summary,
        OutputFormat::Tap => ReportFormat::Tap,
    }
}

fn retrieve_requirement_data(current_dir: &Path, requirements: Requirements) -> RequirementData {
    let mut requirement_data = RequirementData::default();

    for file in requirements.files {
        let Some(value) = read_external_file(current_dir, &file).ok() else {
            continue;
        };

        requirement_data.files.insert(file, value);
    }

    for env_var in requirements.env_vars {
        let Some(value) = env::var(&env_var).ok() else {
            continue;
        };

        requirement_data.env_vars.insert(env_var, value);
    }

    requirement_data
}

fn read_external_file(current_dir: &Path, file_path: &String) -> io::Result<String> {
    let path = current_dir.join(file_path);
    fs::read_to_string(path)
}
