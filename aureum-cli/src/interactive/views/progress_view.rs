use crate::counts::{ConfigStats, PendingCounts, TestCounts};
use crate::interactive::keys;
use crate::interactive::theme;
use crate::interactive::utils::widgets;
use crate::utils::time;
use aureum::{PendingTestCase, RunResult, RunResultKind, run_test_cases};
use crossterm::event::{Event, KeyEventKind};
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};
use std::io::{self, Write};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Renders the completed progress frame to a `TestBackend` and writes it to `writer`.
/// Used in `--record` mode to capture the final progress state before the review session.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_final_progress_frame<W: Write>(
    run_results: &[RunResult],
    pending_counts: PendingCounts,
    config_stats: ConfigStats,
    width: u16,
    height: u16,
    elapsed: Duration,
    writer: &mut W,
    separator: bool,
) -> io::Result<()> {
    let counts = TestCounts::from_results(run_results, config_stats);

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).map_err(io::Error::other)?;
    terminal
        .draw(|frame| render_progress(frame, pending_counts, counts, elapsed, false))
        .map_err(io::Error::other)?;

    let buffer = terminal.backend().buffer().clone();
    let content = buffer.content();
    let w = width as usize;
    let mut lines = Vec::with_capacity(height as usize);
    for y in 0..height as usize {
        let mut line = String::with_capacity(w);
        for x in 0..w {
            line.push_str(content[y * w + x].symbol());
        }
        lines.push(line.trim_end().to_string());
    }

    if separator {
        writeln!(writer, "---")?;
    }
    writeln!(writer, "{}", lines.join("\n"))
}

/// Returns `Some(results)` when all tests complete, or `None` if the user pressed q.
/// On quit the background thread is detached; the caller should `process::exit` after cleanup.
pub(crate) fn run_tests_with_progress(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    test_cases: &[PendingTestCase],
    parallel: bool,
    current_dir: &Path,
    config_stats: ConfigStats,
    stable_duration: Option<Duration>,
) -> io::Result<Option<Vec<RunResult>>> {
    let pending_counts = PendingCounts::from_pending(test_cases);
    let total = pending_counts.total();
    let (progress_tx, progress_rx) = mpsc::channel::<RunResultKind>();
    let (results_tx, results_rx) = mpsc::channel::<Vec<RunResult>>();

    // Clone data so the thread is 'static and can be detached on quit.
    let test_cases_owned = test_cases.to_vec();
    let current_dir_owned = current_dir.to_path_buf();

    let _handle = thread::spawn(move || {
        let results = run_test_cases(
            &test_cases_owned,
            parallel,
            &current_dir_owned,
            &|_i, run_result| {
                let _ = progress_tx.send(run_result.kind());
            },
        );
        let _ = results_tx.send(results);
    });

    let mut skipped = 0usize;
    let mut passed = 0usize;
    let mut failed = 0usize;
    let start = Instant::now();

    loop {
        let mut all_done = false;
        loop {
            match progress_rx.try_recv() {
                Ok(RunResultKind::Skipped) => skipped += 1,
                Ok(RunResultKind::Passed) => passed += 1,
                Ok(RunResultKind::Failed) => failed += 1,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    all_done = true;
                    break;
                }
            }
        }
        let counts = TestCounts {
            config_stats,
            skipped,
            passed,
            failed,
        };
        terminal
            .draw(|frame| {
                render_progress(
                    frame,
                    pending_counts,
                    counts,
                    stable_duration.unwrap_or_else(|| start.elapsed()),
                    false,
                )
            })
            .map_err(io::Error::other)?;
        if all_done || skipped + passed + failed >= total {
            break;
        }
        match crossterm::event::poll(Duration::from_millis(50)) {
            Ok(true) => {
                if let Ok(Event::Key(key)) = crossterm::event::read()
                    && key.kind == KeyEventKind::Press
                    && keys::is_quit_key(&key)
                {
                    // Show "Stopping..." and return immediately; _handle is detached on drop.
                    terminal
                        .draw(|frame| {
                            render_progress(
                                frame,
                                pending_counts,
                                counts,
                                stable_duration.unwrap_or_else(|| start.elapsed()),
                                true,
                            )
                        })
                        .map_err(io::Error::other)?;
                    return Ok(None);
                }
            }
            Ok(false) => {}
            Err(e) => return Err(io::Error::other(e)),
        }
    }

    // All done — collect results (background thread has already finished).
    let results = results_rx
        .recv()
        .map_err(|_| io::Error::other("test runner closed unexpectedly"))?;
    Ok(Some(results))
}

