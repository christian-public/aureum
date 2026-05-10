use aureum::RunResult;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::io::{self, BufRead, Write};

use crate::counts::TestCounts;
use crate::interactive::action::Action;
use crate::interactive::field::FieldDecisions;
use crate::interactive::keys;
use crate::interactive::theme;
use crate::interactive::utils::widgets;
use crate::interactive::views::diff_view;
use crate::utils::shell;

pub(crate) struct ErrorViewContext<'a> {
    pub index: usize,
    pub total: usize,
    pub run_result: &'a RunResult,
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
    let path = ctx.run_result.test_case.id().to_string();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(path, Style::default().add_modifier(Modifier::BOLD)),
        ])),
        inner_chunks[2],
    );

    // Program row — dim `$ program args`
    let program = build_program_display(ctx.run_result);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("$ ".to_owned(), theme::dim()),
            Span::styled(program, theme::dim()),
        ])),
        inner_chunks[3],
    );

    // Content: error message
    let error_text = match &ctx.run_result.result {
        Err(e) => e.to_string(),
        Ok(_) => String::new(),
    };
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

fn build_program_display(run_result: &RunResult) -> String {
    let test_case = &run_result.test_case;
    let path = &test_case.program_path;
    let is_exe = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    let name = if is_exe {
        path.file_stem()
    } else {
        path.file_name()
    }
    .map(|n| shell::shell_quote(&n.to_string_lossy()))
    .unwrap_or_default();
    let display = if test_case.arguments.is_empty() {
        name
    } else {
        let args: Vec<String> = test_case
            .arguments
            .iter()
            .map(|a| shell::shell_quote(a))
            .collect();
        format!("{name} {}", args.join(" "))
    };
    display.replace('\n', "\\n")
}

// ── ErrorIo trait: abstracts I/O so the event loop is shared ─────────────────

trait ErrorIo {
    fn render(&mut self, ctx: &ErrorViewContext<'_>) -> io::Result<()>;
    fn next_key(&mut self) -> io::Result<Option<KeyEvent>>;
}

// Live terminal implementation

struct LiveErrorIo<'a> {
    terminal: &'a mut Terminal<CrosstermBackend<io::Stdout>>,
}

impl ErrorIo for LiveErrorIo<'_> {
    fn render(&mut self, ctx: &ErrorViewContext<'_>) -> io::Result<()> {
        self.terminal
            .draw(|frame| render_error(frame, ctx))
            .map_err(io::Error::other)?;
        Ok(())
    }

    fn next_key(&mut self) -> io::Result<Option<KeyEvent>> {
        loop {
            if let Event::Key(key) = crossterm::event::read()?
                && key.kind == KeyEventKind::Press
            {
                return Ok(Some(key));
            }
        }
    }
}

// Headless TestBackend implementation for --record

struct HeadlessErrorIo<'a, R: BufRead, W: Write> {
    terminal: Terminal<TestBackend>,
    reader: &'a mut R,
    writer: &'a mut W,
    width: u16,
    height: u16,
    /// `Some(sep)` before the first render; `None` after (always emit `---`).
    pending_separator: Option<bool>,
}

impl<R: BufRead, W: Write> ErrorIo for HeadlessErrorIo<'_, R, W> {
    fn render(&mut self, ctx: &ErrorViewContext<'_>) -> io::Result<()> {
        self.terminal
            .draw(|frame| render_error(frame, ctx))
            .map_err(io::Error::other)?;
        let sep = self.pending_separator.take().unwrap_or(true);
        diff_view::write_frame(
            self.terminal.backend(),
            self.width,
            self.height,
            self.writer,
            sep,
        )
    }

    fn next_key(&mut self) -> io::Result<Option<KeyEvent>> {
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
            if let Some(code) = diff_view::parse_key_name(key_name) {
                return Ok(Some(KeyEvent::new(code, KeyModifiers::NONE)));
            }
        }
    }
}

// ── Unified event loop ───────────────────────────────────────────────────────

fn run_error_view(io: &mut impl ErrorIo, ctx: &ErrorViewContext<'_>) -> io::Result<Action> {
    let is_last = ctx.index == ctx.total;
    io.render(ctx)?;

    loop {
        let Some(key) = io.next_key()? else {
            return Ok(Action::Quit);
        };
        if keys::is_quit_key(&key) {
            return Ok(Action::Quit);
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

// ── Public API ───────────────────────────────────────────────────────────────

/// Interactive error view for a real terminal.
pub(crate) fn run_tui_error(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ctx: &ErrorViewContext<'_>,
) -> io::Result<Action> {
    run_error_view(&mut LiveErrorIo { terminal }, ctx)
}

/// Headless error view for `--record` mode. Reads key names from `reader`, emits frames
/// to `writer` separated by `---`.
pub(crate) fn record_error_view<R: BufRead, W: Write>(
    ctx: &ErrorViewContext<'_>,
    width: u16,
    height: u16,
    reader: &mut R,
    writer: &mut W,
    separator_before_first_frame: bool,
) -> io::Result<Action> {
    let backend = TestBackend::new(width, height);
    let terminal = Terminal::new(backend).map_err(io::Error::other)?;
    run_error_view(
        &mut HeadlessErrorIo {
            terminal,
            reader,
            writer,
            width,
            height,
            pending_separator: Some(separator_before_first_frame),
        },
        ctx,
    )
}
