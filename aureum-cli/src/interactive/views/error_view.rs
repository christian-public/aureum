use aureum::{RunError, TestCase};
use crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::io;

use crate::counts::TestCounts;
use crate::interactive::action::Action;
use crate::interactive::field::FieldDecisions;
use crate::interactive::keys;
use crate::interactive::theme;
use crate::interactive::tty::Tty;
use crate::interactive::utils::program_display;
use crate::interactive::utils::widgets;

pub(crate) struct ErrorViewContext<'a> {
    pub index: usize,
    pub total: usize,
    pub test_case: &'a TestCase,
    pub error: &'a RunError,
    pub counts: TestCounts,
    pub watch_mode: bool,
}

fn render_error(frame: &mut ratatui::Frame, ctx: &ErrorViewContext<'_>) {
    let area = frame.area();
    let is_last = ctx.index == ctx.total;

    let outer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(area);

    let outer_block = Block::default().borders(Borders::ALL);
    let inner_area = outer_block.inner(outer_chunks[0]);
    frame.render_widget(outer_block, outer_chunks[0]);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // stats
            Constraint::Length(1), // divider
            Constraint::Length(1), // title
            Constraint::Length(1), // program
            Constraint::Length(1), // divider
            Constraint::Min(1),    // content
        ])
        .split(inner_area);

    let w = inner_area.width as usize;

    // Stats row — same layout as diff view
    let left = format!("  Failed test {} of {}", ctx.index, ctx.total);
    let summary = widgets::TestSummary(ctx.counts);
    let stats_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(summary.width())])
        .split(inner_chunks[0]);
    frame.render_widget(Paragraph::new(left), stats_chunks[0]);
    frame.render_widget(summary, stats_chunks[1]);

    // Dividers
    let render_divider = |frame: &mut ratatui::Frame, slot: Rect| {
        frame.render_widget(
            Paragraph::new(format!("├{}┤", "─".repeat(w))),
            Rect {
                x: outer_chunks[0].x,
                y: slot.y,
                width: outer_chunks[0].width,
                height: 1,
            },
        );
    };
    render_divider(frame, inner_chunks[1]);
    render_divider(frame, inner_chunks[4]);

    // Title row
    let test_id = ctx.test_case.display_id();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(test_id, Style::default().add_modifier(Modifier::BOLD)),
        ])),
        inner_chunks[2],
    );

    // Program row — dim `$ program args`
    let program = program_display::build_program_display(ctx.test_case);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("$ ".to_owned(), theme::dim()),
            Span::styled(program, theme::dim()),
        ])),
        inner_chunks[3],
    );

    // Content: error message
    let error_text = ctx.error.to_string();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("Run error: {error_text}"),
                Style::default().fg(Color::Red),
            ),
        ])),
        inner_chunks[5],
    );

    // Footer
    let enter_hint = if is_last { "finish" } else { "next test" };
    let prev_style = if ctx.index == 1 {
        theme::dim()
    } else {
        Style::default()
    };
    let next_style = if is_last {
        theme::dim()
    } else {
        Style::default()
    };
    let mut line2_spans = vec![
        Span::raw("  "),
        Span::styled("p: previous test", prev_style),
        Span::raw("   "),
        Span::styled("n: next test", next_style),
        Span::raw("   l: list tests"),
    ];
    if ctx.watch_mode {
        line2_spans.push(Span::raw("   Esc: end review"));
    }
    line2_spans.push(Span::raw("   q: quit"));
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::raw(format!("  Enter: {enter_hint}")),
            Line::from(line2_spans),
        ])),
        outer_chunks[1],
    );
}

// ── Public entry point ───────────────────────────────────────────────────────

/// Renders the run-error view for one test and runs its event loop until the user presses
/// a key that ends the view.
pub(crate) fn run_error_view(tty: &mut dyn Tty, ctx: &ErrorViewContext<'_>) -> io::Result<Action> {
    let is_last = ctx.index == ctx.total;
    tty.draw(&mut |frame| render_error(frame, ctx))?;

    loop {
        let Some(key) = tty.next_key()? else {
            return Ok(Action::Quit(FieldDecisions::default()));
        };
        if keys::is_quit_key(&key) {
            return Ok(Action::Quit(FieldDecisions::default()));
        }
        match key.code {
            KeyCode::Enter => return Ok(Action::Proceed(FieldDecisions::default())),
            KeyCode::Char('n') if !is_last => {
                return Ok(Action::Proceed(FieldDecisions::default()));
            }
            KeyCode::Char('p') => return Ok(Action::Previous(FieldDecisions::default())),
            KeyCode::Char('l') => return Ok(Action::ShowList(FieldDecisions::default())),
            KeyCode::Esc if ctx.watch_mode => {
                return Ok(Action::BackToWatch(FieldDecisions::default()));
            }
            _ => {}
        }
    }
}
