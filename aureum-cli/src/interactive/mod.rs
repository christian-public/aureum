mod accept;
mod action;
mod field;
mod keys;
mod review_loop;
mod theme;
mod utils;
mod views;

use crate::interactive::views::progress_view;
use crate::interactive::views::watch_view::{self, IdleOutcome, WatchIdleContext, run_watch_idle};
use crate::utils::time;
use accept::update_test_expectations;
use field::{FieldDecision, FieldDecisions};
use review_loop::{HeadlessDriver, LiveDriver, ReviewOutcome, run_review_loop};

use aureum::{self, RunResult, TestCaseWithExpectations, TestResult};
use chrono::Local;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::time::Instant;

type TuiSessionResult = (Vec<RunResult>, Vec<(usize, FieldDecisions)>);

/// Headless TUI review session used by `--record`. Renders each failing test's diff view
/// into a `TestBackend` of size `width × height`. Key names are read from `reader` (one per
/// line) and the resulting frames are written to `writer`, separated by `---`.
pub fn run_interactive_updates<R, W>(
    run_results: &[RunResult],
    current_dir: &Path,
    reader: &mut R,
    writer: &mut W,
    width: u16,
    height: u16,
) -> io::Result<()>
where
    R: BufRead,
    W: Write,
{
    let failed: Vec<(&RunResult, &TestResult)> = run_results
        .iter()
        .filter_map(|r| match &r.result {
            Ok(t) if !t.is_success() => Some((r, t)),
            _ => None,
        })
        .collect();

    let total = failed.len();
    if total == 0 {
        return Ok(());
    }

    let total_count = run_results.len();
    let passed_count = run_results.iter().filter(|r| r.is_success()).count();

    let mut past_decisions: Vec<Option<FieldDecisions>> = vec![None; total];
    let mut driver = HeadlessDriver {
        width,
        height,
        reader,
        writer,
        emit_separator: false,
        watch_mode: false,
    };
    run_review_loop(
        &failed,
        &mut past_decisions,
        passed_count,
        total_count,
        &mut driver,
    )?;
    // Non-watch session: BackToWatch never occurs; past_decisions already populated.

    let accepted: Vec<(&RunResult, &TestResult, FieldDecisions)> = past_decisions
        .iter()
        .enumerate()
        .filter_map(|(idx, dec_opt)| {
            let decisions = (*dec_opt)?;
            if !decisions.any_accepted() {
                return None;
            }
            let (rr, tr) = failed[idx];
            Some((rr, tr, decisions))
        })
        .collect();

    if !accepted.is_empty() {
        writeln!(writer)?;
        for (run_result, test_result, decisions) in &accepted {
            update_test_expectations(&run_result.test_case, test_result, current_dir, decisions)?;
            writeln!(
                writer,
                "Updated {} ({})",
                run_result.test_case.id(),
                accepted_field_names(decisions)
            )?;
        }
    }

    Ok(())
}

