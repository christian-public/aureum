use aureum::{RunResult, TestResult};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::Terminal;
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::text::Line;
use std::io::{self, BufRead, Write};

use crate::interactive::action::Action;
use crate::interactive::diff_content;
use crate::interactive::diff_render;
use crate::interactive::field::{FailingFields, Field, FieldDecisions, OUTPUT_FIELDS};

// ── Tab enum ─────────────────────────────────────────────────────────────────

/// The three content tabs.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(super) enum Tab {
    Expected,
    Got,
    Diff,
}

// ── Context ──────────────────────────────────────────────────────────────────

pub(super) struct DiffViewContext<'a> {
    pub index: usize,
    pub total: usize,
    pub run_result: &'a RunResult,
    pub test_result: &'a TestResult,
    pub passed_count: usize,
    pub total_count: usize,
}

// ── TUI state ────────────────────────────────────────────────────────────────

pub(super) struct TuiState {
    pub(super) active_tab: Tab,
    pub(super) active_field: Field,
    pub(super) scroll: u16,
    /// Per-field decisions; `None` = undecided, `Some(true)` = accept, `Some(false)` = skip.
    pub(super) field_decisions: FieldDecisions,
    /// Show "you must decide this field first" after pressing Enter on an undecided failing field.
    pub(super) show_enter_error: bool,
    /// Tentative y/n for the current field; committed on Enter, discarded on field navigation.
    pub(super) pending_decision: Option<bool>,
}

impl TuiState {
    fn new(test_result: &TestResult, initial_decisions: Option<FieldDecisions>) -> Self {
        TuiState {
            active_tab: Tab::Diff,
            active_field: FailingFields::of(test_result).first(),
            scroll: 0,
            field_decisions: initial_decisions.unwrap_or_default(),
            show_enter_error: false,
            pending_decision: None,
        }
    }
}

// ── Key handling ─────────────────────────────────────────────────────────────

enum KeyResult {
    Continue,
    TryProceed,
    Exit(Action),
}

/// Pure key-handler: mutates `state` and returns whether to continue or exit.
fn apply_key(state: &mut TuiState, key: KeyCode, test_result: &TestResult) -> KeyResult {
    match key {
        KeyCode::Right => {
            // Navigating to a different field discards any pending y/n decision.
            state.pending_decision = None;
            if let Some(next) = state.active_field.next() {
                state.active_field = next;
                state.scroll = 0;
            }
        }
        KeyCode::Left => {
            state.pending_decision = None;
            if let Some(prev) = state.active_field.prev() {
                state.active_field = prev;
                state.scroll = 0;
            }
        }
        KeyCode::Up => {
            state.scroll = state.scroll.saturating_sub(1);
        }
        KeyCode::Down => {
            state.scroll = state.scroll.saturating_add(1);
        }
        KeyCode::Char('1') => {
            state.active_tab = Tab::Expected;
            state.scroll = 0;
        }
        KeyCode::Char('2') => {
            state.active_tab = Tab::Got;
            state.scroll = 0;
        }
        KeyCode::Char('3') => {
            state.active_tab = Tab::Diff;
            state.scroll = 0;
        }
        KeyCode::Char('i') => {
            state.pending_decision = None;
            state.active_field = Field::Stdin;
            state.scroll = 0;
        }
        KeyCode::Char('o') => {
            state.pending_decision = None;
            state.active_field = Field::Stdout;
            state.scroll = 0;
        }
        KeyCode::Char('e') => {
            state.pending_decision = None;
            state.active_field = Field::Stderr;
            state.scroll = 0;
        }
        KeyCode::Char('x') => {
            state.pending_decision = None;
            state.active_field = Field::ExitCode;
            state.scroll = 0;
        }
        KeyCode::Char('y') => {
            let failing = FailingFields::of(test_result);
            if state.active_field.is_output() && failing.is_failing(state.active_field) {
                let committed = state.field_decisions.get(state.active_field);
                state.pending_decision = (committed != Some(true)).then_some(true);
                state.show_enter_error = false;
            }
            return KeyResult::Continue; // skip catch-all so pending is not cleared
        }
        KeyCode::Char('n') => {
            let failing = FailingFields::of(test_result);
            if state.active_field.is_output() && failing.is_failing(state.active_field) {
                let committed = state.field_decisions.get(state.active_field);
                state.pending_decision = (committed != Some(false)).then_some(false);
                state.show_enter_error = false;
            }
            return KeyResult::Continue;
        }
        KeyCode::Enter => return KeyResult::TryProceed,
        KeyCode::Char('l') => return KeyResult::Exit(Action::ShowList(state.field_decisions)),
        KeyCode::Char('p') => return KeyResult::Exit(Action::Previous(state.field_decisions)),
        KeyCode::Char('q') => return KeyResult::Exit(Action::Quit),
        _ => {}
    }
    // Field navigation and all other keys clear any pending y/n and enter-error.
    state.show_enter_error = false;
    state.pending_decision = None;
    KeyResult::Continue
}

