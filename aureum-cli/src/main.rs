mod utils {
    pub mod file;
}
mod args;
mod find_config_file;

use crate::args::{
    CLI_BINARY_NAME, Command, ListArgs, RunArgs, RunOutputFormat, TestArgs, TestOutputFormat,
    ValidateArgs,
};
use aureum::{
    ReportConfig, ReportFormat, ReportValidateResult, RequirementData, Requirements, TestEntry,
    TestId, TestIdCoverageSet,
};
use itertools::{Either, Itertools};
use relative_path::RelativePathBuf;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use utils::file;

const TEST_FAILURE_EXIT_CODE: i32 = 1;
const INVALID_CLI_USAGE_EXIT_CODE: i32 = 2; // Matches clap's behavior
const INVALID_CONFIG_EXIT_CODE: i32 = 3;

#[cfg_attr(debug_assertions, derive(Debug))]
struct LoadedConfigFile {
    test_id_coverage_set: TestIdCoverageSet,
    requirement_data: RequirementData,
    test_entries: BTreeMap<TestId, TestEntry>,
}

#[cfg_attr(debug_assertions, derive(Debug))]
#[allow(dead_code)]
enum ConfigFileError {
    NoFileName,
    NoParentDirectory,
    ReadFailed(io::Error),
    ParseFailed(aureum::TomlConfigError),
}

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
        load_config_files(found_config_files, current_dir);

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

    let any_validation_errors = loaded_config_files.iter().any(|(_, loaded_config_file)| {
        loaded_config_file
            .test_entries
            .iter()
            .any(|(_test_id, test_entry)| test_entry.has_validation_error())
    });

    for (config_file_path, loaded_config_file) in &loaded_config_files {
        let any_issues = loaded_config_file
            .test_entries
            .iter()
            .any(|(_test_id, test_entry)| test_entry.has_validation_error());

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
        load_config_files(found_config_files, current_dir);

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

    let any_validation_errors = loaded_config_files.iter().any(|(_, loaded_config_file)| {
        loaded_config_file
            .test_entries
            .iter()
            .any(|(_test_id, test_entry)| test_entry.has_validation_error())
    });

    for (config_file_path, loaded_config_file) in &loaded_config_files {
        let any_issues = loaded_config_file
            .test_entries
            .iter()
            .any(|(_test_id, test_entry)| test_entry.has_validation_error());

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

    let all_test_entries = loaded_config_files
        .iter()
        .flat_map(|(_, loaded_config_file)| {
            loaded_config_file
                .test_entries
                .iter()
                .filter(|(test_id, _test_entry)| {
                    loaded_config_file.test_id_coverage_set.contains(test_id)
                })
        })
        .collect::<Vec<_>>();

    let all_test_cases = all_test_entries
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
        load_config_files(found_config_files, current_dir);

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

    let any_validation_errors = loaded_config_files.iter().any(|(_, loaded_config_file)| {
        loaded_config_file
            .test_entries
            .iter()
            .any(|(_test_id, test_entry)| test_entry.has_validation_error())
    });

    let all_test_entries = loaded_config_files
        .iter()
        .flat_map(|(_, loaded_config_file)| {
            loaded_config_file
                .test_entries
                .iter()
                .filter(|(test_id, _test_entry)| {
                    loaded_config_file.test_id_coverage_set.contains(test_id)
                })
        })
        .collect::<Vec<_>>();

    let all_test_cases = all_test_entries
        .iter()
        .flat_map(|(_test_id, test_entry)| test_entry.test_case.as_ref().ok()) // This line is different than in `run_tests()`
        .collect::<Vec<_>>();

    let passthrough_with_single_test_entry =
        matches!(args.output_format, RunOutputFormat::Passthrough) && all_test_entries.len() == 1;

    for (config_file_path, loaded_config_file) in &loaded_config_files {
        let any_issues = loaded_config_file
            .test_entries
            .iter()
            .any(|(_test_id, test_entry)| test_entry.has_validation_error());

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
        load_config_files(found_config_files, current_dir);

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

    let any_validation_errors = loaded_config_files.iter().any(|(_, loaded_config_file)| {
        loaded_config_file
            .test_entries
            .iter()
            .any(|(_test_id, test_entry)| test_entry.has_validation_error())
    });

    for (config_file_path, loaded_config_file) in &loaded_config_files {
        let any_issues = loaded_config_file
            .test_entries
            .iter()
            .any(|(_test_id, test_entry)| test_entry.has_validation_error());

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

    let all_test_entries = loaded_config_files
        .iter()
        .flat_map(|(_, loaded_config_file)| {
            loaded_config_file
                .test_entries
                .iter()
                .filter(|(test_id, _test_entry)| {
                    loaded_config_file.test_id_coverage_set.contains(test_id)
                })
        })
        .collect::<Vec<_>>();

    let all_test_cases = all_test_entries
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

fn load_config_files(
    found_config_files: BTreeMap<RelativePathBuf, TestIdCoverageSet>,
    current_dir: &Path,
) -> (
    BTreeMap<RelativePathBuf, LoadedConfigFile>,
    BTreeMap<RelativePathBuf, ConfigFileError>,
) {
    found_config_files
        .into_iter()
        .partition_map(|(config_file_path, test_id_coverage_set)| {
            let result =
                load_config_file(config_file_path.clone(), test_id_coverage_set, current_dir);
            match result {
                Ok(loaded) => Either::Left((config_file_path, loaded)),
                Err(err) => Either::Right((config_file_path, err)),
            }
        })
}

fn load_config_file(
    config_file_path: RelativePathBuf,
    test_id_coverage_set: TestIdCoverageSet,
    current_dir: &Path,
) -> Result<LoadedConfigFile, ConfigFileError> {
    let file_name = config_file_path
        .file_name()
        .ok_or(ConfigFileError::NoFileName)?;

    let path_to_containing_dir = config_file_path
        .parent()
        .ok_or(ConfigFileError::NoParentDirectory)?;

    let source = fs::read_to_string(config_file_path.to_path(current_dir))
        .map_err(ConfigFileError::ReadFailed)?;

    let config = aureum::parse_toml_config(&source).map_err(ConfigFileError::ParseFailed)?;

    let requirements = aureum::get_requirements(&config);
    let requirement_data =
        retrieve_requirement_data(&path_to_containing_dir.to_path(current_dir), requirements);

    let test_entries = aureum::build_test_entries(
        config,
        path_to_containing_dir,
        file_name,
        &requirement_data,
        &|name, dir| file::find_executable_path(name, dir).ok(),
    );

    Ok(LoadedConfigFile {
        test_id_coverage_set,
        requirement_data,
        test_entries,
    })
}

fn get_report_format(output_format: &TestOutputFormat) -> ReportFormat {
    match output_format {
        TestOutputFormat::Summary => ReportFormat::Summary,
        TestOutputFormat::Tap => ReportFormat::Tap,
    }
}

fn retrieve_requirement_data(current_dir: &Path, requirements: Requirements) -> RequirementData {
    let mut requirement_data = RequirementData::default();

    for file in requirements.files {
        let path = current_dir.join(&file);
        let Ok(value) = fs::read_to_string(path) else {
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
