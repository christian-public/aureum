mod utils {
    pub mod file;
}
mod args;
mod exit_code;
mod find_config_file;
mod load_config_file;

use crate::args::{
    CLI_BINARY_NAME, Command, ListArgs, RunArgs, RunOutputFormat, TestArgs, TestOutputFormat,
    ValidateArgs,
};
use crate::exit_code::ExitCode;
use crate::load_config_file::{ConfigFileError, LoadConfigFilesResult, LoadedConfigFile};
use aureum::{ReportConfig, ReportFormat, ReportValidateResult};
use std::env;
use std::path::{Path, PathBuf};
use std::process;

fn main() {
    let current_dir = env::current_dir().expect("Current directory must be available");

    let cli = args::parse();
    let exit_code = match cli.command {
        Command::Validate(args) => validate_config_files(args, &current_dir),
        Command::List(args) => list_tests(args, &current_dir),
        Command::Run(args) => run_programs(args, &current_dir),
        Command::Test(args) => run_tests(args, &current_dir),
        Command::Version => print_version(),
    };

    let code = exit_code.to_i32();
    if code != 0 {
        process::exit(code);
    }
}

// COMMANDS

fn validate_config_files(args: ValidateArgs, current_dir: &Path) -> ExitCode {
    let config_files = match prepare_config_files(args.paths, args.common.verbose, current_dir) {
        Ok(result) => result,
        Err(err) => return err,
    };

    for (config_file_path, loaded_config_file) in &config_files.loaded {
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

    let has_config_errors = config_files.has_config_errors();

    let table_entries =
        config_files
            .loaded
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
            .chain(config_files.invalid.keys().map(|config_file_path| {
                (config_file_path.clone(), ReportValidateResult::ParseError)
            }))
            .collect();

    aureum::print_validate_table(&table_entries);

    if has_config_errors {
        aureum::print_config_files_contain_errors();

        ExitCode::InvalidConfig
    } else {
        ExitCode::Success
    }
}

fn list_tests(args: ListArgs, current_dir: &Path) -> ExitCode {
    let config_files = match prepare_config_files(args.paths, args.common.verbose, current_dir) {
        Ok(result) => result,
        Err(err) => return err,
    };

    for (config_file_path, loaded_config_file) in &config_files.loaded {
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

    let test_entries_in_coverage_set = config_files
        .loaded
        .values()
        .flat_map(|x| x.test_entries_in_coverage_set())
        .collect::<Vec<_>>();

    let all_test_cases = test_entries_in_coverage_set
        .iter()
        .flat_map(|(_test_id, test_entry)| test_entry.test_case.as_ref().ok()) // This line is different than in `run_tests()`
        .collect::<Vec<_>>();

    let has_config_errors = config_files.has_config_errors();

    for test_case in all_test_cases {
        println!("{}", test_case.id())
    }

    if has_config_errors {
        aureum::print_config_files_contain_errors();

        ExitCode::InvalidConfig
    } else {
        ExitCode::Success
    }
}

fn run_programs(args: RunArgs, current_dir: &Path) -> ExitCode {
    let config_files = match prepare_config_files(args.paths, args.common.verbose, current_dir) {
        Ok(result) => result,
        Err(err) => return err,
    };

    let test_entries_in_coverage_set = config_files
        .loaded
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

    for (config_file_path, loaded_config_file) in &config_files.loaded {
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

    let has_config_errors =
        (config_files.has_config_errors()) && !passthrough_with_single_test_entry;

    let mut any_programs_failed_to_run = false;

    match args.output_format {
        RunOutputFormat::Passthrough => {
            if has_config_errors {
                aureum::print_config_files_contain_errors();
                return ExitCode::InvalidConfig;
            }

            match &all_test_cases[..] {
                [test_case] => match aureum::run_program_passthrough(test_case, current_dir) {
                    Ok(exit_code) => {
                        return ExitCode::Passthrough(exit_code);
                    }
                    Err(_) => {
                        aureum::print_failed_to_run_program();
                        return ExitCode::RunProgramFailure;
                    }
                },
                _ => {
                    aureum::print_run_single_program_only(all_test_cases.len());
                    return ExitCode::InvalidUsage;
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

        ExitCode::RunProgramFailure
    } else if has_config_errors {
        aureum::print_config_files_contain_errors();

        ExitCode::InvalidConfig
    } else {
        ExitCode::Success
    }
}

fn run_tests(args: TestArgs, current_dir: &Path) -> ExitCode {
    let config_files = match prepare_config_files(args.paths, args.common.verbose, current_dir) {
        Ok(result) => result,
        Err(err) => return err,
    };

    for (config_file_path, loaded_config_file) in &config_files.loaded {
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

    let test_entries_in_coverage_set = config_files
        .loaded
        .values()
        .flat_map(|x| x.test_entries_in_coverage_set())
        .collect::<Vec<_>>();

    let all_test_cases = test_entries_in_coverage_set
        .iter()
        .flat_map(|(_test_id, test_entry)| test_entry.test_case_with_expectations().ok())
        .collect::<Vec<_>>();

    let has_config_errors = config_files.has_config_errors();

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

    if has_config_errors {
        aureum::print_config_files_contain_errors();
    }

    let all_tests_passed = run_results.iter().all(|t| t.is_success());

    if !all_tests_passed {
        ExitCode::TestFailure
    } else if has_config_errors {
        ExitCode::InvalidConfig
    } else {
        ExitCode::Success
    }
}

fn print_version() -> ExitCode {
    println!("{} {}", CLI_BINARY_NAME, env!("CARGO_PKG_VERSION"));

    ExitCode::Success
}

// HELPERS

fn prepare_config_files(
    paths: Vec<PathBuf>,
    verbose: bool,
    current_dir: &Path,
) -> Result<LoadConfigFilesResult, ExitCode> {
    let find_config_files_result = find_config_file::find_config_files(paths, current_dir);

    let had_find_errors = !find_config_files_result.errors.is_empty();
    if had_find_errors {
        let paths = find_config_files_result
            .errors
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        aureum::print_invalid_paths(&paths);
    }

    if find_config_files_result.found.is_empty() {
        aureum::print_no_config_files();
        return Err(ExitCode::InvalidConfig);
    }

    if verbose {
        let config_files = find_config_files_result
            .found
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        aureum::print_config_files_found(&config_files);
    }

    let load_config_files_result =
        load_config_file::load_config_files(find_config_files_result, current_dir);

    for (config_file_path, config_file_error) in &load_config_files_result.invalid {
        match config_file_error {
            ConfigFileError::ParseFailed(err) => {
                aureum::print_config_file_error(config_file_path, err);
            }
            _ => {
                // TODO: Handle other errors
            }
        }
    }

    Ok(load_config_files_result)
}

fn get_report_format(output_format: &TestOutputFormat) -> ReportFormat {
    match output_format {
        TestOutputFormat::Summary => ReportFormat::Summary,
        TestOutputFormat::Tap => ReportFormat::Tap,
    }
}