// ── Decision logic ───────────────────────────────────────────────────────────

/// Returns true if Enter would proceed to the next test, false if to the next field.
/// Simulates committing `pending_decision` before checking.
pub(super) fn proceeds_to_next_test(
    active_field: Field,
    pending_decision: Option<bool>,
    field_decisions: FieldDecisions,
    failing: FailingFields,
) -> bool {
    let mut decisions = field_decisions;
    decisions.set(
        active_field,
        pending_decision.or_else(|| field_decisions.get(active_field)),
    );
    decisions.all_decided_for_failing(failing)
}

/// Returns the status message shown in the right-hand panel.
pub(super) fn compute_status(
    field_decisions: FieldDecisions,
    active_field: Field,
    pending_decision: Option<bool>,
    show_enter_error: bool,
    failing: FailingFields,
    is_last: bool,
) -> &'static str {
    if pending_decision.is_some() {
        return if proceeds_to_next_test(active_field, pending_decision, field_decisions, failing) {
            if is_last {
                "Press Enter to confirm and finish."
            } else {
                "Press Enter to confirm and go to the next test."
            }
        } else {
            "Press Enter to confirm and go to the next field."
        };
    }
    if field_decisions.all_decided_for_failing(failing) {
        return if is_last {
            "Press Enter to finish."
        } else {
            "Press Enter to go to the next test."
        };
    }
    if show_enter_error {
        return if is_last {
            "You need to accept [y] or skip [n] the current field before finishing."
        } else {
            "You need to accept [y] or skip [n] the current field before continuing to the next."
        };
    }
    ""
}

/// Commits any pending y/n decision, then checks whether all failing fields are decided.
/// Returns `Some(Action::Proceed(...))` if ready to advance, or `None` (and navigates to
/// the next undecided field) if not.
fn try_proceed(state: &mut TuiState, test_result: &TestResult) -> Option<Action> {
    if let Some(pending) = state.pending_decision.take() {
        state.field_decisions.set(state.active_field, Some(pending));
    }
    let failing = FailingFields::of(test_result);
    if state.field_decisions.all_decided_for_failing(failing) {
        return Some(Action::Proceed(state.field_decisions));
    }
    handle_proceed(state, failing);
    None
}

/// Finds the first output field that is failing and has no decision yet.
/// The scan starts immediately after `after_field` (wrapping stdout→stderr→exit_code);
/// `None` starts from the beginning.
/// Returns `fallback` if all failing fields are already decided (shouldn't happen when
/// `all_decided_for_failing` returned false).
fn first_undecided_failing_field(
    after_field: Option<Field>,
    decisions: FieldDecisions,
    failing: FailingFields,
    fallback: Field,
) -> Field {
    let start = after_field
        .and_then(|f| OUTPUT_FIELDS.iter().position(|&of| of == f))
        .map_or(0, |i| (i + 1) % 3);
    for offset in 0..3 {
        let field = OUTPUT_FIELDS[(start + offset) % 3];
        if failing.is_failing(field) && decisions.get(field).is_none() {
            return field;
        }
    }
    fallback
}

/// Handles an Enter keypress: advances to the next undecided field, or sets the enter-error
/// flag if the current field is failing and still undecided.
fn handle_proceed(state: &mut TuiState, failing: FailingFields) {
    let on_failing = state.active_field.is_output() && failing.is_failing(state.active_field);
    if on_failing && state.field_decisions.get(state.active_field).is_none() {
        state.show_enter_error = true;
        return;
    }
    state.show_enter_error = false;
    state.active_field = if on_failing {
        first_undecided_failing_field(
            Some(state.active_field),
            state.field_decisions,
            failing,
            state.active_field,
        )
    } else {
        first_undecided_failing_field(None, state.field_decisions, failing, failing.first())
    };
    state.scroll = 0;
}

// ── DiffIo trait: abstracts I/O so the event loop is shared ─────────────────

trait DiffIo {
    fn render(
        &mut self,
        ctx: &DiffViewContext<'_>,
        test_result: &TestResult,
        state: &TuiState,
        content: &[Line<'static>],
    ) -> io::Result<()>;

    /// Returns the next key to process, or `None` on EOF.
    fn next_key(&mut self) -> io::Result<Option<KeyCode>>;
}

// Live terminal implementation

struct LiveDiffIo<'a> {
    terminal: &'a mut Terminal<CrosstermBackend<io::Stdout>>,
}

impl DiffIo for LiveDiffIo<'_> {
    fn render(
        &mut self,
        ctx: &DiffViewContext<'_>,
        test_result: &TestResult,
        state: &TuiState,
        content: &[Line<'static>],
    ) -> io::Result<()> {
        self.terminal
            .draw(|frame| diff_render::render_tui(frame, ctx, test_result, state, content))
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
}

// Headless TestBackend implementation for --record

struct HeadlessDiffIo<'a, R: BufRead, W: Write> {
    terminal: Terminal<TestBackend>,
    reader: &'a mut R,
    writer: &'a mut W,
    width: u16,
    height: u16,
    /// `Some(sep)` before the first render (sep = whether to emit `---` before it);
    /// `None` after the first render (always emit `---`).
    pending_separator: Option<bool>,
}

