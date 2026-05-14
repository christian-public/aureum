mod accept;
mod action;
mod field;
mod keys;
mod review_loop;
mod theme;
mod utils;
mod views;

use crate::counts::{ConfigStats, TestCounts};
use crate::interactive::views::progress_view;
use crate::interactive::views::watch_view::{self, IdleOutcome, WatchIdleContext};
use crate::utils::time;
use crate::watch;
use accept::update_test_expectations;
use aureum::{self, PendingTestCase, RunResult};
use chrono::Local;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use field::{FieldDecision, FieldDecisions};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use review_loop::FailedTest;
use review_loop::{HeadlessDriver, LiveDriver, ReviewOutcome};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// Headless TUI review session used by `--record`. Renders the final progress frame followed
/// by each failing test's diff view into a `TestBackend` of size `width × height`. Key names
/// are read from `reader` (one per line) and the resulting frames are written to `writer`,
/// separated by `---`.
#[allow(clippy::too_many_arguments)]
pub fn run_interactive_updates<R, W>(
    run_results: &[RunResult],
    current_dir: &Path,
    reader: &mut R,
    writer: &mut W,
    width: u16,
    height: u16,
    config_stats: ConfigStats,
    stable_duration: Duration,
) -> io::Result<()>
where
    R: BufRead,
    W: Write,
{
    progress_view::record_final_progress_frame(
        run_results,
        config_stats,
        width,
        height,
        stable_duration,
        writer,
        false,
    )?;

    let failed = build_failed_tests(run_results);

    let total = failed.len();
    if total == 0 {
        return Ok(());
    }

    let counts = TestCounts::from_results(run_results, config_stats);

    let mut past_decisions: Vec<Option<FieldDecisions>> = vec![None; total];
    let mut driver = HeadlessDriver {
        width,
        height,
        reader,
        writer,
        emit_separator: true,
        watch_mode: false,
    };
    review_loop::run_review_loop(&failed, &mut past_decisions, counts, &mut driver)?;
    // Non-watch session: BackToWatch never occurs; past_decisions already populated.

    let mut wrote_header = false;
    for (idx, dec_opt) in past_decisions.iter().enumerate() {
        let Some(decisions) = dec_opt else { continue };
        if !decisions.any_accepted() {
            continue;
        }
        let failed_test = &failed[idx];
        let Ok(test_outcome) = failed_test.result else {
            continue;
        };
        if !wrote_header {
            writeln!(writer)?;
            wrote_header = true;
        }
        update_test_expectations(failed_test.test_case, test_outcome, current_dir, decisions)?;
        writeln!(
            writer,
            "Updated {} ({})",
            failed_test.test_case.display_id(),
            accepted_field_names(decisions)
        )?;
    }

    Ok(())
}

/// Headless watch session for `--watch --record`. Runs tests, shows the idle screen
/// headlessly, and loops on file-change commands from `reader`. The special key name
/// `"file-change"` simulates a watcher event; all other key names are forwarded to the
/// idle or review views. Frames are written to `writer` separated by `---`.
#[allow(clippy::too_many_arguments)]
pub fn run_interactive_updates_with_watch<R, W>(
    load_test_cases: &dyn Fn() -> (Vec<PendingTestCase>, ConfigStats),
    parallel: bool,
    current_dir: &Path,
    reader: &mut R,
    writer: &mut W,
    width: u16,
    height: u16,
) -> io::Result<Vec<RunResult>>
where
    R: BufRead,
    W: Write,
{
    let mut emit_separator = false;

    'rerun: loop {
        let (test_cases, config_stats) = load_test_cases();
        let run_results = aureum::run_test_cases(&test_cases, parallel, current_dir, &|_, _| {});

        progress_view::record_final_progress_frame(
            &run_results,
            config_stats,
            width,
            height,
            Duration::ZERO,
            writer,
            emit_separator,
        )?;
        emit_separator = true;

        loop {
            let outcome = watch_view::record_watch_idle(
                &run_results,
                width,
                height,
                reader,
                writer,
                emit_separator,
                "12:00:00",
                "0.5s",
                config_stats,
            )?;
            emit_separator = true;

            match outcome {
                IdleOutcome::Rerun => continue 'rerun,
                IdleOutcome::Quit => return Ok(run_results),
                IdleOutcome::Review => {
                    let counts = TestCounts::from_results(&run_results, config_stats);
                    let failed_pairs = build_failed_tests(&run_results);

                    if failed_pairs.is_empty() {
                        continue;
                    }

                    let mut past_decisions = vec![None; failed_pairs.len()];
                    let mut driver = HeadlessDriver {
                        width,
                        height,
                        reader,
                        writer,
                        emit_separator,
                        watch_mode: true,
                    };

                    let outcome = review_loop::run_review_loop(
                        &failed_pairs,
                        &mut past_decisions,
                        counts,
                        &mut driver,
                    )?;
                    emit_separator = driver.emit_separator;

                    let decisions: Vec<(usize, FieldDecisions)> = match outcome {
                        ReviewOutcome::Quit => return Ok(run_results),
                        ReviewOutcome::BackToWatch(d) => d
                            .into_iter()
                            .filter(|(_, dec)| dec.any_accepted())
                            .collect(),
                        ReviewOutcome::Done => past_decisions
                            .iter()
                            .enumerate()
                            .filter_map(|(i, dec_opt)| {
                                let dec = (*dec_opt)?;
                                if !dec.any_accepted() {
                                    return None;
                                }
                                Some((i, dec))
                            })
                            .collect(),
                    };

                    for (failed_idx, decisions) in &decisions {
                        let failed_test = &failed_pairs[*failed_idx];
                        let Ok(test_outcome) = failed_test.result else {
                            continue;
                        };
                        update_test_expectations(
                            failed_test.test_case,
                            test_outcome,
                            current_dir,
                            decisions,
                        )?;
                    }
                }
            }
        }
    }
}

