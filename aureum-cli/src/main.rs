mod report;
mod vendor;
mod utils {
    pub mod file;
    pub mod shell;
}
mod args;
mod exit_code;
mod find_config_file;
mod interactive;
mod load_config_file;
mod template;

use crate::args::{
    CLI_BINARY_NAME, Command, InitArgs, ListArgs, RunArgs, RunOutputFormat, TerminalSize, TestArgs,
    TestOutputFormat, ValidateArgs,
};
use crate::exit_code::ExitCode;
use crate::load_config_file::{LoadConfigFilesResult, LoadedConfigFile};
use crate::report::test_case::{ReportConfig, ReportFormat};
use crate::report::validate::ReportValidateResult;
use aureum::{TestCase, TestCaseWithExpectations};
use relative_path::RelativePathBuf;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process;

const TEMPLATE_01_MINIMAL_TEST: &str = include_str!("../assets/01_minimal_test.au.toml");
const TEMPLATE_02_NESTED_TESTS: &str = include_str!("../assets/02_nested_tests.au.toml");
const TEMPLATE_03_ALL_SUPPORTED_FIELDS: &str =
    include_str!("../assets/03_all_supported_fields.au.toml");

fn main() {
    let current_dir = env::current_dir().expect("Current directory must be available");

    let cli = args::parse();
    let exit_code = match cli.command {
        Command::Init(args) => init_config(args),
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

fn init_config(args: InitArgs) -> ExitCode {
    let t01 = template::format_template("Minimal test", TEMPLATE_01_MINIMAL_TEST);
    let t02 = template::format_template("Nested tests", TEMPLATE_02_NESTED_TESTS);
    let t03 = template::format_template("All supported fields", TEMPLATE_03_ALL_SUPPORTED_FIELDS);

    let template = [
        t01,
        template::comment_lines(&t02),
        template::comment_lines(&t03),
    ]
    .join("\n\n");

    match args.path {
        None => {
            print!("{}", template);

            ExitCode::Success
        }
        Some(path) => {
            if path.exists() {
                report::init::print_file_already_exists(&path);
                return ExitCode::GeneralError;
            }

            let write_result = fs::write(&path, template);
            if write_result.is_err() {
                report::init::print_failed_to_write_file(&path);
                return ExitCode::GeneralError;
            }

            ExitCode::Success
        }
    }
}

fn validate_config_files(args: ValidateArgs, current_dir: &Path) -> ExitCode {
    let config_files = match prepare_config_files(args.paths, args.common.verbose, current_dir) {
        Ok(result) => result,
        Err(err) => return err,
    };

    print_config_details_if_needed(
        &config_files.loaded,
        args.common.verbose,
        args.common.hide_absolute_paths,
    );

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

    report::validate::print_validate_table(&table_entries);

    if has_config_errors {
        report::validate::print_config_files_contain_errors();

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

    print_config_details_if_needed(
        &config_files.loaded,
        args.common.verbose,
        args.common.hide_absolute_paths,
    );

    let test_entries_in_coverage_set = config_files
        .loaded
        .values()
        .flat_map(|x| x.test_entries_in_coverage_set())
        .collect::<Vec<_>>();

    let all_test_cases = test_entries_in_coverage_set
        .iter()
        .flat_map(|(_test_id, test_entry)| test_entry.test_case.clone().ok())
        .collect::<Vec<_>>();

    let has_config_errors = config_files.has_config_errors();

    if args.tree {
        report::list::print_test_list_as_tree(&all_test_cases);
    } else {
        for test_case in &all_test_cases {
            println!("{}", test_case.id());
        }
    }

    if has_config_errors {
        report::validate::print_config_files_contain_errors();

        ExitCode::InvalidConfig
    } else {
        ExitCode::Success
    }
}

fn run_programs(args: RunArgs, current_dir: &Path) -> ExitCode {
    let is_passthrough = matches!(args.format, RunOutputFormat::Passthrough);
    if is_passthrough && args.common.verbose {
        report::run::print_verbose_is_not_supported_in_passthrough();

        return ExitCode::InvalidUsage;
    }

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
        .flat_map(|(_test_id, test_entry)| test_entry.test_case.clone().ok())
        .collect::<Vec<_>>();

    match args.format {
        RunOutputFormat::Passthrough => {
            let test_entry_count = test_entries_in_coverage_set.len();
            if test_entry_count != 1 {
                report::validate::print_run_single_program_only(test_entry_count);

                return ExitCode::InvalidUsage;
            }

            match &all_test_cases[..] {
                [test_case] => run_program_as_passthrough(test_case, current_dir),
                _ => {
                    print_config_details_if_needed(
                        &config_files.loaded,
                        args.common.verbose,
                        args.common.hide_absolute_paths,
                    );

                    report::validate::print_config_files_contain_errors();

                    ExitCode::InvalidConfig
                }
            }
        }
        RunOutputFormat::Toml => {
            print_config_details_if_needed(
                &config_files.loaded,
                args.common.verbose,
                args.common.hide_absolute_paths,
            );

            let any_programs_failed_to_run =
                run_programs_with_toml_output(&all_test_cases, current_dir);
            let has_config_errors = config_files.has_config_errors();

            if any_programs_failed_to_run {
                report::run::print_one_or_more_programs_failed_to_run();

                ExitCode::RunProgramFailure
            } else if has_config_errors {
                report::validate::print_config_files_contain_errors();

                ExitCode::InvalidConfig
            } else {
                ExitCode::Success
            }
        }
    }
}

fn run_program_as_passthrough(test_case: &TestCase, current_dir: &Path) -> ExitCode {
    match aureum::run_program_passthrough(test_case, current_dir) {
        Ok(exit_code) => ExitCode::Passthrough(exit_code),
        Err(_) => {
            report::run::print_failed_to_run_program();

            ExitCode::RunProgramFailure
        }
    }
}

fn run_programs_with_toml_output(all_test_cases: &[TestCase], current_dir: &Path) -> bool {
    let mut any_programs_failed_to_run = false;

    for (index, test_case) in all_test_cases.iter().enumerate() {
        if index > 0 {
            println!(); // Print extra newline between test cases
        }

        report::run::print_test_case_id_as_toml_comment(test_case);

        match aureum::run_program(test_case, current_dir) {
            Ok(output) => {
                report::run::print_output_as_toml(&output);
            }
            Err(_) => {
                report::run::print_failed_to_run_program_as_toml();
                any_programs_failed_to_run = true;
            }
        }
    }

    any_programs_failed_to_run
}

fn run_tests(args: TestArgs, current_dir: &Path) -> ExitCode {
    if args.interactive && !io::stdout().is_terminal() {
        report::test_case::print_interactive_mode_requires_a_terminal_error();
        return ExitCode::InvalidUsage;
    }

    let config_files = match prepare_config_files(args.paths, args.common.verbose, current_dir) {
        Ok(result) => result,
        Err(err) => return err,
    };

    let test_entries_in_coverage_set = config_files
        .loaded
        .values()
        .flat_map(|x| x.test_entries_in_coverage_set())
        .collect::<Vec<_>>();

    let all_test_cases: Vec<TestCaseWithExpectations> = test_entries_in_coverage_set
        .iter()
        .flat_map(|(_test_id, test_entry)| test_entry.test_case_with_expectations().ok())
        .collect();

    let has_config_errors = config_files.has_config_errors();

    let run_results = if args.interactive {
        match interactive::run_with_progress_and_review(&all_test_cases, args.parallel, current_dir)
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: Interactive session failed: {e}");
                return ExitCode::TestFailure;
            }
        }
    } else {
        // --record suppresses normal output; only TUI frames go to stdout.
        let quiet = args.record.is_some();

        let report_config = ReportConfig {
            number_of_tests: all_test_cases.len(),
            format: get_report_format(&args.format),
        };

        if !quiet {
            print_config_details_if_needed(
                &config_files.loaded,
                args.common.verbose,
                args.common.hide_absolute_paths,
            );
            report::test_case::print_test_cases_start(&report_config);
        }

        let results = aureum::run_test_cases(
            &all_test_cases,
            args.parallel,
            current_dir,
            &|index, test_case, result| {
                if !quiet {
                    report::test_case::print_test_case(&report_config, index, test_case, result);
                }
            },
        );

        if !quiet {
            report::test_case::print_test_cases_end(&report_config, &results);
        }

        if has_config_errors && !quiet {
            report::validate::print_config_files_contain_errors();
        }

        if let Some(TerminalSize { width, height }) = args.record {
            let stdin = io::stdin();
            let stdout = io::stdout();
            if let Err(e) = interactive::run_interactive_updates(
                &results,
                current_dir,
                &mut stdin.lock(),
                &mut stdout.lock(),
                width,
                height,
            ) {
                eprintln!("error: Record session failed: {e}");
            }
        }

        results
    };

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
        report::validate::print_invalid_paths(&paths);
    }

    if find_config_files_result.found.is_empty() {
        report::validate::print_no_config_files();
        return Err(ExitCode::InvalidConfig);
    }

    if verbose {
        let config_files = find_config_files_result
            .found
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        report::validate::print_config_files_found(&config_files);
    }

    let load_config_files_result =
        load_config_file::load_config_files(find_config_files_result, current_dir);

    for (config_file_path, config_file_error) in &load_config_files_result.invalid {
        report::validate::print_config_file_error(config_file_path, config_file_error);
    }

    Ok(load_config_files_result)
}

fn print_config_details_if_needed(
    loaded: &BTreeMap<RelativePathBuf, LoadedConfigFile>,
    verbose: bool,
    hide_absolute_paths: bool,
) {
    for (config_file_path, loaded_config_file) in loaded {
        if loaded_config_file.has_validation_errors() || verbose {
            report::validate::print_config_details(
                config_file_path,
                &loaded_config_file.test_entries,
                &loaded_config_file.requirement_data,
                verbose,
                hide_absolute_paths,
            );
        }
    }
}

fn get_report_format(format: &TestOutputFormat) -> ReportFormat {
    match format {
        TestOutputFormat::Summary => ReportFormat::Summary,
        TestOutputFormat::Tap => ReportFormat::Tap,
    }
}
