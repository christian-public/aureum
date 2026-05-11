use crate::exit_code::ExitCode;
use crate::find_config_file;
use crate::load_config_file;
use crate::load_config_file::LoadConfigFilesResult;
use crate::report;
use std::path::{Path, PathBuf};

pub fn prepare_config_files(
    paths: Vec<PathBuf>,
    current_dir: &Path,
    default_timeout: u64,
    verbose: bool,
) -> Result<LoadConfigFilesResult, ExitCode> {
    let find_config_files_result = find_config_file::find_config_files(paths, current_dir);

    if !find_config_files_result.errors.is_empty() {
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
        load_config_file::load_config_files(find_config_files_result, current_dir, default_timeout);

    for (config_file_path, config_file_error) in &load_config_files_result.invalid {
        report::validate::print_config_file_error(config_file_path, config_file_error);
    }

    Ok(load_config_files_result)
}