/// Full interactive watch session: runs tests, shows idle screen, lets the user review and
/// accept failures, and re-runs on file changes. Always returns the last run's results.
pub fn run_with_progress_review_and_watch<'a>(
    load_test_cases: &dyn Fn() -> (Vec<PendingTestCase>, ConfigStats),
    parallel: bool,
    current_dir: &Path,
    watch_paths: impl IntoIterator<Item = &'a PathBuf>,
    stable_duration: Option<Duration>,
) -> io::Result<Vec<RunResult>> {
    let watch_handle = watch::start_watcher_for_paths(watch_paths)?;

    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(io::Error::other)?;

    let result = run_watch_interactive_loop(
        &mut terminal,
        load_test_cases,
        parallel,
        current_dir,
        &watch_handle.receiver,
        stable_duration,
    );

    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen);

    let run_results = result?.unwrap_or_default();
    Ok(run_results)
}

fn run_watch_interactive_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    load_test_cases: &dyn Fn() -> (Vec<PendingTestCase>, ConfigStats),
    parallel: bool,
    current_dir: &Path,
    change_rx: &Receiver<usize>,
    stable_duration: Option<Duration>,
) -> io::Result<Option<Vec<RunResult>>> {
    'rerun: loop {
        let (test_cases, config_stats) = load_test_cases();
        let run_start = Instant::now();
        // Run tests with live progress view.
        let Some(last_results) = progress_view::run_tests_with_progress(
            terminal,
            &test_cases,
            parallel,
            current_dir,
            config_stats,
            stable_duration,
        )?
        else {
            return Ok(None); // user pressed q during progress
        };
        let finished_at = Local::now().format("%H:%M:%S").to_string();
        let duration = time::format_duration(run_start.elapsed());

        // Idle/watching loop — re-enters after review until a file change triggers a re-run.
        loop {
            let idle_ctx = WatchIdleContext {
                run_results: &last_results,
                finished_at: &finished_at,
                duration: &duration,
                config_stats,
            };
            match watch_view::run_watch_idle(terminal, &idle_ctx, change_rx)? {
                IdleOutcome::Rerun => continue 'rerun,
                IdleOutcome::Quit => return Ok(Some(last_results)),
                IdleOutcome::Review => {
                    // Enter review loop (watch mode = true so Esc returns here).
                    let failed_pairs = build_failed_tests(&last_results);
                    let counts = TestCounts::from_results(&last_results, config_stats);
                    let Some(accepted) = run_watch_review(terminal, &failed_pairs, counts)? else {
                        return Ok(Some(last_results)); // user pressed q
                    };
                    for (failed_idx, decisions) in &accepted {
                        let failed_test = &failed_pairs[*failed_idx];
                        let Ok(test_outcome) = failed_test.result else {
                            continue;
                        };
                        update_test_expectations(
                            failed_test.test_case,
                            test_outcome,
                            current_dir,
                            decisions,
                        )?;
                    }
                    // Back to idle screen after review.
                }
            }
        }
    }
}

/// Runs a watch-mode review session. Returns `Some(accepted_decisions)` when the user
/// finishes or presses Esc (back to watch), or `None` if the user pressed `q`.
/// Decisions are returned but NOT written to disk — the caller applies them.
fn run_watch_review<'a>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    failed_pairs: &'a [FailedTest<'a>],
    counts: TestCounts,
) -> io::Result<Option<Vec<(usize, FieldDecisions)>>> {
    if failed_pairs.is_empty() {
        return Ok(Some(vec![]));
    }

    let mut past_decisions: Vec<Option<FieldDecisions>> = vec![None; failed_pairs.len()];
    let mut driver = LiveDriver {
        terminal,
        watch_mode: true,
    };

    let outcome =
        review_loop::run_review_loop(failed_pairs, &mut past_decisions, counts, &mut driver)?;

    let filter_accepted = |pairs: Vec<(usize, FieldDecisions)>| -> Vec<(usize, FieldDecisions)> {
        pairs
            .into_iter()
            .filter(|(_, dec)| dec.any_accepted())
            .collect()
    };

    match outcome {
        ReviewOutcome::Quit => Ok(None),
        ReviewOutcome::BackToWatch(d) => Ok(Some(filter_accepted(d))),
        ReviewOutcome::Done => {
            let collected: Vec<(usize, FieldDecisions)> = past_decisions
                .iter()
                .enumerate()
                .filter_map(|(i, dec_opt)| {
                    let dec = (*dec_opt)?;
                    if !dec.any_accepted() {
                        return None;
                    }
                    Some((i, dec))
                })
                .collect();
            Ok(Some(collected))
        }
    }
}

