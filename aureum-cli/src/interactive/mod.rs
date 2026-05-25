mod accept;
mod action;
mod field;
mod keys;
mod review_loop;
mod theme;
mod tty;
mod utils;
mod views;

use crate::counts::{ConfigStats, PlannedCounts, TestCounts};
use crate::interactive::tty::{LiveTty, RecordTty};
use crate::interactive::views::progress_view;
use crate::interactive::views::watch_view::{self, IdleOutcome, WatchIdleContext};
use crate::stable_output::StableOutput;
use crate::utils::time;
use crate::watch;
use accept::update_test_expectations;
use aureum::{self, PlannedTestCase, RunResult};
use chrono::Local;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use field::{FieldDecision, FieldDecisions, OUTPUT_FIELDS};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use review_loop::FailedTest;
use review_loop::ReviewOutcome;
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
    planned_counts: PlannedCounts,
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
        planned_counts,
        config_stats,
        width,
        height,
        stable_duration,
        writer,
        false,
    )?;

    let failed = build_failed_tests(run_results);
    if failed.is_empty() {
        return Ok(());
    }

    let counts = TestCounts::from_results(run_results, config_stats);
    let mut past_decisions: Vec<Option<FieldDecisions>> = vec![None; failed.len()];
    {
        let mut record_tty = RecordTty::new(width, height, reader, writer, true)?;
        review_loop::run_review_loop(&failed, &mut past_decisions, counts, &mut record_tty, false)?;
    }

    let mut wrote_header = false;
    for (idx, decisions) in accepted_from_past(&past_decisions) {
        let failed_test = &failed[idx];
        let Ok(test_outcome) = failed_test.result else {
            continue;
        };
        if !wrote_header {
            writeln!(writer)?;
            wrote_header = true;
        }
        update_test_expectations(failed_test.test_case, test_outcome, current_dir, &decisions)?;
        writeln!(
            writer,
            "Updated {} ({})",
            failed_test.test_case.display_id(),
            accepted_field_names(&decisions)
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
    load_test_cases: &dyn Fn() -> (Vec<PlannedTestCase>, ConfigStats),
    parallel: bool,
    current_dir: &Path,
    reader: &mut R,
    writer: &mut W,
    width: u16,
    height: u16,
    stable_output: StableOutput,
) -> io::Result<Vec<RunResult>>
where
    R: BufRead,
    W: Write,
{
    let mut emit_separator = false;
    let finished_at = stable_output.finished_at.format("%H:%M:%S").to_string();
    let run_time = time::format_duration(stable_output.run_time);

    'rerun: loop {
        let (test_cases, config_stats) = load_test_cases();
        let planned_counts = PlannedCounts::from_planned(&test_cases);
        let run_results = aureum::run_test_cases(&test_cases, parallel, current_dir, &|_, _| {});

        progress_view::record_final_progress_frame(
            &run_results,
            planned_counts,
            config_stats,
            width,
            height,
            stable_output.elapsed,
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
                &finished_at,
                &run_time,
                config_stats,
            )?;
            emit_separator = true;

            match outcome {
                IdleOutcome::Rerun => continue 'rerun,
                IdleOutcome::Quit => return Ok(run_results),
                IdleOutcome::Review => {
                    let failed = build_failed_tests(&run_results);
                    if failed.is_empty() {
                        continue;
                    }
                    let counts = TestCounts::from_results(&run_results, config_stats);
                    let mut past_decisions = vec![None; failed.len()];
                    let review_outcome = {
                        let mut record_tty = RecordTty::new(width, height, reader, writer, true)?;
                        review_loop::run_review_loop(
                            &failed,
                            &mut past_decisions,
                            counts,
                            &mut record_tty,
                            true,
                        )?
                    };

                    apply_decisions(&failed, &accepted_from_past(&past_decisions), current_dir)?;
                    if let ReviewOutcome::Quit = review_outcome {
                        return Ok(run_results);
                    }
                }
            }
        }
    }
}

