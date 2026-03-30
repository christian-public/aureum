mod utils {
    pub mod file;
}
mod args;
mod config_file;

use crate::args::{CLI_BINARY_NAME, Command, ListArgs, TestArgs, TestOutputFormat, ValidateArgs};
use aureum::print_test_case;
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

    let cli = args::parse();
    match cli.command {
        Command::Validate(args) => {
            validate_config_files(current_dir, args);
        }
        Command::List(args) => {
            list_tests(current_dir, args);
        }
        Command::Test(args) => {
            run_tests(current_dir, args);
        }
        Command::Version => {
            print_version();
        }
    }
}

// COMMANDS

fn validate_config_files(current_dir: PathBuf, args: ValidateArgs) {
    let find_config_files_result = config_file::find_config_files(args.paths, &current_dir);

    if !find_config_files_result.errors.is_empty() {
        let paths = find_config_files_result
            .errors
            .into_iter()
            .map(|(path, _err)| path)
            .collect::<Vec<_>>();

        aureum::print_invalid_paths(paths);
    }

    if find_config_files_result.found_config_files.is_empty() {
        aureum::print_no_config_files();
        process::exit(INVALID_USER_INPUT_EXIT_CODE);
    }

    if args.common.verbose {
        let config_files = find_config_files_result
            .found_config_files
            .keys()
            .cloned()
            .collect::<Vec<_>>();

        aureum::print_files_found(&config_files);
    }

    let mut any_failed_configs = false;

    for (config_file, _test_id_coverage_set) in find_config_files_result.found_config_files {
        let Some(file_name) = config_file.file_name() else {
            // TODO: Show error
            continue;
        };

        let Some(path_to_containing_dir) = config_file.parent() else {
            // TODO: Show error
            continue;
        };

        let Ok(source) = fs::read_to_string(config_file.to_path(&current_dir)) else {
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
                if any_issues || args.common.verbose {
                    aureum::print_config_details(
                        config_file,
                        &parsed_toml_config,
                        &requirement_data,
                        args.common.verbose,
                        args.common.hide_absolute_paths,
                    );

                    if any_issues {
                        any_failed_configs = true;
                    }
                }
            }
            Err(error) => {
                aureum::print_toml_config_error(config_file, error);
                any_failed_configs = true;
            }
        }
    }

    if any_failed_configs {
        eprintln!("Some config files contain errors (See above)");
        process::exit(TEST_FAILURE_EXIT_CODE)
    } else {
        println!("All config files are valid")
    }
}

fn list_tests(current_dir: PathBuf, args: ListArgs) {
    let find_config_files_result = config_file::find_config_files(args.paths, &current_dir);

    if !find_config_files_result.errors.is_empty() {
        let paths = find_config_files_result
            .errors
            .into_iter()
            .map(|(path, _err)| path)
            .collect::<Vec<_>>();

        aureum::print_invalid_paths(paths);
    }

    if find_config_files_result.found_config_files.is_empty() {
        aureum::print_no_config_files();
        process::exit(INVALID_USER_INPUT_EXIT_CODE);
    }

    if args.common.verbose {
        let config_files = find_config_files_result
            .found_config_files
            .keys()
            .cloned()
            .collect::<Vec<_>>();

        aureum::print_files_found(&config_files);
    }

    let mut all_test_cases = vec![];
    let mut any_failed_configs = false;

    for (config_file, test_id_coverage_set) in find_config_files_result.found_config_files {
        let Some(file_name) = config_file.file_name() else {
            // TODO: Show error
            continue;
        };

        let Some(path_to_containing_dir) = config_file.parent() else {
            // TODO: Show error
            continue;
        };

        let Ok(source) = fs::read_to_string(config_file.to_path(&current_dir)) else {
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
                if any_issues || args.common.verbose {
                    aureum::print_config_details(
                        config_file,
                        &parsed_toml_config,
                        &requirement_data,
                        args.common.verbose,
                        args.common.hide_absolute_paths,
                    );

                    if any_issues {
                        any_failed_configs = true;
                    }
                }

                all_test_cases.extend(
                    parsed_toml_config
                        .into_values()
                        .filter_map(|x| x.test_cases.ok())
                        .filter(|x| test_id_coverage_set.contains(&x.test_id)),
                );
            }
            Err(error) => {
                aureum::print_toml_config_error(config_file, error);
                any_failed_configs = true;
            }
        }
    }

    for test_case in all_test_cases {
        println!("{}", test_case.id())
    }

    if any_failed_configs {
        eprintln!("Some config files contain errors (See above)");
        process::exit(TEST_FAILURE_EXIT_CODE)
    }
}

fn run_tests(current_dir: PathBuf, args: TestArgs) {
    let find_config_files_result = config_file::find_config_files(args.paths, &current_dir);

    if !find_config_files_result.errors.is_empty() {
        let paths = find_config_files_result
            .errors
            .into_iter()
            .map(|(path, _err)| path)
            .collect::<Vec<_>>();

        aureum::print_invalid_paths(paths);
    }

    if find_config_files_result.found_config_files.is_empty() {
        aureum::print_no_config_files();
        process::exit(INVALID_USER_INPUT_EXIT_CODE);
    }

    if args.common.verbose {
        let config_files = find_config_files_result
            .found_config_files
            .keys()
            .cloned()
            .collect::<Vec<_>>();

        aureum::print_files_found(&config_files);
    }

    let mut all_test_cases = vec![];
    let mut any_failed_configs = false;

    for (config_file, test_id_coverage_set) in find_config_files_result.found_config_files {
        let Some(file_name) = config_file.file_name() else {
            // TODO: Show error
            continue;
        };

        let Some(path_to_containing_dir) = config_file.parent() else {
            // TODO: Show error
            continue;
        };

        let Ok(source) = fs::read_to_string(config_file.to_path(&current_dir)) else {
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
                if any_issues || args.common.verbose {
                    aureum::print_config_details(
                        config_file,
                        &parsed_toml_config,
                        &requirement_data,
                        args.common.verbose,
                        args.common.hide_absolute_paths,
                    );

                    if any_issues {
                        any_failed_configs = true;
                    }
                }

                all_test_cases.extend(
                    parsed_toml_config
                        .into_values()
                        .filter_map(|x| x.test_cases.ok())
                        .filter(|x| test_id_coverage_set.contains(&x.test_id)),
                );
            }
            Err(error) => {
                aureum::print_toml_config_error(config_file, error);
                any_failed_configs = true;
            }
        }
    }

    let report_config = ReportConfig {
        number_of_tests: all_test_cases.len(),
        format: get_report_format(&args.output_format),
    };

    aureum::print_start_test_cases(&report_config);

    let run_results = aureum::run_test_cases(
        &report_config,
        &all_test_cases,
        args.run_tests_in_parallel,
        &current_dir,
        &print_test_case,
    );

    aureum::print_summary(&report_config, &run_results);

    if any_failed_configs {
        eprintln!("Some config files contain errors (See above)");
    }

    let all_tests_passed = run_results.iter().all(|t| t.is_success());

    if any_failed_configs || !all_tests_passed {
        process::exit(TEST_FAILURE_EXIT_CODE)
    }
}

fn print_version() {
    println!("{} {}", CLI_BINARY_NAME, env!("CARGO_PKG_VERSION"));
}

// HELPERS

fn get_report_format(output_format: &TestOutputFormat) -> ReportFormat {
    match output_format {
        TestOutputFormat::Summary => ReportFormat::Summary,
        TestOutputFormat::Tap => ReportFormat::Tap,
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