/// Full interactive session for a real terminal: shows live test progress, then lets the user
/// review and accept failures one by one. Enters/leaves alternate screen internally.
pub fn run_with_progress_and_review(
    test_cases: &[PendingTestCase],
    parallel: bool,
    current_dir: &Path,
    config_stats: ConfigStats,
    stable_duration: Option<Duration>,
) -> io::Result<Vec<RunResult>> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(io::Error::other)?;

    let result = run_tui_session(
        &mut terminal,
        test_cases,
        parallel,
        current_dir,
        config_stats,
        stable_duration,
    );

    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen);

    let Some(run_results) = result? else {
        // User quit during progress; background test thread detached.
        std::process::exit(1);
    };

    Ok(run_results)
}

fn run_tui_session(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    test_cases: &[PendingTestCase],
    parallel: bool,
    current_dir: &Path,
    config_stats: ConfigStats,
    stable_duration: Option<Duration>,
) -> io::Result<Option<Vec<RunResult>>> {
    let Some(run_results) = progress_view::run_tests_with_progress(
        terminal,
        test_cases,
        parallel,
        current_dir,
        config_stats,
        stable_duration,
    )?
    else {
        return Ok(None); // user quit; background thread detached
    };

    let counts = TestCounts::from_results(&run_results, config_stats);
    let failed_pairs = build_failed_tests(&run_results);

    let mut past_decisions: Vec<Option<FieldDecisions>> = vec![None; failed_pairs.len()];
    let mut driver = LiveDriver {
        terminal,
        watch_mode: false,
    };
    review_loop::run_review_loop(&failed_pairs, &mut past_decisions, counts, &mut driver)?;
    // Non-watch session: BackToWatch/Quit both just proceed to apply decisions.

    for (i, dec_opt) in past_decisions.iter().enumerate() {
        let Some(decisions) = dec_opt else { continue };
        if !decisions.any_accepted() {
            continue;
        }
        let failed_test = &failed_pairs[i];
        let Ok(test_outcome) = failed_test.result else {
            continue;
        };
        update_test_expectations(failed_test.test_case, test_outcome, current_dir, decisions)?;
    }

    Ok(Some(run_results))
}

fn build_failed_tests(run_results: &[RunResult]) -> Vec<FailedTest<'_>> {
    run_results
        .iter()
        .filter_map(|r| match r {
            RunResult::Ran { test_case, result } if !r.is_success() => {
                Some(FailedTest { test_case, result })
            }
            _ => None,
        })
        .collect()
}

fn accepted_field_names(decisions: &FieldDecisions) -> String {
    let mut names = Vec::new();
    if decisions.stdout == FieldDecision::Accepted {
        names.push("stdout");
    }
    if decisions.stderr == FieldDecision::Accepted {
        names.push("stderr");
    }
    if decisions.exit_code == FieldDecision::Accepted {
        names.push("exit_code");
    }
    names.join(", ")
}

#[cfg(test)]
mod tests {
    use super::utils::test_helpers::{TempDir, make_test_case_root};
    use super::*;
    use aureum::{FieldOutcome, TestCase, TestOutcome};
    use std::io::Cursor;

    fn failing_run_result_stdout(test_case: TestCase, expected: &str, got: &str) -> RunResult {
        RunResult::Ran {
            test_case,
            result: Ok(TestOutcome {
                stdout: FieldOutcome::Diff {
                    expected: expected.to_string(),
                    got: got.to_string(),
                },
                stderr: FieldOutcome::NotChecked("".to_string()),
                exit_code: FieldOutcome::NotChecked(0),
            }),
        }
    }

    #[test]
    fn test_record_mode_accept_file_reference_updates_file_and_prints_updated() {
        let tmp = TempDir::new("record_accept_fileref");
        tmp.write("expected_stdout.txt", "wrong\n");
        tmp.write(
            "test.toml",
            "program = \"echo\"\nexpected_stdout = { file = \"expected_stdout.txt\" }\n",
        );

        let tc = make_test_case_root("", "test.toml");
        let results = vec![failing_run_result_stdout(tc, "wrong\n", "actual\n")];

        let mut input = Cursor::new(b"a\nenter\n");
        let mut output = Vec::<u8>::new();

        run_interactive_updates(
            &results,
            tmp.path(),
            &mut input,
            &mut output,
            80,
            20,
            ConfigStats::default(),
            std::time::Duration::ZERO,
        )
        .unwrap();

        assert_eq!(tmp.read("expected_stdout.txt"), "actual\n");
        assert!(String::from_utf8_lossy(&output).contains("Updated test.toml (stdout)"));
    }
}
