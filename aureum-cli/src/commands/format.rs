use crate::args::FormatArgs;
use crate::exit_code::ExitCode;
use crate::find_config_file;
use crate::format;
use crate::report;
use std::path::Path;

pub fn format_config_files(args: FormatArgs, current_dir: &Path) -> ExitCode {
    let find_result = find_config_file::find_config_files(args.paths, current_dir);

    if !find_result.errors.is_empty() {
        let paths = find_result.errors.keys().cloned().collect::<Vec<_>>();
        report::validate::print_invalid_paths(&paths);
    }

    if find_result.found.is_empty() {
        report::validate::print_no_config_files();
        return ExitCode::InvalidConfig;
    }

    let mut any_would_change = false;
    let mut had_error = false;

    for config_path in find_result.found.keys() {
        let abs_path = config_path.to_path(current_dir);

        let result = if args.check {
            format::check_file(&abs_path).map(|changed| (changed, false))
        } else {
            format::format_file(&abs_path).map(|changed| (changed, changed))
        };

        match result {
            Ok((changed, _written)) => {
                if changed {
                    if args.check {
                        report::format::print_would_change(config_path);
                    }
                    any_would_change = true;
                }
            }
            Err(e) => {
                report::format::print_format_error(config_path, &e);
                had_error = true;
            }
        }
    }

    if had_error || (args.check && any_would_change) {
        ExitCode::GeneralError
    } else {
        ExitCode::Success
    }
}
