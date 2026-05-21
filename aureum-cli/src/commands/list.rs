use crate::args::{ListArgs, ListShowFilter};
use crate::commands::common;
use crate::exit_code::ExitCode;
use crate::report;
use std::path::Path;

pub fn list_tests(args: ListArgs, current_dir: &Path) -> ExitCode {
    let config_files = match common::prepare_config_files(
        args.paths,
        current_dir,
        u64::MAX,
        args.common.verbose,
        Some(current_dir),
    ) {
        Ok(result) => result,
        Err(err) => return err,
    };

    report::validate::print_config_details_if_needed(
        &config_files.loaded,
        args.common.verbose,
        args.common.stable_output,
    );

    let ids = config_files
        .loaded
        .values()
        .flat_map(|loaded| loaded.test_entries_in_coverage_set())
        .filter(|entry| match args.show {
            ListShowFilter::All => true,
            ListShowFilter::Runnable => entry.is_runnable_if_no_validation_errors(),
            ListShowFilter::Skipped => entry.is_skipped(),
        })
        .map(|entry| entry.id.clone())
        .collect::<Vec<_>>();

    if args.tree {
        report::list::print_test_list_as_tree(&ids);
    } else {
        for id in ids {
            println!("{}", id.display_id());
        }
    }

    if config_files.has_config_errors() {
        report::validate::print_config_files_contain_errors();

        ExitCode::InvalidConfig
    } else {
        ExitCode::Success
    }
}
