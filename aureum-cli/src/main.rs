mod utils {
    pub mod file;
}
mod args;
mod find_config_file;
mod load_config_file;

use crate::args::{
    CLI_BINARY_NAME, Command, ListArgs, RunArgs, RunOutputFormat, TestArgs, TestOutputFormat,
    ValidateArgs,
};
use crate::load_config_file::{ConfigFileError, LoadedConfigFile};
use aureum::{ReportConfig, ReportFormat, ReportValidateResult, TestIdCoverageSet};
use relative_path::RelativePathBuf;
use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process;

const TEST_FAILURE_EXIT_CODE: i32 = 1;
const INVALID_CLI_USAGE_EXIT_CODE: i32 = 2; // Matches clap's behavior
const INVALID_CONFIG_EXIT_CODE: i32 = 3;

fn main() {
    let current_dir = env::current_dir().expect("Current directory must be available");

    let cli = args::parse();
    match cli.command {
        Command::Validate(args) => {
            validate_config_files(args, &current_dir);
        }
        Command::List(args) => {
            list_tests(args, &current_dir);
        }
        Command::Run(args) => {
            run_programs(args, &current_dir);
        }
        Command::Test(args) => {
            run_tests(args, &current_dir);
        }
        Command::Version => {
            print_version();
        }
    }
}

// COMMANDS

fn validate_config_files(args: ValidateArgs, current_dir: &Path) {
    let found_config_files = find_and_validate_config_files(args.paths, current_dir);

    if found_config_files.is_empty() {
        aureum::print_no_config_files();
        process::exit(INVALID_CLI_USAGE_EXIT_CODE);
    }

    if args.common.verbose {
        let config_files = found_config_files.keys().cloned().collect::<Vec<_>>();

        aureum::print_config_files_found(&config_files);
    }

    let (loaded_config_files, invalid_config_files) =
        load_config_file::load_config_files(found_config_files, current_dir);

    let table_entries =
        loaded_config_files
            .iter()
            .map(
                |(config_file_path, LoadedConfigFile { test_entries, .. })| {
                    let is_valid = test_entries.values().all(|x| x.is_testable());
                    let validate_result = if is_valid {
                        ReportValidateResult::Success(test_entries.len())
                    } else {
                        ReportValidateResult::ValidationError(test_entries.len())
                    };

                    (config_file_path.clone(), validate_result)
                },
            )
            .chain(invalid_config_files.keys().map(|config_file_path| {
                (config_file_path.clone(), ReportValidateResult::ParseError)
            }))
            .collect();

    for (config_file_path, config_file_error) in &invalid_config_files {
        match config_file_error {
            ConfigFileError::ParseFailed(err) => {
                aureum::print_config_file_error(config_file_path, err);
            }
            _ => {
                // TODO: Handle other errors
            }
        }
    }

    let any_validation_errors = loaded_config_files
        .values()
        .any(|x| x.has_validation_errors());

    for (config_file_path, loaded_config_file) in &loaded_config_files {
        let any_issues = loaded_config_file.has_validation_errors();

        if any_issues || args.common.verbose {
            aureum::print_config_details(
                config_file_path,
                &loaded_config_file.test_entries,
                &loaded_config_file.requirement_data,
                args.common.verbose,
                args.common.hide_absolute_paths,
            );
        }
    }

    let any_failed_configs = !invalid_config_files.is_empty() || any_validation_errors;

    aureum::print_validate_table(&table_entries);

    if any_failed_configs {
        aureum::print_config_files_contain_errors();
        process::exit(INVALID_CONFIG_EXIT_CODE);
    }
}

