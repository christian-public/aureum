use crate::args::{TerminalSize, TestArgs, TestOutputFormat};
use crate::commands::common;
use crate::exit_code::ExitCode;
use crate::interactive;
use crate::report;
use crate::report::test::{ReportConfig, ReportFormat};
use crate::watch;
use aureum::{RunResult, TestCaseWithExpectations};
use std::collections::BTreeSet;
use std::io::{self, BufRead, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

pub fn run_tests(args: TestArgs, current_dir: &Path) -> ExitCode {
    if args.interactive && !io::stdout().is_terminal() {
        report::test::print_interactive_mode_requires_a_terminal_error();
        return ExitCode::InvalidUsage;
    }
    if args.interactive && args.watch {
        run_tests_interactive_watch(args, current_dir)
    } else if args.interactive {
        run_tests_interactive(args, current_dir)
    } else if args.watch && args.record.is_some() {
        run_tests_record_watch(args, current_dir)
    } else if args.watch {
        run_tests_watch(args, current_dir)
    } else {
        run_tests_once(args, current_dir)
    }
}

fn exit_code_from_run_results(run_results: &[RunResult], has_config_errors: bool) -> ExitCode {
    if !run_results.iter().all(|t| t.is_success()) {
        ExitCode::TestFailure
    } else if has_config_errors {
        ExitCode::InvalidConfig
    } else {
        ExitCode::Success
    }
}

fn run_tests_interactive_watch(args: TestArgs, current_dir: &Path) -> ExitCode {
    let reload_paths = args.paths.clone();
    let watch_input_paths = args.paths.clone();
    let reload_dir = current_dir.to_path_buf();
    let default_timeout = args.default_timeout;
    let reload_fn =
        move || watch::load_test_cases_for_watch(&reload_paths, &reload_dir, default_timeout);

    let config_files =
        match common::prepare_config_files(args.paths, args.common.verbose, current_dir) {
            Ok(result) => result,
            Err(err) => return err,
        };
    let watch_paths = watch::collect_watch_paths(&watch_input_paths, &config_files, current_dir);
    if args.common.verbose {
        report::validate::print_watch_files_verbose(
            &watch_paths,
            current_dir,
            args.common.hide_absolute_paths,
        );
    }
    report::validate::print_config_details_if_needed(
        &config_files.loaded,
        args.common.verbose,
        args.common.hide_absolute_paths,
    );

    match interactive::run_with_progress_review_and_watch(
        &reload_fn,
        args.parallel,
        current_dir,
        &watch_paths,
    ) {
        Ok(run_results) => {
            exit_code_from_run_results(&run_results, config_files.has_config_errors())
        }
        Err(e) => {
            report::test::print_interactive_watch_session_failed(&e);
            ExitCode::TestFailure
        }
    }
}

fn run_tests_interactive(args: TestArgs, current_dir: &Path) -> ExitCode {
    let config_files =
        match common::prepare_config_files(args.paths, args.common.verbose, current_dir) {
            Ok(result) => result,
            Err(err) => return err,
        };
    let test_entries = config_files
        .loaded
        .values()
        .flat_map(|x| x.test_entries_in_coverage_set())
        .collect::<Vec<_>>();
    let all_test_cases: Vec<TestCaseWithExpectations> = test_entries
        .iter()
        .flat_map(|(_, entry)| entry.test_case_with_expectations().ok())
        .collect();

    match interactive::run_with_progress_and_review(&all_test_cases, args.parallel, current_dir) {
        Ok(run_results) => {
            exit_code_from_run_results(&run_results, config_files.has_config_errors())
        }
        Err(e) => {
            report::test::print_interactive_session_failed(&e);
            ExitCode::TestFailure
        }
    }
}

fn run_tests_record_watch(args: TestArgs, current_dir: &Path) -> ExitCode {
    let TerminalSize { width, height } = args.record.expect("called only when record is Some");
    let reload_paths = args.paths.clone();
    let reload_dir = current_dir.to_path_buf();
    let default_timeout = args.default_timeout;
    let reload_fn =
        move || watch::load_test_cases_for_watch(&reload_paths, &reload_dir, default_timeout);

    let config_files =
        match common::prepare_config_files(args.paths, args.common.verbose, current_dir) {
            Ok(result) => result,
            Err(err) => return err,
        };

    let stdin = io::stdin();
    let stdout = io::stdout();
    match interactive::run_interactive_updates_with_watch(
        &reload_fn,
        args.parallel,
        current_dir,
        &mut stdin.lock(),
        &mut stdout.lock(),
        width,
        height,
    ) {
        Ok(run_results) => {
            exit_code_from_run_results(&run_results, config_files.has_config_errors())
        }
        Err(e) => {
            report::test::print_watch_record_session_failed(&e);
            ExitCode::TestFailure
        }
    }
}

fn run_tests_watch(args: TestArgs, current_dir: &Path) -> ExitCode {
    let reload_paths = args.paths.clone();
    let watch_input_paths = args.paths.clone();
    let reload_dir = current_dir.to_path_buf();
    let default_timeout = args.default_timeout;
    let reload_fn =
        move || watch::load_test_cases_for_watch(&reload_paths, &reload_dir, default_timeout);

    let config_files =
        match common::prepare_config_files(args.paths, args.common.verbose, current_dir) {
            Ok(result) => result,
            Err(err) => return err,
        };
    let watch_paths = watch::collect_watch_paths(&watch_input_paths, &config_files, current_dir);
    if args.common.verbose {
        report::validate::print_watch_files_verbose(
            &watch_paths,
            current_dir,
            args.common.hide_absolute_paths,
        );
    }
    report::validate::print_config_details_if_needed(
        &config_files.loaded,
        args.common.verbose,
        args.common.hide_absolute_paths,
    );

    match run_watch_loop(&reload_fn, args.parallel, current_dir, &watch_paths) {
        Ok(run_results) => {
            exit_code_from_run_results(&run_results, config_files.has_config_errors())
        }
        Err(e) => {
            report::test::print_watch_session_failed(&e);
            ExitCode::TestFailure
        }
    }
}

fn run_tests_once(args: TestArgs, current_dir: &Path) -> ExitCode {
    let config_files =
        match common::prepare_config_files(args.paths, args.common.verbose, current_dir) {
            Ok(result) => result,
            Err(err) => return err,
        };
    let has_config_errors = config_files.has_config_errors();

    if args.record.is_none() {
        report::validate::print_config_details_if_needed(
            &config_files.loaded,
            args.common.verbose,
            args.common.hide_absolute_paths,
        );
    }

    let test_entries = config_files
        .loaded
        .values()
        .flat_map(|x| x.test_entries_in_coverage_set())
        .collect::<Vec<_>>();
    let all_test_cases: Vec<TestCaseWithExpectations> = test_entries
        .iter()
        .flat_map(|(_, entry)| {
            if let Ok(mut tc) = entry.test_case_with_expectations() {
                tc.test_case.timeout_seconds =
                    tc.test_case.timeout_seconds.or(Some(args.default_timeout));
                Some(tc)
            } else {
                None
            }
        })
        .collect();

    let run_results = run_test_report(
        &all_test_cases,
        args.parallel,
        &args.format,
        args.record,
        has_config_errors,
        current_dir,
    );
    exit_code_from_run_results(&run_results, has_config_errors)
}

fn run_test_report(
    all_test_cases: &[TestCaseWithExpectations],
    parallel: bool,
    format: &TestOutputFormat,
    record: Option<TerminalSize>,
    has_config_errors: bool,
    current_dir: &Path,
) -> Vec<RunResult> {
    // --record suppresses normal output; only TUI frames go to stdout.
    let quiet = record.is_some();

    let report_config = ReportConfig {
        number_of_tests: all_test_cases.len(),
        format: get_report_format(format),
    };

    if !quiet {
        report::test::print_test_cases_start(&report_config);
    }

    let results = aureum::run_test_cases(
        all_test_cases,
        parallel,
        current_dir,
        &|index, test_case, result| {
            if !quiet {
                report::test::print_test_case(&report_config, index, test_case, result);
            }
        },
    );

    if !quiet {
        report::test::print_test_cases_end(&report_config, &results);
    }

    if has_config_errors && !quiet {
        report::validate::print_config_files_contain_errors();
    }

    if let Some(TerminalSize { width, height }) = record {
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
            report::test::print_record_session_failed(&e);
        }
    }

    results
}

