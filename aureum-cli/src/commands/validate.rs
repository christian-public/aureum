use crate::args::ValidateArgs;
use crate::commands::common;
use crate::exit_code::ExitCode;
use crate::load_config_file::LoadedConfigFile;
use crate::report;
use crate::report::validate::ReportValidateResult;
use std::path::Path;

pub fn validate_config_files(args: ValidateArgs, current_dir: &Path) -> ExitCode {
    let config_files = match common::prepare_config_files(
        args.paths,
        current_dir,
        u64::MAX,
        args.common.verbose,
        None,
    ) {
        Ok(result) => result,
        Err(err) => return err,
    };

    report::validate::print_config_details_if_needed(
        &config_files.loaded,
        args.common.verbose,
        args.common.stable_output,
    );

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
                    let is_valid =
                        watch_file_errors.is_empty() && test_entries.iter().all(|x| x.is_valid());
                    let total = test_entries.len();
                    let skipped = test_entries.iter().filter(|x| x.is_skipped()).count();
                    let validate_result = if is_valid {
                        ReportValidateResult::Success { total, skipped }
                    } else {
                        ReportValidateResult::ValidationError { total, skipped }
                    };

                    (config_file_path.clone(), validate_result)
                },
            )
            .chain(config_files.invalid.keys().map(|config_file_path| {
                (config_file_path.clone(), ReportValidateResult::ParseError)
            }))
            .collect();

    report::validate::print_validate_table(&table_entries);

    if config_files.has_config_errors() {
        report::validate::print_config_files_contain_errors();

        ExitCode::InvalidConfig
    } else {
        ExitCode::Success
    }
}