fn list_tests(args: ListArgs, current_dir: &Path) {
    let found_config_files = find_and_validate_config_files(args.paths, current_dir);

    if found_config_files.is_empty() {
        aureum::print_no_config_files();
        process::exit(INVALID_CLI_USAGE_EXIT_CODE);
    }

    if args.common.verbose {
        let config_files = found_config_files.keys().cloned().collect::<Vec<_>>();

        aureum::print_config_files_found(&config_files);
    }

    let (loaded_config_files, invalid_config_files) =
        load_config_file::load_config_files(found_config_files, current_dir);

    for (config_file_path, config_file_error) in &invalid_config_files {
        match config_file_error {
            ConfigFileError::ParseFailed(err) => {
                aureum::print_config_file_error(config_file_path, err);
            }
            _ => {
                // TODO: Handle other errors
            }
        }
    }

    let any_validation_errors = loaded_config_files
        .values()
        .any(|x| x.has_validation_errors());

    for (config_file_path, loaded_config_file) in &loaded_config_files {
        let any_issues = loaded_config_file.has_validation_errors();

        if any_issues || args.common.verbose {
            aureum::print_config_details(
                config_file_path,
                &loaded_config_file.test_entries,
                &loaded_config_file.requirement_data,
                args.common.verbose,
                args.common.hide_absolute_paths,
            );
        }
    }

    let test_entries_in_coverage_set = loaded_config_files
        .values()
        .flat_map(|x| x.test_entries_in_coverage_set())
        .collect::<Vec<_>>();

    let all_test_cases = test_entries_in_coverage_set
        .iter()
        .flat_map(|(_test_id, test_entry)| test_entry.test_case.as_ref().ok()) // This line is different than in `run_tests()`
        .collect::<Vec<_>>();

    let any_failed_configs = !invalid_config_files.is_empty() || any_validation_errors;

    for test_case in all_test_cases {
        println!("{}", test_case.id())
    }

    if any_failed_configs {
        aureum::print_config_files_contain_errors();
        process::exit(INVALID_CONFIG_EXIT_CODE);
    }
}

fn run_programs(args: RunArgs, current_dir: &Path) {
    let found_config_files = find_and_validate_config_files(args.paths, current_dir);

    if found_config_files.is_empty() {
        aureum::print_no_config_files();
        process::exit(INVALID_CLI_USAGE_EXIT_CODE);
    }

    if args.common.verbose {
        let config_files = found_config_files.keys().cloned().collect::<Vec<_>>();

        aureum::print_config_files_found(&config_files);
    }

    let (loaded_config_files, invalid_config_files) =
        load_config_file::load_config_files(found_config_files, current_dir);

    for (config_file_path, config_file_error) in &invalid_config_files {
        match config_file_error {
            ConfigFileError::ParseFailed(err) => {
                aureum::print_config_file_error(config_file_path, err);
            }
            _ => {
                // TODO: Handle other errors
            }
        }
    }

    let any_validation_errors = loaded_config_files
        .values()
        .any(|x| x.has_validation_errors());

    let test_entries_in_coverage_set = loaded_config_files
        .values()
        .flat_map(|x| x.test_entries_in_coverage_set())
        .collect::<Vec<_>>();

    let all_test_cases = test_entries_in_coverage_set
        .iter()
        .flat_map(|(_test_id, test_entry)| test_entry.test_case.as_ref().ok()) // This line is different than in `run_tests()`
        .collect::<Vec<_>>();

    let passthrough_with_single_test_entry =
        matches!(args.output_format, RunOutputFormat::Passthrough)
            && test_entries_in_coverage_set.len() == 1;

    for (config_file_path, loaded_config_file) in &loaded_config_files {
        let any_issues = loaded_config_file.has_validation_errors();

        if (any_issues || args.common.verbose) && !passthrough_with_single_test_entry {
            aureum::print_config_details(
                config_file_path,
                &loaded_config_file.test_entries,
                &loaded_config_file.requirement_data,
                args.common.verbose,
                args.common.hide_absolute_paths,
            );
        }
    }

    let any_failed_configs = (!invalid_config_files.is_empty() || any_validation_errors)
        && !passthrough_with_single_test_entry;

    let mut any_programs_failed_to_run = false;

    match args.output_format {
        RunOutputFormat::Passthrough => {
            if any_failed_configs {
                aureum::print_config_files_contain_errors();
                process::exit(INVALID_CONFIG_EXIT_CODE);
            }

            match &all_test_cases[..] {
                [test_case] => match aureum::run_program_passthrough(test_case, current_dir) {
                    Ok(exit_code) => {
                        process::exit(exit_code);
                    }
                    Err(_) => {
                        aureum::print_failed_to_run_program();
                        process::exit(TEST_FAILURE_EXIT_CODE);
                    }
                },
                _ => {
                    aureum::print_run_single_program_only(all_test_cases.len());
                    process::exit(INVALID_CLI_USAGE_EXIT_CODE);
                }
            }
        }
        RunOutputFormat::Toml => {
            for (index, test_case) in all_test_cases.iter().enumerate() {
                if index > 0 {
                    println!(); // Print extra newline between test cases
                }

                aureum::print_test_case_id_as_toml_comment(test_case);

                match aureum::run_program(test_case, current_dir) {
                    Ok(output) => {
                        aureum::print_output_as_toml(&output);
                    }
                    Err(_) => {
                        aureum::print_failed_to_run_program_as_toml();
                        any_programs_failed_to_run = true;
                    }
                }
            }
        }
    }

    if any_programs_failed_to_run {
        aureum::print_one_or_more_programs_failed_to_run();
        process::exit(TEST_FAILURE_EXIT_CODE);
    } else if any_failed_configs {
        aureum::print_config_files_contain_errors();
        process::exit(INVALID_CONFIG_EXIT_CODE);
    }
}

