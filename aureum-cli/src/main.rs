mod args;
mod file;
mod report;
mod test_path;

use crate::args::{Cli, Command, ListArgs, OutputFormat, TestArgs};
use aureum::{ReportConfig, ReportFormat};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::exit;

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
    let source_files = file::expand_test_paths(&args.paths, &current_dir)
        .keys()
        .cloned()
        .collect::<Vec<_>>();

    if source_files.is_empty() {
        report::print_no_config_files();
        exit(INVALID_USER_INPUT_EXIT_CODE);
    }

    if args.verbose {
        report::print_files_found(&source_files);
    }

    let mut all_test_cases = vec![];
    let mut any_failed_configs = false;

    for source_file in source_files {
        let source_path = source_file.to_logical_path(".");

        let Ok(source) = fs::read_to_string(source_path) else {
            any_failed_configs = true; // TODO: Should save the error and display it
            continue;
        };

        match aureum::parse_toml_config(&source_file, &source) {
            Ok(config) => {
                let any_issues = report::any_issues_in_toml_config(&config);
                if any_issues || args.verbose {
                    report::print_config_details(
                        source_file,
                        &config,
                        args.verbose,
                        args.hide_absolute_paths,
                    );

                    if any_issues {
                        any_failed_configs = true;
                    }
                }

                all_test_cases.extend(config.tests.into_values().filter_map(|x| x.test_case.ok()));
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
    let source_files = file::expand_test_paths(&args.paths, &current_dir)
        .keys()
        .cloned()
        .collect::<Vec<_>>();

    if source_files.is_empty() {
        report::print_no_config_files();
        exit(INVALID_USER_INPUT_EXIT_CODE);
    }

    if args.verbose {
        report::print_files_found(&source_files);
    }

    let mut all_test_cases = vec![];
    let mut any_failed_configs = false;

    for source_file in source_files {
        let source_path = source_file.to_logical_path(".");

        let Ok(source) = fs::read_to_string(source_path) else {
            any_failed_configs = true; // TODO: Should save the error and display it
            continue;
        };

        match aureum::parse_toml_config(&source_file, &source) {
            Ok(config) => {
                let any_issues = report::any_issues_in_toml_config(&config);
                if any_issues || args.verbose {
                    report::print_config_details(
                        source_file,
                        &config,
                        args.verbose,
                        args.hide_absolute_paths,
                    );

                    if any_issues {
                        any_failed_configs = true;
                    }
                }

                all_test_cases.extend(config.tests.into_values().filter_map(|x| x.test_case.ok()));
            }
            Err(error) => {
                report::print_toml_config_error(source_file, error);
                any_failed_configs = true;
            }
        }
    }

    let report_config = ReportConfig {
        number_of_tests: all_test_cases.len(),
        format: get_report_format(&args),
    };

    let run_results =
        aureum::run_test_cases(&report_config, &all_test_cases, args.run_tests_in_parallel);

    if any_failed_configs {
        eprintln!("Some config files contain errors (See above)");
    }

    let all_tests_passed = run_results.iter().all(|t| t.is_success());

    if any_failed_configs || !all_tests_passed {
        exit(TEST_FAILURE_EXIT_CODE)
    }
}

fn get_report_format(args: &TestArgs) -> ReportFormat {
    match args.output_format {
        OutputFormat::Summary => ReportFormat::Summary,
        OutputFormat::Tap => ReportFormat::Tap,
    }
}
