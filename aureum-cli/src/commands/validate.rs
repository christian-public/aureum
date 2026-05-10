use crate::args::ValidateArgs;
use crate::commands::common;
use crate::exit_code::ExitCode;
use crate::load_config_file::LoadedConfigFile;
use crate::report;
use crate::report::validate::ReportValidateResult;
use std::path::Path;

pub fn validate_config_files(args: ValidateArgs, current_dir: &Path) -> ExitCode {
    let config_files =
        match common::prepare_config_files(args.paths, args.common.verbose, current_dir) {
            Ok(result) => result,
            Err(err) => return err,
        };

    report::validate::print_config_details_if_needed(
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
                |(
                    config_file_path,
                    LoadedConfigFile {
                        test_entries,
                        watch_file_errors,
                        ..
                    },
                )| {
                    let is_valid = watch_file_errors.is_empty()
                        && test_entries.iter().all(|(_, x)| x.is_runnable());
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
