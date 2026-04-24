mod accept;
mod action;
mod diff_content;
mod diff_render;
mod diff_view;
mod field;
mod list_view;
mod progress_view;
mod review_loop;
mod style;

use accept::update_test_expectations;
use field::FieldDecisions;
use progress_view::run_tests_with_progress;
use review_loop::{HeadlessDriver, LiveDriver, run_review_loop};

use aureum::{RunResult, TestCaseWithExpectations, TestResult};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, BufRead, Write};
use std::path::Path;

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
    };
    run_review_loop(
        &failed,
        &mut past_decisions,
        passed_count,
        total_count,
        &mut driver,
    )?;

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
    let Some(run_results) = run_tests_with_progress(terminal, test_cases, parallel, current_dir)?
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
    let mut driver = LiveDriver { terminal };
    run_review_loop(
        &failed_pairs,
        &mut past_decisions,
        passed_count,
        total_count,
        &mut driver,
    )?;

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
    if decisions.stdout == Some(true) {
        names.push("stdout");
    }
    if decisions.stderr == Some(true) {
        names.push("stderr");
    }
    if decisions.exit_code == Some(true) {
        names.push("exit_code");
    }
    names.join(", ")
}

#[cfg(test)]
mod test_helpers;

#[cfg(test)]
mod tests {
    use super::test_helpers::{TempDir, make_test_case_root};
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

        let mut input = Cursor::new(b"y\nenter\n");
        let mut output = Vec::<u8>::new();

        run_interactive_updates(&results, tmp.path(), &mut input, &mut output, 80, 20).unwrap();

        assert_eq!(tmp.read("expected_stdout.txt"), "actual\n");
        assert!(String::from_utf8_lossy(&output).contains("Updated test.toml (stdout)"));
    }
}