fn run_test_batch(
    test_cases: &[TestCaseWithExpectations],
    parallel: bool,
    current_dir: &Path,
    format: &TestOutputFormat,
) -> Vec<RunResult> {
    let report_config = ReportConfig {
        number_of_tests: test_cases.len(),
        format: get_report_format(format),
    };
    report::test::print_test_cases_start(&report_config);
    let results = aureum::run_test_cases(test_cases, parallel, current_dir, &|index, tc, r| {
        report::test::print_test_case(&report_config, index, tc, r);
    });
    report::test::print_test_cases_end(&report_config, &results);
    results
}

fn run_watch_loop(
    load_test_cases: impl Fn() -> Vec<TestCaseWithExpectations>,
    parallel: bool,
    current_dir: &Path,
    watch_paths: &BTreeSet<PathBuf>,
) -> io::Result<Vec<RunResult>> {
    let watch::WatchHandle {
        receiver: watch_rx,
        watched_path_count,
        _debouncer: _watcher,
    } = watch::start_watcher_for_paths(watch_paths)?;
    report::test::print_watch_started(watched_path_count);
    let format = TestOutputFormat::Summary;
    let mut last_results = run_test_batch(&load_test_cases(), parallel, current_dir, &format);

    let (trigger_tx, trigger_rx) = mpsc::channel::<bool>();

    {
        let tx = trigger_tx.clone();
        std::thread::spawn(move || {
            while watch_rx.recv().is_ok() {
                if tx.send(true).is_err() {
                    break;
                }
            }
        });
    }

    if !io::stdin().is_terminal() {
        std::thread::spawn(move || {
            for line in io::stdin().lock().lines() {
                match line {
                    Ok(l) if l == "file-change" => {
                        if trigger_tx.send(true).is_err() {
                            return;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            let _ = trigger_tx.send(false);
        });
    }

    while let Ok(true) = trigger_rx.recv() {
        let mut quit_pending = false;
        while let Ok(msg) = trigger_rx.try_recv() {
            if !msg {
                quit_pending = true;
            }
        }
        report::test::print_watch_detected_file_changes();
        last_results = run_test_batch(&load_test_cases(), parallel, current_dir, &format);
        if quit_pending {
            break;
        }
    }

    Ok(last_results)
}

fn get_report_format(format: &TestOutputFormat) -> ReportFormat {
    match format {
        TestOutputFormat::Summary => ReportFormat::Summary,
        TestOutputFormat::Tap => ReportFormat::Tap,
    }
}