fn run_tests(args: TestArgs, current_dir: &Path) {
    let found_config_files = find_and_validate_config_files(args.paths, current_dir);

    if found_config_files.is_empty() {
        aureum::print_no_config_files();
        process::exit(INVALID_CLI_USAGE_EXIT_CODE);
    }

    if args.common.verbose {
        let config_files = found_config_files.keys().cloned().collect::<Vec<_>>();

        aureum::print_config_files_found(&config_files);
    }

    let (loaded_config_files, invalid_config_files) =
        load_config_file::load_config_files(found_config_files, current_dir);

    for (config_file_path, config_file_error) in &invalid_config_files {
        match config_file_error {
            ConfigFileError::ParseFailed(err) => {
                aureum::print_config_file_error(config_file_path, err);
            }
            _ => {
                // TODO: Handle other errors
            }
        }
    }

    let any_validation_errors = loaded_config_files
        .values()
        .any(|x| x.has_validation_errors());

    for (config_file_path, loaded_config_file) in &loaded_config_files {
        let any_issues = loaded_config_file.has_validation_errors();

        if any_issues || args.common.verbose {
            aureum::print_config_details(
                config_file_path,
                &loaded_config_file.test_entries,
                &loaded_config_file.requirement_data,
                args.common.verbose,
                args.common.hide_absolute_paths,
            );
        }
    }

    let test_entries_in_coverage_set = loaded_config_files
        .values()
        .flat_map(|x| x.test_entries_in_coverage_set())
        .collect::<Vec<_>>();

    let all_test_cases = test_entries_in_coverage_set
        .iter()
        .flat_map(|(_test_id, test_entry)| test_entry.test_case_with_expectations().ok())
        .collect::<Vec<_>>();

    let any_failed_configs = !invalid_config_files.is_empty() || any_validation_errors;

    let report_config = ReportConfig {
        number_of_tests: all_test_cases.len(),
        format: get_report_format(&args.output_format),
    };

    aureum::print_start_test_cases(&report_config);

    let run_results = aureum::run_test_cases(
        &report_config,
        &all_test_cases,
        args.parallel,
        current_dir,
        &aureum::print_test_case,
    );

    aureum::print_summary(&report_config, &run_results);

    if any_failed_configs {
        aureum::print_config_files_contain_errors();
    }

    let all_tests_passed = run_results.iter().all(|t| t.is_success());

    if !all_tests_passed {
        process::exit(TEST_FAILURE_EXIT_CODE);
    } else if any_failed_configs {
        process::exit(INVALID_CONFIG_EXIT_CODE);
    }
}

fn print_version() {
    println!("{} {}", CLI_BINARY_NAME, env!("CARGO_PKG_VERSION"));
}

// HELPERS

fn find_and_validate_config_files(
    paths: Vec<PathBuf>,
    current_dir: &Path,
) -> BTreeMap<RelativePathBuf, TestIdCoverageSet> {
    let result = find_config_file::find_config_files(paths, current_dir);

    if !result.errors.is_empty() {
        let paths = result.errors.into_keys().collect::<Vec<_>>();
        aureum::print_invalid_paths(paths);
    }

    result.found_config_files
}

fn get_report_format(output_format: &TestOutputFormat) -> ReportFormat {
    match output_format {
        TestOutputFormat::Summary => ReportFormat::Summary,
        TestOutputFormat::Tap => ReportFormat::Tap,
    }
}
