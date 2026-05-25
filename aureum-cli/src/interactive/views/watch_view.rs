use crate::counts::{ConfigStats, TestCounts};
use crate::interactive::keys;
use crate::interactive::theme;
use crate::interactive::utils::frame;
use crate::interactive::utils::widgets;
use aureum::RunResult;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::{Frame, Terminal};
use std::io::{self, BufRead, Write};
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// Outcome of the idle/watching screen.
pub(crate) enum IdleOutcome {
    /// User pressed `r` to enter review mode (only possible when failures > 0).
    Review,
    /// A file change was received from the watcher, or the user pressed `t` to re-run.
    Rerun,
    /// User pressed `q` to quit.
    Quit,
}

pub(crate) struct WatchIdleContext<'a> {
    pub run_results: &'a [RunResult],
    pub finished_at: &'a str,
    pub duration: &'a str,
    pub config_stats: ConfigStats,
}

/// Shows the idle/watching screen until a file change arrives, the user presses `r`
/// (review, only when failures exist), or `q` (quit).
pub(crate) fn run_watch_idle(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ctx: &WatchIdleContext<'_>,
    change_rx: &Receiver<usize>,
) -> io::Result<IdleOutcome> {
    let counts = TestCounts::from_results(ctx.run_results, ctx.config_stats);
    let failed = counts.failed;

    loop {
        terminal
            .draw(|frame| render_idle(frame, counts, ctx.finished_at, ctx.duration))
            .map_err(io::Error::other)?;

        // Poll for key events with a short timeout so we can also check the channel.
        match crossterm::event::poll(Duration::from_millis(50)) {
            Ok(true) => {
                if let Ok(Event::Key(key)) = crossterm::event::read()
                    && key.kind == KeyEventKind::Press
                {
                    match key.code {
                        KeyCode::Char('f') if failed > 0 => return Ok(IdleOutcome::Review),
                        KeyCode::Char('r') => return Ok(IdleOutcome::Rerun),
                        _ if keys::is_quit_key(&key) => {
                            return Ok(IdleOutcome::Quit);
                        }
                        _ => {}
                    }
                }
            }
            Ok(false) => {}
            Err(e) => return Err(io::Error::other(e)),
        }

        // Drain all pending change events.
        let mut count = 0usize;
        loop {
            match change_rx.try_recv() {
                Ok(n) => count += n,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Watcher died — treat as quit so the outer loop can exit cleanly.
                    return Ok(IdleOutcome::Quit);
                }
            }
        }
        if count > 0 {
            return Ok(IdleOutcome::Rerun);
        }
    }
}

