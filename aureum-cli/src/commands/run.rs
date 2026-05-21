use crate::args::{RunArgs, RunOutputFormat};
use crate::commands::common;
use crate::exit_code::ExitCode;
use crate::report;
use crate::scratch_session::ScratchSession;
use aureum::TestCase;
use std::path::Path;

pub fn run_programs(args: RunArgs, current_dir: &Path) -> ExitCode {
    let is_passthrough = matches!(args.format, RunOutputFormat::Passthrough);
    if is_passthrough && args.common.verbose {
        report::run::print_verbose_is_not_supported_in_passthrough();

        return ExitCode::InvalidUsage;
    }

    let scratch_session = match ScratchSession::create(&args.scratch) {
        Ok(s) => s,
        Err(err) => {
            report::test::print_failed_to_set_up_scratch(&err);
            return ExitCode::RunProgramFailure;
        }
    };

    let config_files = match common::prepare_config_files(
        args.paths,
        current_dir,
        u64::MAX,
        args.common.verbose,
        scratch_session.root(),
    ) {
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
        .flat_map(|test_entry| test_entry.test_case.clone().ok())
        .collect::<Vec<_>>();

    scratch_session.prepare_for_run();

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
                    report::validate::print_config_details_if_needed(
                        &config_files.loaded,
                        args.common.verbose,
                        args.common.stable_output,
                    );

                    report::validate::print_config_files_contain_errors();

                    ExitCode::InvalidConfig
                }
            }
        }
        RunOutputFormat::Toml => {
            report::validate::print_config_details_if_needed(
                &config_files.loaded,
                args.common.verbose,
                args.common.stable_output,
            );

            let any_programs_failed_to_run =
                run_programs_with_toml_output(&all_test_cases, current_dir);

            if any_programs_failed_to_run {
                report::run::print_one_or_more_programs_failed_to_run();

                ExitCode::RunProgramFailure
            } else if config_files.has_config_errors() {
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
            println!();
        }

        report::run::print_test_id_as_toml_comment(&test_case.id);

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
