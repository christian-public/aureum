use crate::args::{TerminalSize, TestArgs, TestOutputFormat};
use crate::commands::common;
use crate::counts::{ConfigStats, PlannedCounts};
use crate::exit_code::ExitCode;
use crate::interactive;
use crate::load_config_file::LoadConfigFilesResult;
use crate::report;
use crate::report::test::{ReportConfig, ReportFormat};
use crate::watch::{self, WatchHandle};
use aureum::{PlannedTestCase, RunResult};
use std::collections::BTreeSet;
use std::io::{self, BufRead, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub fn run_tests(args: TestArgs, current_dir: &Path) -> ExitCode {
    if let Some(TerminalSize { width, height }) = args.record {
        if args.watch {
            run_tests_record_watch(args, width, height, current_dir)
        } else {
            run_tests_record(args, width, height, current_dir)
        }
    } else if args.interactive {
        if !io::stdout().is_terminal() {
            report::test::print_interactive_mode_requires_a_terminal_error();
            return ExitCode::InvalidUsage;
        }

        if args.watch {
            run_tests_interactive_watch(args, current_dir)
        } else {
            run_tests_interactive(args, current_dir)
        }
    } else {
        if args.watch {
            run_tests_noninteractive_watch(args, current_dir)
        } else {
            run_tests_noninteractive(args, current_dir)
        }
    }
}

fn run_tests_interactive_watch(args: TestArgs, current_dir: &Path) -> ExitCode {
    let reload_dir = current_dir.to_path_buf();
    let default_timeout = args.default_timeout;

    let config_files = match common::prepare_config_files(
        args.paths.clone(),
        current_dir,
        args.default_timeout,
        args.common.verbose,
    ) {
        Ok(result) => result,
        Err(err) => return err,
    };
    let watch_paths = watch::collect_watch_paths(&args.paths, &config_files, current_dir);
    if args.common.verbose {
        report::validate::print_watch_files_verbose(
            &watch_paths,
            current_dir,
            args.common.stable_output,
        );
    }
    report::validate::print_config_details_if_needed(
        &config_files.loaded,
        args.common.verbose,
        args.common.stable_output,
    );

    let reload_paths = args.paths;
    let reload_fn =
        move || watch::load_test_cases_for_watch(&reload_paths, &reload_dir, default_timeout);

    match interactive::run_with_progress_review_and_watch(
        &reload_fn,
        args.parallel,
        current_dir,
        &watch_paths,
        args.common.stable_output().map(|s| s.duration),
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
    let config_files = match common::prepare_config_files(
        args.paths,
        current_dir,
        args.default_timeout,
        args.common.verbose,
    ) {
        Ok(result) => result,
        Err(err) => return err,
    };
    let config_stats = config_files.config_stats();
    let all_test_cases = collect_test_cases(&config_files);

    match interactive::run_with_progress_and_review(
        &all_test_cases,
        args.parallel,
        current_dir,
        config_stats,
        args.common.stable_output().map(|s| s.duration),
    ) {
        Ok(run_results) => {
            exit_code_from_run_results(&run_results, config_files.has_config_errors())
        }
        Err(e) => {
            report::test::print_interactive_session_failed(&e);
            ExitCode::TestFailure
        }
    }
}

fn run_tests_record_watch(args: TestArgs, width: u16, height: u16, current_dir: &Path) -> ExitCode {
    let reload_paths = args.paths.clone();
    let reload_dir = current_dir.to_path_buf();
    let default_timeout = args.default_timeout;
    let reload_fn =
        move || watch::load_test_cases_for_watch(&reload_paths, &reload_dir, default_timeout);

    let config_files = match common::prepare_config_files(
        args.paths,
        current_dir,
        args.default_timeout,
        args.common.verbose,
    ) {
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

fn run_tests_record(args: TestArgs, width: u16, height: u16, current_dir: &Path) -> ExitCode {
    let config_files = match common::prepare_config_files(
        args.paths,
        current_dir,
        args.default_timeout,
        args.common.verbose,
    ) {
        Ok(result) => result,
        Err(err) => return err,
    };
    let all_test_cases = collect_test_cases(&config_files);

    let run_results =
        aureum::run_test_cases(&all_test_cases, args.parallel, current_dir, &|_, _| {});

    let config_stats = config_files.config_stats();

    let stdin = io::stdin();
    let stdout = io::stdout();
    if let Err(e) = interactive::run_interactive_updates(
        &run_results,
        PlannedCounts::from_planned(&all_test_cases),
        current_dir,
        &mut stdin.lock(),
        &mut stdout.lock(),
        width,
        height,
        config_stats,
        args.common
            .stable_output()
            .map(|s| s.duration)
            .unwrap_or_default(),
    ) {
        report::test::print_record_session_failed(&e);
    }

    exit_code_from_run_results(&run_results, config_files.has_config_errors())
}

fn run_tests_noninteractive_watch(args: TestArgs, current_dir: &Path) -> ExitCode {
    let reload_dir = current_dir.to_path_buf();
    let default_timeout = args.default_timeout;

    let config_files = match common::prepare_config_files(
        args.paths.clone(),
        current_dir,
        args.default_timeout,
        args.common.verbose,
    ) {
        Ok(result) => result,
        Err(err) => return err,
    };
    let watch_paths = watch::collect_watch_paths(&args.paths, &config_files, current_dir);
    if args.common.verbose {
        report::validate::print_watch_files_verbose(
            &watch_paths,
            current_dir,
            args.common.stable_output,
        );
    }
    report::validate::print_config_details_if_needed(
        &config_files.loaded,
        args.common.verbose,
        args.common.stable_output,
    );

    let reload_paths = args.paths;
    let reload_fn =
        move || watch::load_test_cases_for_watch(&reload_paths, &reload_dir, default_timeout);

    match run_watch_loop(
        &reload_fn,
        args.parallel,
        current_dir,
        &watch_paths,
        args.common.verbose,
        args.common.stable_output().map(|s| s.duration),
    ) {
        Ok(run_results) => {
            exit_code_from_run_results(&run_results, config_files.has_config_errors())
        }
        Err(error) => {
            report::test::print_watch_session_failed(&error);
            ExitCode::TestFailure
        }
    }
}

fn run_tests_noninteractive(args: TestArgs, current_dir: &Path) -> ExitCode {
    let config_files = match common::prepare_config_files(
        args.paths,
        current_dir,
        args.default_timeout,
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

    let all_test_cases = collect_test_cases(&config_files);

    let stable_duration = args.common.stable_output().map(|s| s.duration);
    let run_results = run_test_cases_noninteractive(
        &all_test_cases,
        args.parallel,
        current_dir,
        &args.format,
        args.common.verbose,
        config_files.config_stats(),
        stable_duration,
    );

    let has_config_errors = config_files.has_config_errors();
    if has_config_errors {
        report::validate::print_config_files_contain_errors();
    }

    exit_code_from_run_results(&run_results, has_config_errors)
}

// HELPERS

fn run_test_cases_noninteractive(
    test_cases: &[PlannedTestCase],
    parallel: bool,
    current_dir: &Path,
    format: &TestOutputFormat,
    verbose: bool,
    config_stats: ConfigStats,
    stable_duration: Option<Duration>,
) -> Vec<RunResult> {
    let report_config = ReportConfig {
        planned_counts: PlannedCounts::from_planned(test_cases),
        format: get_report_format(format),
        verbose,
    };

    report::test::print_test_cases_start(&report_config);

    let start = Instant::now();
    let run_results =
        aureum::run_test_cases(test_cases, parallel, current_dir, &|index, run_result| {
            report::test::print_test_case(&report_config, index, run_result);
        });
    let elapsed = stable_duration.unwrap_or_else(|| start.elapsed());

    report::test::print_test_cases_end(&report_config, &run_results, config_stats, elapsed);

    run_results
}

fn run_watch_loop(
    load_test_cases: impl Fn() -> (Vec<PlannedTestCase>, ConfigStats),
    parallel: bool,
    current_dir: &Path,
    watch_paths: &BTreeSet<PathBuf>,
    verbose: bool,
    stable_duration: Option<Duration>,
) -> io::Result<Vec<RunResult>> {
    let WatchHandle {
        receiver: watch_rx,
        watched_path_count,
        _debouncer: watcher,
    } = watch::start_watcher_for_paths(watch_paths)?;
    report::test::print_watch_started(watched_path_count);
    let format = TestOutputFormat::Summary;
    let (initial_cases, initial_config_stats) = load_test_cases();
    let mut last_run_results = run_test_cases_noninteractive(
        &initial_cases,
        parallel,
        current_dir,
        &format,
        verbose,
        initial_config_stats,
        stable_duration,
    );

    let (trigger_tx, trigger_rx) = mpsc::channel::<bool>();

    let watch_forwarder = {
        let tx = trigger_tx.clone();
        std::thread::spawn(move || {
            while watch_rx.recv().is_ok() {
                if tx.send(true).is_err() {
                    break;
                }
            }
        })
    };

    if !io::stdin().is_terminal() {
        // Stdin reader is intentionally not joined: `Stdin::lines` blocks with
        // no portable way to cancel, so the thread lives until process exit.
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
    } else {
        drop(trigger_tx);
    }

    while let Ok(true) = trigger_rx.recv() {
        let mut quit_pending = false;
        while let Ok(msg) = trigger_rx.try_recv() {
            if !msg {
                quit_pending = true;
            }
        }
        report::test::print_watch_detected_file_changes();
        let (cases, config_stats) = load_test_cases();
        last_run_results = run_test_cases_noninteractive(
            &cases,
            parallel,
            current_dir,
            &format,
            verbose,
            config_stats,
            stable_duration,
        );
        if quit_pending {
            break;
        }
    }

    drop(trigger_rx);
    drop(watcher);
    let _ = watch_forwarder.join();

    Ok(last_run_results)
}

fn collect_test_cases(config_files: &LoadConfigFilesResult) -> Vec<PlannedTestCase> {
    config_files
        .loaded
        .values()
        .flat_map(|x| x.test_entries_in_coverage_set())
        .filter_map(|entry| entry.planned_test_case().ok())
        .collect()
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

fn get_report_format(format: &TestOutputFormat) -> ReportFormat {
    match format {
        TestOutputFormat::Summary => ReportFormat::Summary,
        TestOutputFormat::Tap => ReportFormat::Tap,
    }
}