/// Headless watch session for `--watch --record`. Runs tests, shows the idle screen
/// headlessly, and loops on file-change commands from `reader`. The special key name
/// `"file-change"` simulates a watcher event; all other key names are forwarded to the
/// idle or review views. Frames are written to `writer` separated by `---`.
#[allow(clippy::too_many_arguments)]
pub fn run_interactive_updates_with_watch<R, W>(
    load_test_cases: &dyn Fn() -> Vec<TestCaseWithExpectations>,
    parallel: bool,
    current_dir: &Path,
    _watch_pattern: &str,
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
        let test_cases = load_test_cases();
        let run_results = aureum::run_test_cases(&test_cases, parallel, current_dir, &|_, _, _| {});

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
            )?;
            emit_separator = true;

            match outcome {
                IdleOutcome::Rerun => continue 'rerun,
                IdleOutcome::Quit => return Ok(run_results),
                IdleOutcome::Review => {
                    let total_count = run_results.len();
                    let passed_count = run_results.iter().filter(|r| r.is_success()).count();
                    let failed_results: Vec<(usize, &RunResult, &TestResult)> = run_results
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, rr)| match &rr.result {
                            Ok(tr) if !tr.is_success() => Some((idx, rr, tr)),
                            _ => None,
                        })
                        .collect();

                    if failed_results.is_empty() {
                        continue;
                    }

                    let failed_pairs: Vec<(&RunResult, &TestResult)> = failed_results
                        .iter()
                        .map(|(_, rr, tr)| (*rr, *tr))
                        .collect();
                    let mut past_decisions = vec![None; failed_pairs.len()];
                    let mut driver = HeadlessDriver {
                        width,
                        height,
                        reader,
                        writer,
                        emit_separator,
                        watch_mode: true,
                    };

                    let outcome = run_review_loop(
                        &failed_pairs,
                        &mut past_decisions,
                        passed_count,
                        total_count,
                        &mut driver,
                    )?;
                    emit_separator = driver.emit_separator;

                    let decisions: Vec<(usize, FieldDecisions)> = match outcome {
                        ReviewOutcome::Quit => return Ok(run_results),
                        ReviewOutcome::BackToWatch(d) => d
                            .into_iter()
                            .filter(|(_, dec)| dec.any_accepted())
                            .map(|(i, dec)| (failed_results[i].0, dec))
                            .collect(),
                        ReviewOutcome::Done => past_decisions
                            .iter()
                            .enumerate()
                            .filter_map(|(i, dec_opt)| {
                                let dec = (*dec_opt)?;
                                if !dec.any_accepted() {
                                    return None;
                                }
                                Some((failed_results[i].0, dec))
                            })
                            .collect(),
                    };

                    for (idx, decisions) in &decisions {
                        let run_result = &run_results[*idx];
                        let Ok(test_result) = &run_result.result else {
                            continue;
                        };
                        update_test_expectations(
                            &run_result.test_case,
                            test_result,
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
pub fn run_with_progress_review_and_watch(
    load_test_cases: &dyn Fn() -> Vec<TestCaseWithExpectations>,
    parallel: bool,
    current_dir: &Path,
    watch_pattern: &str,
) -> io::Result<Vec<RunResult>> {
    let watch_handle = crate::watch::start_watcher(watch_pattern, current_dir)?;

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
    );

    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen);

    let run_results = result?.unwrap_or_default();
    Ok(run_results)
}

