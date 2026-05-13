use crate::args::ListArgs;
use crate::commands::common;
use crate::exit_code::ExitCode;
use crate::report;
use aureum::TestId;
use relative_path::RelativePathBuf;
use std::path::Path;

pub fn list_tests(args: ListArgs, current_dir: &Path) -> ExitCode {
    let config_files = match common::prepare_config_files(
        args.paths,
        current_dir,
        u64::MAX,
        args.common.verbose,
    ) {
        Ok(result) => result,
        Err(err) => return err,
    };

    report::validate::print_config_details_if_needed(
        &config_files.loaded,
        args.common.verbose,
        args.common.stable_output,
    );

    let test_paths: Vec<(RelativePathBuf, &TestId)> = config_files
        .loaded
        .iter()
        .flat_map(|(file_path, loaded)| {
            loaded
                .test_entries_in_coverage_set()
                .map(move |(test_id, _)| (file_path.clone(), test_id))
        })
        .collect();

    if args.tree {
        report::list::print_test_list_as_tree(&test_paths);
    } else {
        for (file_path, test_id) in &test_paths {
            println!("{}", aureum::format_test_id(file_path, test_id));
        }
    }

    if config_files.has_config_errors() {
        report::validate::print_config_files_contain_errors();

        ExitCode::InvalidConfig
    } else {
        ExitCode::Success
    }
}