impl<R: BufRead, W: Write> DiffIo for HeadlessDiffIo<'_, R, W> {
    fn render(
        &mut self,
        ctx: &DiffViewContext<'_>,
        test_result: &TestResult,
        state: &TuiState,
        content: &[Line<'static>],
    ) -> io::Result<()> {
        self.terminal
            .draw(|frame| diff_render::render_tui(frame, ctx, test_result, state, content))
            .map_err(io::Error::other)?;
        let sep = self.pending_separator.take().unwrap_or(true);
        write_frame(
            self.terminal.backend(),
            self.width,
            self.height,
            self.writer,
            sep,
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
            if let Some(key) = parse_key_name(key_name) {
                return Ok(Some(key));
            }
        }
    }
}

// ── Unified event loop ───────────────────────────────────────────────────────

fn run_diff_view(
    io: &mut impl DiffIo,
    ctx: &DiffViewContext<'_>,
    initial_decisions: Option<FieldDecisions>,
) -> io::Result<Action> {
    let test_result = ctx.test_result;
    let mut state = TuiState::new(test_result, initial_decisions);
    let stdin = ctx.run_result.test_case.stdin.as_deref();

    let content =
        diff_content::build_content(test_result, state.active_field, stdin, state.active_tab);
    io.render(ctx, test_result, &state, &content)?;

    loop {
        let Some(key) = io.next_key()? else {
            return Ok(Action::Quit);
        };
        match apply_key(&mut state, key, test_result) {
            KeyResult::Continue => {}
            KeyResult::TryProceed => {
                if let Some(action) = try_proceed(&mut state, test_result) {
                    return Ok(action);
                }
            }
            KeyResult::Exit(action) => return Ok(action),
        }
        let content =
            diff_content::build_content(test_result, state.active_field, stdin, state.active_tab);
        state.scroll = state.scroll.min(content.len().saturating_sub(1) as u16);
        io.render(ctx, test_result, &state, &content)?;
    }
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Interactive diff view for a real terminal.
pub(super) fn run_tui_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ctx: &DiffViewContext<'_>,
    initial_decisions: Option<FieldDecisions>,
) -> io::Result<Action> {
    run_diff_view(&mut LiveDiffIo { terminal }, ctx, initial_decisions)
}

/// Headless diff view for `--record` mode. Reads key names from `reader`, emits frames
/// to `writer` separated by `---`.
pub(super) fn record_view_diff<R: BufRead, W: Write>(
    ctx: &DiffViewContext<'_>,
    width: u16,
    height: u16,
    reader: &mut R,
    writer: &mut W,
    initial_decisions: Option<FieldDecisions>,
    separator_before_first_frame: bool,
) -> io::Result<Action> {
    let backend = TestBackend::new(width, height);
    let terminal = Terminal::new(backend).map_err(io::Error::other)?;
    run_diff_view(
        &mut HeadlessDiffIo {
            terminal,
            reader,
            writer,
            width,
            height,
            pending_separator: Some(separator_before_first_frame),
        },
        ctx,
        initial_decisions,
    )
}

// ── Utilities ────────────────────────────────────────────────────────────────

/// Parses a key name from a line of stdin into a `KeyCode`.
///
/// Single printable characters are passed directly (e.g. `e`, `1`).
/// Special keys use their name (e.g. `up`, `enter`, `esc`).
pub(super) fn parse_key_name(s: &str) -> Option<KeyCode> {
    match s {
        "up" => Some(KeyCode::Up),
        "down" => Some(KeyCode::Down),
        "left" => Some(KeyCode::Left),
        "right" => Some(KeyCode::Right),
        "enter" => Some(KeyCode::Enter),
        "esc" => Some(KeyCode::Esc),
        s if s.chars().count() == 1 => Some(KeyCode::Char(s.chars().next().unwrap())),
        _ => None,
    }
}

/// Renders the TestBackend buffer to a string, with trailing whitespace trimmed per line.
pub(super) fn frame_to_string(backend: &TestBackend, width: u16, height: u16) -> String {
    let buffer = backend.buffer();
    let content = buffer.content();
    let width = width as usize;
    let mut lines: Vec<String> = Vec::with_capacity(height as usize);
    for y in 0..height as usize {
        let mut line = String::with_capacity(width);
        for x in 0..width {
            line.push_str(content[y * width + x].symbol());
        }
        lines.push(line.trim_end().to_string());
    }
    lines.join("\n")
}

/// Writes a rendered frame to `writer`. Prepends `---\n` when `separator` is true.
pub(super) fn write_frame<W: Write>(
    backend: &TestBackend,
    width: u16,
    height: u16,
    writer: &mut W,
    separator: bool,
) -> io::Result<()> {
    if separator {
        writeln!(writer, "---")?;
    }
    writeln!(writer, "{}", frame_to_string(backend, width, height))
}