fn run_watch_interactive_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    load_test_cases: &dyn Fn() -> Vec<TestCaseWithExpectations>,
    parallel: bool,
    current_dir: &Path,
    change_rx: &Receiver<usize>,
) -> io::Result<Option<Vec<RunResult>>> {
    'rerun: loop {
        let test_cases = load_test_cases();
        let run_start = Instant::now();
        // Run tests with live progress view.
        let Some(last_results) =
            progress_view::run_tests_with_progress(terminal, &test_cases, parallel, current_dir)?
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
            };
            match run_watch_idle(terminal, &idle_ctx, change_rx)? {
                IdleOutcome::Rerun => continue 'rerun,
                IdleOutcome::Quit => return Ok(Some(last_results)),
                IdleOutcome::Review => {
                    // Enter review loop (watch mode = true so Esc returns here).
                    let Some(accepted) = run_watch_review(terminal, &last_results)? else {
                        return Ok(Some(last_results)); // user pressed q
                    };
                    for (idx, decisions) in &accepted {
                        let run_result = &last_results[*idx];
                        let Ok(test_result) = &run_result.result else {
                            continue;
                        };
                        update_test_expectations(
                            &run_result.test_case,
                            test_result,
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
fn run_watch_review(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    run_results: &[RunResult],
) -> io::Result<Option<Vec<(usize, FieldDecisions)>>> {
    let failed_results: Vec<(usize, &RunResult, &TestResult)> = run_results
        .iter()
        .enumerate()
        .filter_map(|(idx, rr)| {
            if let Ok(tr) = &rr.result {
                if !tr.is_success() {
                    Some((idx, rr, tr))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    if failed_results.is_empty() {
        return Ok(Some(vec![]));
    }

    let total_count = run_results.len();
    let passed_count = run_results.iter().filter(|r| r.is_success()).count();
    let failed_pairs: Vec<(&RunResult, &TestResult)> = failed_results
        .iter()
        .map(|(_, rr, tr)| (*rr, *tr))
        .collect();

    let mut past_decisions: Vec<Option<FieldDecisions>> = vec![None; failed_pairs.len()];
    let mut driver = LiveDriver {
        terminal,
        watch_mode: true,
    };

    let outcome = run_review_loop(
        &failed_pairs,
        &mut past_decisions,
        passed_count,
        total_count,
        &mut driver,
    )?;

    // Map from failed-array index to run_results index, collecting accepted decisions.
    let map_decisions = |pairs: &[(usize, FieldDecisions)]| -> Vec<(usize, FieldDecisions)> {
        pairs
            .iter()
            .filter(|(_, dec)| dec.any_accepted())
            .map(|&(i, dec)| (failed_results[i].0, dec))
            .collect()
    };

    match outcome {
        ReviewOutcome::Quit => Ok(None),
        ReviewOutcome::BackToWatch(d) => Ok(Some(map_decisions(&d))),
        ReviewOutcome::Done => {
            let collected: Vec<(usize, FieldDecisions)> = past_decisions
                .iter()
                .enumerate()
                .filter_map(|(i, dec_opt)| {
                    let dec = (*dec_opt)?;
                    if !dec.any_accepted() {
                        return None;
                    }
                    Some((failed_results[i].0, dec))
                })
                .collect();
            Ok(Some(collected))
        }
    }
}

/// Full interactive session for a real terminal: shows live test progress, then lets the user
/// review and accept failures one by one. Enters/leaves alternate screen internally.
pub fn run_with_progress_and_review(
    test_cases: &[TestCaseWithExpectations],
    parallel: bool,
    current_dir: &Path,
) -> io::Result<Vec<RunResult>> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(io::Error::other)?;

    let result = run_tui_session(&mut terminal, test_cases, parallel, current_dir);

    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen);

    let Some((run_results, accepted_result_indices)) = result? else {
        // User quit during progress; background test thread detached.
        std::process::exit(1);
    };

    for &(idx, decisions) in &accepted_result_indices {
        let run_result = &run_results[idx];
        let Ok(test_result) = &run_result.result else {
            continue;
        };
        update_test_expectations(&run_result.test_case, test_result, current_dir, &decisions)?;
    }

    Ok(run_results)
}

fn run_tui_session(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    test_cases: &[TestCaseWithExpectations],
    parallel: bool,
    current_dir: &Path,
) -> io::Result<Option<TuiSessionResult>> {
    let Some(run_results) =
        progress_view::run_tests_with_progress(terminal, test_cases, parallel, current_dir)?
    else {
        return Ok(None); // user quit; background thread detached
    };

    let total_count = test_cases.len();
    let passed_count = run_results.iter().filter(|r| r.is_success()).count();

    let failed_results: Vec<(usize, &RunResult, &TestResult)> = run_results
        .iter()
        .enumerate()
        .filter_map(|(idx, rr)| {
            if let Ok(tr) = &rr.result {
                if !tr.is_success() {
                    Some((idx, rr, tr))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();
    let total_failed = failed_results.len();

    let failed_pairs: Vec<(&RunResult, &TestResult)> = failed_results
        .iter()
        .map(|(_, rr, tr)| (*rr, *tr))
        .collect();

    let mut past_decisions: Vec<Option<FieldDecisions>> = vec![None; total_failed];
    let mut driver = LiveDriver {
        terminal,
        watch_mode: false,
    };
    run_review_loop(
        &failed_pairs,
        &mut past_decisions,
        passed_count,
        total_count,
        &mut driver,
    )?;
    // Non-watch session: BackToWatch/Quit both just proceed to collect decisions.

    let accepted_result_indices: Vec<(usize, FieldDecisions)> = past_decisions
        .iter()
        .enumerate()
        .filter_map(|(i, dec_opt)| {
            let decisions = (*dec_opt)?;
            if !decisions.any_accepted() {
                return None;
            }
            Some((failed_results[i].0, decisions))
        })
        .collect();

    Ok(Some((run_results, accepted_result_indices)))
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
    use aureum::{TestCase, TestCaseExpectations, ValueComparison};
    use std::io::Cursor;

    fn failing_run_result_stdout(
        test_case: TestCase,
        expectations: TestCaseExpectations,
        expected: &str,
        got: &str,
    ) -> RunResult {
        RunResult {
            test_case,
            expectations,
            result: Ok(TestResult {
                stdout: ValueComparison::Diff {
                    expected: expected.to_string(),
                    got: got.to_string(),
                },
                stderr: ValueComparison::NotChecked("".to_string()),
                exit_code: ValueComparison::NotChecked(0),
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
        let expectations = TestCaseExpectations {
            stdout: Some("expected".to_string()),
            stderr: None,
            exit_code: None,
        };
        let results = vec![failing_run_result_stdout(
            tc,
            expectations,
            "wrong\n",
            "actual\n",
        )];

        let mut input = Cursor::new(b"a\nenter\n");
        let mut output = Vec::<u8>::new();

        run_interactive_updates(&results, tmp.path(), &mut input, &mut output, 80, 20).unwrap();

        assert_eq!(tmp.read("expected_stdout.txt"), "actual\n");
        assert!(String::from_utf8_lossy(&output).contains("Updated test.toml (stdout)"));
    }
}
