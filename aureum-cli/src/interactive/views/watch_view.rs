use crate::interactive::keys;
use crate::interactive::theme;
use crate::interactive::views::diff_view;
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
}

/// Shows the idle/watching screen until a file change arrives, the user presses `r`
/// (review, only when failures exist), or `q` (quit).
pub(crate) fn run_watch_idle(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ctx: &WatchIdleContext<'_>,
    change_rx: &Receiver<usize>,
) -> io::Result<IdleOutcome> {
    let passed = ctx.run_results.iter().filter(|r| r.is_success()).count();
    let total = ctx.run_results.len();
    let failed = total - passed;

    loop {
        terminal
            .draw(|frame| render_idle(frame, passed, total, ctx.finished_at, ctx.duration))
            .map_err(io::Error::other)?;

        // Poll for key events with a short timeout so we can also check the channel.
        match crossterm::event::poll(Duration::from_millis(50)) {
            Ok(true) => {
                if let Ok(Event::Key(key)) = crossterm::event::read()
                    && key.kind == KeyEventKind::Press
                {
                    match key.code {
                        KeyCode::Char('r') if failed > 0 => return Ok(IdleOutcome::Review),
                        KeyCode::Char('t') => return Ok(IdleOutcome::Rerun),
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

fn render_idle(frame: &mut Frame, passed: usize, total: usize, finished_at: &str, duration: &str) {
    let failed = total - passed;
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
    let left = "  Watching for changes";
    let passed_str = format!("{} passed", passed);
    let failed_str = format!("{} failed", failed);
    let right_len = passed_str.len() + 2 + failed_str.len() + 2;
    let gap = w.saturating_sub(left.len() + right_len).max(1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(left),
            Span::raw(" ".repeat(gap)),
            Span::styled(passed_str, Style::default().fg(Color::Green)),
            Span::raw("  "),
            Span::styled(failed_str, Style::default().fg(Color::Red)),
            Span::raw("  "),
        ])),
        inner_chunks[0],
    );

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

    // Status title + border color
    let border_color = if failed == 0 {
        Color::Green
    } else {
        Color::Red
    };
    let status_label = if failed == 0 {
        format!(
            " ✓ All {} {} passed ",
            total,
            if total == 1 { "test" } else { "tests" }
        )
    } else {
        format!(
            " ✗ {} {} failed ",
            failed,
            if failed == 1 { "test" } else { "tests" }
        )
    };
    let title_style = Style::default()
        .fg(border_color)
        .add_modifier(Modifier::BOLD);
    let title = Line::from(Span::styled(status_label.clone(), title_style));

    // Box sizing: fixed padding on each side, wide enough for title and minimum width
    const BOX_PADDING: usize = 2;
    const MIN_BOX_TOTAL_W: usize = 31;
    let content_block_w = label_w + 2 + value_w;
    let title_text_len = status_label.len();
    let box_inner_w = (content_block_w + BOX_PADDING * 2)
        .max(title_text_len + 2)
        .max(MIN_BOX_TOTAL_W - 2);
    let box_total_w = (box_inner_w + 2) as u16;

    // Body: vertically centre the box + optional hint below
    let body_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(6), // box: top border + spacer + 2 rows + spacer + bottom border
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
        .border_style(Style::default().fg(border_color))
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

    // Hint below the box when tests are failing
    if failed > 0 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Press r to review",
                Style::default().fg(Color::Red),
            )))
            .alignment(Alignment::Center),
            body_chunks[2],
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
                Span::raw("  t: run tests   "),
                Span::styled("r: review failures", r_style),
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
) -> io::Result<IdleOutcome> {
    let passed = run_results.iter().filter(|r| r.is_success()).count();
    let total = run_results.len();
    let failed = total - passed;

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).map_err(io::Error::other)?;

    terminal
        .draw(|frame| render_idle(frame, passed, total, finished_at, duration))
        .map_err(io::Error::other)?;
    diff_view::write_frame(terminal.backend(), width, height, writer, emit_separator)?;

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
        if let Some(key) = diff_view::parse_key_name(key_name) {
            match key {
                KeyCode::Char('r') if failed > 0 => return Ok(IdleOutcome::Review),
                KeyCode::Char('t') => return Ok(IdleOutcome::Rerun),
                KeyCode::Char('q') => return Ok(IdleOutcome::Quit),
                _ => {}
            }
            terminal
                .draw(|frame| render_idle(frame, passed, total, finished_at, duration))
                .map_err(io::Error::other)?;
            diff_view::write_frame(terminal.backend(), width, height, writer, true)?;
        }
    }
}