/// Full interactive watch session: runs tests, shows idle screen, lets the user review and
/// accept failures, and re-runs on file changes. Always returns the last run's results.
pub fn run_with_progress_review_and_watch<'a>(
    load_test_cases: &dyn Fn() -> (Vec<PlannedTestCase>, ConfigStats),
    parallel: bool,
    current_dir: &Path,
    watch_paths: impl IntoIterator<Item = &'a PathBuf>,
    stable_output: Option<StableOutput>,
) -> io::Result<Vec<RunResult>> {
    let watch_handle = watch::start_watcher_for_paths(watch_paths)?;
    let mut terminal = enter_alternate_screen()?;

    let result = run_watch_interactive_loop(
        &mut terminal,
        load_test_cases,
        parallel,
        current_dir,
        &watch_handle.receiver,
        stable_output,
    );

    leave_alternate_screen(&mut terminal);
    Ok(result?.unwrap_or_default())
}

/// Full interactive session for a real terminal: shows live test progress, then lets the user
/// review and accept failures one by one. Enters/leaves alternate screen internally.
pub fn run_with_progress_and_review(
    test_cases: &[PlannedTestCase],
    parallel: bool,
    current_dir: &Path,
    config_stats: ConfigStats,
    stable_duration: Option<Duration>,
) -> io::Result<Vec<RunResult>> {
    let mut terminal = enter_alternate_screen()?;

    let result = run_tui_session(
        &mut terminal,
        test_cases,
        parallel,
        current_dir,
        config_stats,
        stable_duration,
    );

    leave_alternate_screen(&mut terminal);

    let Some(run_results) = result? else {
        // User quit during progress; background test thread detached.
        std::process::exit(1);
    };
    Ok(run_results)
}

// ── Internals ────────────────────────────────────────────────────────────────

fn run_tui_session(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    test_cases: &[PlannedTestCase],
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

    let failed = build_failed_tests(&run_results);
    let counts = TestCounts::from_results(&run_results, config_stats);
    let mut past_decisions: Vec<Option<FieldDecisions>> = vec![None; failed.len()];
    {
        let mut live_tty = LiveTty { terminal };
        review_loop::run_review_loop(&failed, &mut past_decisions, counts, &mut live_tty, false)?;
    }
    // Non-watch session: BackToWatch/Quit both just proceed to apply decisions.
    apply_decisions(&failed, &accepted_from_past(&past_decisions), current_dir)?;

    Ok(Some(run_results))
}

fn run_watch_interactive_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    load_test_cases: &dyn Fn() -> (Vec<PlannedTestCase>, ConfigStats),
    parallel: bool,
    current_dir: &Path,
    change_rx: &Receiver<usize>,
    stable_output: Option<StableOutput>,
) -> io::Result<Option<Vec<RunResult>>> {
    'rerun: loop {
        let (test_cases, config_stats) = load_test_cases();
        let run_start = Instant::now();
        let Some(last_results) = progress_view::run_tests_with_progress(
            terminal,
            &test_cases,
            parallel,
            current_dir,
            config_stats,
            stable_output.map(|s| s.elapsed),
        )?
        else {
            return Ok(None); // user pressed q during progress
        };
        let finished_at = stable_output
            .map(|s| s.finished_at)
            .unwrap_or_else(|| Local::now().time())
            .format("%H:%M:%S")
            .to_string();
        let duration = time::format_duration(
            stable_output
                .map(|s| s.run_time)
                .unwrap_or_else(|| run_start.elapsed()),
        );

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
                    let failed = build_failed_tests(&last_results);
                    if failed.is_empty() {
                        continue;
                    }
                    let counts = TestCounts::from_results(&last_results, config_stats);
                    let mut past_decisions = vec![None; failed.len()];
                    let review_outcome = {
                        let mut live_tty = LiveTty { terminal };
                        review_loop::run_review_loop(
                            &failed,
                            &mut past_decisions,
                            counts,
                            &mut live_tty,
                            true,
                        )?
                    };

                    apply_decisions(&failed, &accepted_from_past(&past_decisions), current_dir)?;
                    if let ReviewOutcome::Quit = review_outcome {
                        return Ok(Some(last_results));
                    }
                    // Back to idle screen after review.
                }
            }
        }
    }
}

