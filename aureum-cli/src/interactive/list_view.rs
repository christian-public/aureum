use aureum::{RunResult, TestResult};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};
use std::io::{self, BufRead, Write};

use crate::interactive::action::ListAction;
use crate::interactive::diff_view;
use crate::interactive::field::{FailingFields, FieldDecision, FieldDecisions, OUTPUT_FIELDS};
use crate::interactive::theme;

pub(super) struct ListViewContext<'a> {
    pub failed: &'a [(&'a RunResult, &'a TestResult)],
    pub past_decisions: &'a [Option<FieldDecisions>],
    pub passed_count: usize,
    pub total_count: usize,
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
                (true, true, false) => theme::checkmark_span(),
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
    let failed_count = ctx.total_count - ctx.passed_count;
    let left = "  Failed tests";
    let passed_str = format!("{} passed", ctx.passed_count);
    let failed_str = format!("{} failed", failed_count);
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
        Rect {
            x: outer_chunks[0].x,
            y: inner_chunks[1].y,
            width: outer_chunks[0].width,
            height: 1,
        },
    );

    // List content
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, (run_result, test_result)) in ctx.failed.iter().enumerate() {
        let is_selected = i == selection;
        let test_id = run_result.test_case.id().to_string();
        let dec = ctx.past_decisions.get(i).and_then(|d| d.as_ref());
        let failing = FailingFields::of(test_result);
        let [b1, sp1, icon, sp2, b2] = decision_indicator_spans(dec, failing);
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
        lines.push(Line::from(vec![
            Span::raw("  "),
            arrow_span,
            Span::raw(" "),
            b1,
            sp1,
            icon,
            sp2,
            b2,
            Span::raw(" "),
            Span::styled(test_id, id_style),
        ]));
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

// ── ListIo trait: abstracts I/O so the event loop is shared ─────────────────

trait ListIo {
    fn render(
        &mut self,
        ctx: &ListViewContext<'_>,
        selection: usize,
        scroll: usize,
    ) -> io::Result<()>;
    /// Returns the next key to process, or `None` on EOF.
    fn next_key(&mut self) -> io::Result<Option<KeyCode>>;
    /// chrome: 2 (outer border top+bottom) + 1 (stats) + 1 (divider) + 2 (footer) = 6
    fn content_height(&mut self) -> io::Result<usize>;
}

// Live terminal implementation

struct LiveListIo<'a> {
    terminal: &'a mut Terminal<CrosstermBackend<io::Stdout>>,
}

impl ListIo for LiveListIo<'_> {
    fn render(
        &mut self,
        ctx: &ListViewContext<'_>,
        selection: usize,
        scroll: usize,
    ) -> io::Result<()> {
        self.terminal
            .draw(|frame| render_list(frame, ctx, selection, scroll))
            .map_err(io::Error::other)?;
        Ok(())
    }

    fn next_key(&mut self) -> io::Result<Option<KeyCode>> {
        loop {
            if let Event::Key(key) = crossterm::event::read()?
                && key.kind == KeyEventKind::Press
            {
                return Ok(Some(key.code));
            }
        }
    }

    fn content_height(&mut self) -> io::Result<usize> {
        Ok(
            (self.terminal.size().map_err(io::Error::other)?.height as usize)
                .saturating_sub(6)
                .max(1),
        )
    }
}

// Headless TestBackend implementation for --record

struct HeadlessListIo<'a, R: BufRead, W: Write> {
    terminal: Terminal<TestBackend>,
    reader: &'a mut R,
    writer: &'a mut W,
    width: u16,
    height: u16,
}

impl<R: BufRead, W: Write> ListIo for HeadlessListIo<'_, R, W> {
    fn render(
        &mut self,
        ctx: &ListViewContext<'_>,
        selection: usize,
        scroll: usize,
    ) -> io::Result<()> {
        self.terminal
            .draw(|frame| render_list(frame, ctx, selection, scroll))
            .map_err(io::Error::other)?;
        // Always preceded by a separator since we came from a diff view.
        diff_view::write_frame(
            self.terminal.backend(),
            self.width,
            self.height,
            self.writer,
            true,
        )
    }

    fn next_key(&mut self) -> io::Result<Option<KeyCode>> {
        let mut line = String::new();
        loop {
            line.clear();
            if self.reader.read_line(&mut line)? == 0 {
                return Ok(None);
            }
            let key_name = line.trim();
            if key_name.is_empty() {
                continue;
            }
            if let Some(key) = diff_view::parse_key_name(key_name) {
                return Ok(Some(key));
            }
        }
    }

    fn content_height(&mut self) -> io::Result<usize> {
        Ok((self.height as usize).saturating_sub(6).max(1))
    }
}

// ── Unified event loop ───────────────────────────────────────────────────────

fn run_list_view(
    io: &mut impl ListIo,
    ctx: &ListViewContext<'_>,
    initial_selection: usize,
) -> io::Result<ListAction> {
    if ctx.failed.is_empty() {
        return Ok(ListAction::Quit);
    }
    let mut selection = initial_selection.min(ctx.failed.len() - 1);
    let mut scroll = 0usize;

    scroll = list_scroll(selection, scroll, io.content_height()?);
    io.render(ctx, selection, scroll)?;

    loop {
        let Some(key) = io.next_key()? else {
            return Ok(ListAction::Quit);
        };
        if let ListKeyResult::Exit(action) =
            apply_list_key(key, &mut selection, ctx.failed.len(), initial_selection)
        {
            return Ok(action);
        }
        scroll = list_scroll(selection, scroll, io.content_height()?);
        io.render(ctx, selection, scroll)?;
    }
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Interactive list view for a real terminal.
pub(crate) fn run_list_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ctx: &ListViewContext<'_>,
    initial_selection: usize,
) -> io::Result<ListAction> {
    run_list_view(&mut LiveListIo { terminal }, ctx, initial_selection)
}

/// Headless list view for `--record` mode. Reads key names from `reader`, emits frames
/// to `writer` separated by `---`. Always writes a separator before the first frame.
pub(super) fn record_list_view<R: BufRead, W: Write>(
    ctx: &ListViewContext<'_>,
    width: u16,
    height: u16,
    reader: &mut R,
    writer: &mut W,
    initial_selection: usize,
) -> io::Result<ListAction> {
    let backend = TestBackend::new(width, height);
    let terminal = Terminal::new(backend).map_err(io::Error::other)?;
    run_list_view(
        &mut HeadlessListIo {
            terminal,
            reader,
            writer,
            width,
            height,
        },
        ctx,
        initial_selection,
    )
}