fn render_idle(frame: &mut Frame, counts: TestCounts, finished_at: &str, duration: &str) {
    let skipped = counts.skipped;
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

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // stats header
            Constraint::Length(1), // divider
            Constraint::Min(1),    // centred status
        ])
        .split(inner_area);

    // Stats header
    let summary = widgets::TestSummary(counts);
    let stats_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(summary.width())])
        .split(inner_chunks[0]);
    frame.render_widget(Paragraph::new("  Watching for changes"), stats_chunks[0]);
    frame.render_widget(summary, stats_chunks[1]);

    // Divider
    frame.render_widget(
        Paragraph::new(format!("├{}┤", "─".repeat(w))),
        ratatui::layout::Rect {
            x: outer_chunks[0].x,
            y: inner_chunks[1].y,
            width: outer_chunks[0].width,
            height: 1,
        },
    );

    // Table data
    let table_rows: [(&str, &str); 2] = [("Last run", finished_at), ("Run time", duration)];
    let label_w = 8usize;
    let value_w = table_rows.iter().map(|(_, v)| v.len()).max().unwrap_or(0);

    // Status title + border style
    let (border_style, title, title_text_len) = if failed > 0 {
        let label = format!(
            " ✗ {} {} failed ",
            failed,
            if failed == 1 { "test" } else { "tests" }
        );
        let style = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
        let width = label.len();
        (
            Style::default().fg(Color::Red),
            Line::from(Span::styled(label, style)),
            width,
        )
    } else if passed == 0 && skipped > 0 {
        let word = if skipped == 1 { "test" } else { "tests" };
        let text = format!(" All {skipped} {word} skipped ");
        let width = " ⊘".len() + text.len();
        (
            Style::default(),
            Line::from(vec![Span::raw(" "), theme::skip_span(), Span::raw(text)]),
            width,
        )
    } else {
        let word = if passed == 1 { "test" } else { "tests" };
        let label = if skipped == 0 {
            format!(" ✓ All {passed} {word} passed ")
        } else {
            format!(" ✓ {passed} {word} passed ")
        };
        let style = Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD);
        let width = label.len();
        (
            Style::default().fg(Color::Green),
            Line::from(Span::styled(label, style)),
            width,
        )
    };

    // Box sizing: fixed padding on each side, wide enough for title and minimum width
    const BOX_PADDING: usize = 2;
    const MIN_BOX_TOTAL_W: usize = 31;
    let content_block_w = label_w + 2 + value_w;
    let box_inner_w = (content_block_w + BOX_PADDING * 2)
        .max(title_text_len + 2)
        .max(MIN_BOX_TOTAL_W - 2);
    let box_total_w = (box_inner_w + 2) as u16;

    // Body: vertically centre the box + optional hint below.
    // The spacer and hint rows are reserved unconditionally so the box keeps
    // the same vertical position whether or not tests are failing.
    let body_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(6), // box: top border + spacer + 2 rows + spacer + bottom border
            Constraint::Length(1), // spacer between box and hint
            Constraint::Length(1), // hint below box (failures only)
            Constraint::Fill(1),
        ])
        .split(inner_chunks[2]);

    // Horizontally centre the box
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(box_total_w),
            Constraint::Fill(1),
        ])
        .split(body_chunks[1]);

    let box_area = h_chunks[1];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(title)
        .title_alignment(Alignment::Center);
    let box_inner = block.inner(box_area);
    frame.render_widget(block, box_area);

    // Table rows inside the box, with a blank line above and below
    let table_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // spacer
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1), // spacer
        ])
        .split(box_inner);

    let pad = " ".repeat((box_inner_w - content_block_w) / 2);
    for (i, (label, value)) in table_rows.iter().enumerate() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(pad.clone()),
                Span::styled(format!("{label:<label_w$}"), theme::dim()),
                Span::raw("  "),
                Span::raw(*value),
            ])),
            table_chunks[1 + i],
        );
    }

    // Hint below the box when tests are failing. Centre it within the same
    // width as the box so the two share one centre axis instead of each being
    // centred independently (which can drift by a column).
    if failed > 0 {
        let hint_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(box_total_w),
                Constraint::Fill(1),
            ])
            .split(body_chunks[3]);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Press f to review failures",
                Style::default().fg(Color::Red),
            )))
            .alignment(Alignment::Center),
            hint_chunks[1],
        );
    }

    // Footer
    let r_style = if failed > 0 {
        Style::default()
    } else {
        theme::dim()
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::raw("  r: run tests   "),
                Span::styled("f: review failures", r_style),
            ]),
            Line::raw("  q: quit"),
        ]),
        outer_chunks[1],
    );
}

/// Headless idle view for `--record` mode. Renders into a `TestBackend` and writes frames
/// to `writer`. Reads key names from `reader` (one per line); the special command
/// `"file-change"` simulates a watcher event and returns `IdleOutcome::FileChange`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_watch_idle<R: BufRead, W: Write>(
    run_results: &[RunResult],
    width: u16,
    height: u16,
    reader: &mut R,
    writer: &mut W,
    emit_separator: bool,
    finished_at: &str,
    duration: &str,
    config_stats: ConfigStats,
) -> io::Result<IdleOutcome> {
    let counts = TestCounts::from_results(run_results, config_stats);
    let failed = counts.failed;

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).map_err(io::Error::other)?;

    terminal
        .draw(|frame| render_idle(frame, counts, finished_at, duration))
        .map_err(io::Error::other)?;
    frame::write_frame(terminal.backend(), width, height, writer, emit_separator)?;

    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(IdleOutcome::Quit);
        }
        let key_name = line.trim();
        if key_name.is_empty() {
            continue;
        }
        if key_name == "file-change" {
            return Ok(IdleOutcome::Rerun);
        }
        if let Some(key) = frame::parse_key_name(key_name) {
            match key {
                KeyCode::Char('f') if failed > 0 => return Ok(IdleOutcome::Review),
                KeyCode::Char('r') => return Ok(IdleOutcome::Rerun),
                KeyCode::Char('q') => return Ok(IdleOutcome::Quit),
                _ => {}
            }
            terminal
                .draw(|frame| render_idle(frame, counts, finished_at, duration))
                .map_err(io::Error::other)?;
            frame::write_frame(terminal.backend(), width, height, writer, true)?;
        }
    }
}