fn enter_alternate_screen() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).map_err(io::Error::other)
}

fn leave_alternate_screen(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) {
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen);
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

/// Returns `(index, decisions)` pairs where the per-test decision has at least one accepted
/// field. Applied regardless of how the review ended — any confirmed field is durable.
fn accepted_from_past(past_decisions: &[Option<FieldDecisions>]) -> Vec<(usize, FieldDecisions)> {
    past_decisions
        .iter()
        .enumerate()
        .filter_map(|(i, dec_opt)| dec_opt.filter(|d| d.any_accepted()).map(|d| (i, d)))
        .collect()
}

fn apply_decisions(
    failed: &[FailedTest<'_>],
    decisions: &[(usize, FieldDecisions)],
    current_dir: &Path,
) -> io::Result<()> {
    for (idx, dec) in decisions {
        let failed_test = &failed[*idx];
        let Ok(test_outcome) = failed_test.result else {
            continue;
        };
        update_test_expectations(failed_test.test_case, test_outcome, current_dir, dec)?;
    }
    Ok(())
}

fn accepted_field_names(decisions: &FieldDecisions) -> String {
    OUTPUT_FIELDS
        .iter()
        .filter(|&&f| decisions.get(f) == FieldDecision::Accepted)
        .map(|&f| f.name())
        .collect::<Vec<_>>()
        .join(", ")
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
            "program = \"echo\"\nexpected_stdout = { from_file = \"expected_stdout.txt\" }\n",
        );

        let tc = make_test_case_root("", "test.toml");
        let results = vec![failing_run_result_stdout(tc, "wrong\n", "actual\n")];

        let mut input = Cursor::new(b"a\nenter\n");
        let mut output = Vec::<u8>::new();

        run_interactive_updates(
            &results,
            PlannedCounts {
                runnable: 1,
                skipped: 0,
            },
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

    #[test]
    fn test_record_mode_quit_persists_only_confirmed_fields() {
        let tmp = TempDir::new("record_quit_partial");
        tmp.write(
            "test.toml",
            "program = \"echo\"\nexpected_stdout = \"wrong-out\"\nexpected_stderr = \"wrong-err\"\n",
        );

        let tc = make_test_case_root("", "test.toml");
        let results = vec![RunResult::Ran {
            test_case: tc,
            result: Ok(TestOutcome {
                stdout: FieldOutcome::Diff {
                    expected: "wrong-out".to_string(),
                    got: "actual-out".to_string(),
                },
                stderr: FieldOutcome::Diff {
                    expected: "wrong-err".to_string(),
                    got: "actual-err".to_string(),
                },
                exit_code: FieldOutcome::NotChecked(0),
            }),
        }];

        // On stdout: `a` stages Accept, `enter` confirms and advances to stderr.
        // On stderr: `a` stages Accept but `q` quits before confirming.
        let mut input = Cursor::new(b"a\nenter\na\nq\n");
        let mut output = Vec::<u8>::new();

        run_interactive_updates(
            &results,
            PlannedCounts {
                runnable: 1,
                skipped: 0,
            },
            tmp.path(),
            &mut input,
            &mut output,
            80,
            20,
            ConfigStats::default(),
            std::time::Duration::ZERO,
        )
        .unwrap();

        let updated = tmp.read("test.toml");
        assert!(
            updated.contains("expected_stdout = \"actual-out\""),
            "stdout was confirmed and should be saved, got:\n{updated}"
        );
        assert!(
            updated.contains("expected_stderr = \"wrong-err\""),
            "stderr was staged but not confirmed and must not be saved, got:\n{updated}"
        );
        assert!(String::from_utf8_lossy(&output).contains("Updated test.toml (stdout)"));
    }
}