fn render_progress(
    frame: &mut Frame,
    pending_counts: PendingCounts,
    counts: TestCounts,
    elapsed: Duration,
    stopping: bool,
) {
    let total = pending_counts.total();
    let passed = counts.passed;
    let failed = counts.failed;
    let area = frame.area();

    let outer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(area);

    let outer_block = Block::default().borders(Borders::ALL);
    let inner_area = outer_block.inner(outer_chunks[0]);
    frame.render_widget(outer_block, outer_chunks[0]);

    let w = inner_area.width as usize;

    // Inner layout: header + divider + content
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header: "Running N tests   P passed  F failed"
            Constraint::Length(1), // divider
            Constraint::Min(1),    // centered progress content
        ])
        .split(inner_area);

    // Header row
    let label = if total == 1 { "test" } else { "tests" };
    let left = if pending_counts.runnable == total {
        format!("  Running {} {label}", total)
    } else {
        format!("  Running {} of {} {label}", pending_counts.runnable, total)
    };
    let summary = widgets::TestSummary(counts);
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(summary.width())])
        .split(inner_chunks[0]);
    frame.render_widget(Paragraph::new(left), header_chunks[0]);
    frame.render_widget(summary, header_chunks[1]);

    // Divider with T-junction chars
    frame.render_widget(
        Paragraph::new(format!("├{}┤", "─".repeat(w))),
        ratatui::layout::Rect {
            x: outer_chunks[0].x,
            y: inner_chunks[1].y,
            width: outer_chunks[0].width,
            height: 1,
        },
    );

    // Content area: two lines (bar / info) vertically centred
    let body_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(1), // ████░░░░  progress bar
            Constraint::Length(1), // 20 / 42        1.4s
            Constraint::Fill(1),
        ])
        .split(inner_chunks[2]);

    // Progress bar ─────────────────────────────────────────────────────────
    let runnable = pending_counts.runnable;
    let ran = passed + failed;
    let bar_width = w.saturating_sub(4).clamp(1, 60);

    // When there's nothing to run (everything skipped), show a full bar.
    let filled_chars = (ran * bar_width).checked_div(runnable).unwrap_or(bar_width);

    let empty_chars = bar_width - filled_chars;

    let bar_line = Line::from(vec![
        Span::styled("█".repeat(filled_chars), Style::default().fg(Color::Cyan)),
        Span::styled("░".repeat(empty_chars), theme::dim()),
    ]);
    frame.render_widget(
        Paragraph::new(bar_line).alignment(Alignment::Center),
        body_chunks[1],
    );

    // Info line: "20 / 42" (left)   "1.4s" (right)
    let elapsed_text = if stopping {
        "Stopping…".to_string()
    } else {
        time::format_duration(elapsed)
    };
    // Align info columns to the same width and horizontal position as the bar.
    let bar_left = body_chunks[2].x + (w as u16).saturating_sub(bar_width as u16) / 2;
    let bar_rect = ratatui::layout::Rect {
        x: bar_left,
        y: body_chunks[2].y,
        width: bar_width as u16,
        height: 1,
    };
    let info_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Fill(1),
        ])
        .split(bar_rect);
    frame.render_widget(
        Paragraph::new(format!("{} / {}", ran, runnable)).alignment(Alignment::Left),
        info_chunks[0],
    );

    frame.render_widget(
        Paragraph::new(elapsed_text).alignment(Alignment::Right),
        info_chunks[2],
    );

    // Footer
    let footer_text = if stopping {
        "\n  Stopping..."
    } else {
        "\n  q: quit"
    };
    frame.render_widget(Paragraph::new(footer_text), outer_chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_to_string(
        width: u16,
        height: u16,
        total: usize,
        passed: usize,
        failed: usize,
        elapsed: Duration,
        stopping: bool,
    ) -> String {
        let counts = TestCounts {
            config_stats: ConfigStats::default(),
            skipped: 0,
            passed,
            failed,
        };
        let pending_counts = PendingCounts {
            runnable: total,
            skipped: 0,
        };
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_progress(frame, pending_counts, counts, elapsed, stopping))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let content = buffer.content();
        let w = width as usize;
        let mut lines: Vec<String> = Vec::with_capacity(height as usize);
        for y in 0..height as usize {
            let mut line = String::with_capacity(w);
            for x in 0..w {
                line.push_str(content[y * w + x].symbol());
            }
            lines.push(line.trim_end().to_string());
        }
        lines.join("\n")
    }

    #[test]
    fn test_render_no_tests_complete() {
        let actual = render_to_string(60, 10, 5, 0, 0, Duration::ZERO, false);
        assert_eq!(
            actual,
            [
                "┌──────────────────────────────────────────────────────────┐",
                "│  Running 5 tests          0 skipped  0 passed  0 failed  │",
                "├──────────────────────────────────────────────────────────┤",
                "│                                                          │",
                "│  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │",
                "│  0 / 5                                             0.0s  │",
                "│                                                          │",
                "└──────────────────────────────────────────────────────────┘",
                "",
                "  q: quit",
            ]
            .join("\n")
        );
    }

    #[test]
    fn test_render_partial() {
        let actual = render_to_string(60, 10, 5, 2, 1, Duration::from_millis(1400), false);
        assert_eq!(
            actual,
            [
                "┌──────────────────────────────────────────────────────────┐",
                "│  Running 5 tests          0 skipped  2 passed  1 failed  │",
                "├──────────────────────────────────────────────────────────┤",
                "│                                                          │",
                "│  ████████████████████████████████░░░░░░░░░░░░░░░░░░░░░░  │",
                "│  3 / 5                                             1.4s  │",
                "│                                                          │",
                "└──────────────────────────────────────────────────────────┘",
                "",
                "  q: quit",
            ]
            .join("\n")
        );
    }

    #[test]
    fn test_render_all_passed() {
        let actual = render_to_string(60, 10, 5, 5, 0, Duration::from_millis(1400), false);
        assert_eq!(
            actual,
            [
                "┌──────────────────────────────────────────────────────────┐",
                "│  Running 5 tests          0 skipped  5 passed  0 failed  │",
                "├──────────────────────────────────────────────────────────┤",
                "│                                                          │",
                "│  ██████████████████████████████████████████████████████  │",
                "│  5 / 5                                             1.4s  │",
                "│                                                          │",
                "└──────────────────────────────────────────────────────────┘",
                "",
                "  q: quit",
            ]
            .join("\n")
        );
    }

    #[test]
    fn test_render_stopping() {
        let actual = render_to_string(60, 10, 5, 2, 1, Duration::ZERO, true);
        assert_eq!(
            actual,
            [
                "┌──────────────────────────────────────────────────────────┐",
                "│  Running 5 tests          0 skipped  2 passed  1 failed  │",
                "├──────────────────────────────────────────────────────────┤",
                "│                                                          │",
                "│  ████████████████████████████████░░░░░░░░░░░░░░░░░░░░░░  │",
                "│  3 / 5                                        Stopping…  │",
                "│                                                          │",
                "└──────────────────────────────────────────────────────────┘",
                "",
                "  Stopping...",
            ]
            .join("\n")
        );
    }
}
