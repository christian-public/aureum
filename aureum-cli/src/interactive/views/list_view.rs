use crate::interactive::review_loop::FailedTest;
use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::io;

use crate::counts::TestCounts;
use crate::interactive::action::ListAction;
use crate::interactive::field::{FailingFields, FieldDecision, FieldDecisions, OUTPUT_FIELDS};
use crate::interactive::theme;
use crate::interactive::tty::Tty;
use crate::interactive::utils::widgets;

pub(crate) struct ListViewContext<'a> {
    pub failed: &'a [FailedTest<'a>],
    pub past_decisions: &'a [Option<FieldDecisions>],
    pub counts: TestCounts,
}

/// Returns styled spans for the decision indicator box: dim `[`, space, icon, space, dim `]`.
/// `failing` indicates which of the 3 output fields actually have a diff.
fn decision_indicator_spans(
    dec: Option<&FieldDecisions>,
    failing: FailingFields,
) -> [Span<'static>; 5] {
    let icon = match dec {
        None => Span::raw(" "),
        Some(d) => {
            let all_decided = OUTPUT_FIELDS
                .iter()
                .all(|&f| !failing.is_failing(f) || d.get(f) != FieldDecision::Undecided);
            let has_accept = d.any_accepted();
            let has_skip = d.any_skipped();
            match (all_decided, has_accept, has_skip) {
                (_, false, false) => Span::raw(" "), // visited but nothing decided yet
                (true, true, false) => theme::accept_span(),
                (true, false, true) => theme::skip_span(),
                _ => theme::partial_span(), // partial progress or mixed accept+skip
            }
        }
    };
    [
        Span::styled("[", theme::dim()),
        Span::raw(" "),
        icon,
        Span::raw(" "),
        Span::styled("]", theme::dim()),
    ]
}

/// Renders the list view: header stats, divider, scrollable list of failing tests.
fn render_list(frame: &mut Frame, ctx: &ListViewContext<'_>, selection: usize, scroll: usize) {
    use ratatui::layout::Rect;
    let area = frame.area();

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
            Constraint::Min(1),    // list
        ])
        .split(inner_area);

    let w = inner_area.width as usize;

    // Stats row — matches diff view style
    let summary = widgets::TestSummary(ctx.counts);
    let stats_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(summary.width())])
        .split(inner_chunks[0]);
    frame.render_widget(Paragraph::new("  Failed tests"), stats_chunks[0]);
    frame.render_widget(summary, stats_chunks[1]);

    // Divider
    frame.render_widget(
        Paragraph::new(format!("├{}┤", "─".repeat(w))),
        Rect {
            x: outer_chunks[0].x,
            y: inner_chunks[1].y,
            width: outer_chunks[0].width,
            height: 1,
        },
    );

    // List content
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, failed_test) in ctx.failed.iter().enumerate() {
        let is_selected = i == selection;
        let test_id = failed_test.test_case.display_id();
        let dec = ctx.past_decisions.get(i).and_then(|d| d.as_ref());
        let id_style = if is_selected {
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan)
        } else {
            Style::default()
        };
        let arrow_span = if is_selected {
            theme::arrow_span().style(id_style)
        } else {
            Span::raw(" ")
        };
        let mut spans = vec![Span::raw("  "), arrow_span, Span::raw(" ")];
        match failed_test.result.as_ref() {
            Ok(test_outcome) => {
                let failing = FailingFields::of(test_outcome);
                let [b1, sp1, icon, sp2, b2] = decision_indicator_spans(dec, failing);
                spans.extend([b1, sp1, icon, sp2, b2]);
            }
            Err(error) => {
                // Non-reviewable error entry — show [ ! ] in red
                spans.extend([
                    Span::styled("[", theme::dim()),
                    Span::raw(" "),
                    Span::styled("!", Style::default().fg(Color::Red)),
                    Span::raw(" "),
                    Span::styled("]", theme::dim()),
                ]);
                spans.push(Span::raw(" "));
                spans.push(Span::styled(test_id, id_style));
                spans.push(Span::styled(format!(" — {error}"), theme::dim()));
                lines.push(Line::from(spans));
                continue;
            }
        }
        spans.push(Span::raw(" "));
        spans.push(Span::styled(test_id, id_style));
        lines.push(Line::from(spans));
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines)).scroll((scroll as u16, 0)),
        inner_chunks[2],
    );

    // Footer
    let footer = "  ↑↓: navigate   Enter: select test\n  Esc: cancel selection   q: quit";
    frame.render_widget(Paragraph::new(footer), outer_chunks[1]);
}

enum ListKeyResult {
    Continue,
    Exit(ListAction),
}

/// Pure key-handler for the list view: updates `selection` and returns whether to exit.
fn apply_list_key(
    key: KeyCode,
    selection: &mut usize,
    total: usize,
    initial_selection: usize,
) -> ListKeyResult {
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            *selection = selection.saturating_sub(1);
            ListKeyResult::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if *selection + 1 < total {
                *selection += 1;
            }
            ListKeyResult::Continue
        }
        KeyCode::Enter => ListKeyResult::Exit(ListAction::JumpTo(*selection)),
        KeyCode::Esc => ListKeyResult::Exit(ListAction::JumpTo(initial_selection)),
        KeyCode::Char('q') => ListKeyResult::Exit(ListAction::Quit),
        _ => ListKeyResult::Continue,
    }
}

/// Computes scroll offset to keep `selection` visible within `content_height` rows.
fn list_scroll(selection: usize, scroll: usize, content_height: usize) -> usize {
    if selection < scroll {
        selection
    } else if content_height > 0 && selection >= scroll + content_height {
        selection + 1 - content_height
    } else {
        scroll
    }
}

/// chrome: 2 (outer border top+bottom) + 1 (stats) + 1 (divider) + 2 (footer) = 6
const LIST_CHROME_HEIGHT: u16 = 6;

fn content_height(tty: &dyn Tty) -> io::Result<usize> {
    Ok((tty.area()?.height as usize)
        .saturating_sub(LIST_CHROME_HEIGHT as usize)
        .max(1))
}

// ── Public entry point ───────────────────────────────────────────────────────

/// Renders the failing-tests list view and runs its event loop until the user picks a test
/// (Enter), cancels (Esc), or quits (q).
pub(crate) fn run_list_view(
    tty: &mut dyn Tty,
    ctx: &ListViewContext<'_>,
    initial_selection: usize,
) -> io::Result<ListAction> {
    if ctx.failed.is_empty() {
        return Ok(ListAction::Quit);
    }
    let mut selection = initial_selection.min(ctx.failed.len() - 1);
    let mut scroll = 0usize;

    scroll = list_scroll(selection, scroll, content_height(tty)?);
    tty.draw(&mut |frame| render_list(frame, ctx, selection, scroll))?;

    loop {
        let Some(key) = tty.next_key()? else {
            return Ok(ListAction::Quit);
        };
        if let ListKeyResult::Exit(action) = apply_list_key(
            key.code,
            &mut selection,
            ctx.failed.len(),
            initial_selection,
        ) {
            return Ok(action);
        }
        scroll = list_scroll(selection, scroll, content_height(tty)?);
        tty.draw(&mut |frame| render_list(frame, ctx, selection, scroll))?;
    }
}
