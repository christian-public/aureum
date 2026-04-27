use aureum::{RunResult, TestResult};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::Terminal;
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::text::Line;
use std::io::{self, BufRead, Write};

use crate::interactive::action::Action;
use crate::interactive::diff_content;
use crate::interactive::diff_render;
use crate::interactive::field::{
    FailingFields, Field, FieldDecision, FieldDecisions, OUTPUT_FIELDS,
};

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
    /// True when running under `--watch`; enables Esc → back-to-watch.
    pub watch_mode: bool,
}

// ── EnterOutcome ─────────────────────────────────────────────────────────────

/// What pressing Enter will do given the current view state.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(super) enum EnterOutcome {
    /// Not on a failing field — Enter navigates to the first failing field.
    JumpToFirstFailing,
    /// On a failing field with no decision yet — Enter will show an error.
    NeedsDecision,
    /// Has a staged decision; will commit it and move to the next failing field.
    ConfirmNextField,
    /// Has a staged decision; will commit it and proceed to the next test.
    ConfirmNextTest,
    /// Has a staged decision; will commit it and finish (last test).
    ConfirmFinish,
    /// Already decided; will move to the next failing field.
    NextField,
    /// Already decided; will proceed to the next test.
    NextTest,
    /// Already decided; will finish (last test).
    Finish,
}

/// Computes what pressing Enter will do given the current state.
pub(super) fn enter_outcome(
    active_field: Field,
    staged_decision: FieldDecision,
    field_decisions: FieldDecisions,
    failing: FailingFields,
    is_last: bool,
) -> EnterOutcome {
    if !active_field.is_output() || !failing.is_failing(active_field) {
        return EnterOutcome::JumpToFirstFailing;
    }
    let has_staged = staged_decision != FieldDecision::Undecided;
    let has_committed = field_decisions.get(active_field) != FieldDecision::Undecided;
    if !has_staged && !has_committed {
        return EnterOutcome::NeedsDecision;
    }
    let is_last_failing = next_failing_field_after(active_field, failing).is_none();
    match (has_staged, is_last_failing, is_last) {
        (true, false, _) => EnterOutcome::ConfirmNextField,
        (true, true, false) => EnterOutcome::ConfirmNextTest,
        (true, true, true) => EnterOutcome::ConfirmFinish,
        (false, false, _) => EnterOutcome::NextField,
        (false, true, false) => EnterOutcome::NextTest,
        (false, true, true) => EnterOutcome::Finish,
    }
}

// ── TUI state ────────────────────────────────────────────────────────────────

pub(super) struct TuiState {
    pub(super) active_tab: Tab,
    pub(super) active_field: Field,
    pub(super) scroll: u16,
    pub(super) field_decisions: FieldDecisions,
    pub(super) show_enter_error: bool,
    /// Tentative a/s for the current field; committed on Enter, discarded on field navigation.
    pub(super) staged_decision: FieldDecision,
    /// Which output fields have a diff; derived once from `TestResult` on construction.
    pub(super) failing: FailingFields,
}

impl TuiState {
    fn new(test_result: &TestResult, initial_decisions: FieldDecisions) -> Self {
        let failing = FailingFields::of(test_result);
        TuiState {
            active_tab: Tab::Diff,
            active_field: failing.first(),
            scroll: 0,
            field_decisions: initial_decisions,
            show_enter_error: false,
            staged_decision: FieldDecision::Undecided,
            failing,
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
fn apply_key(state: &mut TuiState, key: KeyCode, is_last: bool, watch_mode: bool) -> KeyResult {
    match key {
        KeyCode::Right => {
            // Navigating to a different field discards any staged a/s decision.
            state.staged_decision = FieldDecision::Undecided;
            if let Some(next) = state.active_field.next() {
                state.active_field = next;
                state.scroll = 0;
            }
        }
        KeyCode::Left => {
            state.staged_decision = FieldDecision::Undecided;
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
        KeyCode::Char('1') if state.active_field != Field::Stdin => {
            state.active_tab = Tab::Expected;
            state.scroll = 0;
        }
        KeyCode::Char('2') if state.active_field != Field::Stdin => {
            state.active_tab = Tab::Got;
            state.scroll = 0;
        }
        KeyCode::Char('3') if state.active_field != Field::Stdin => {
            state.active_tab = Tab::Diff;
            state.scroll = 0;
        }
        KeyCode::Char('i') => {
            state.staged_decision = FieldDecision::Undecided;
            state.active_field = Field::Stdin;
            state.scroll = 0;
        }
        KeyCode::Char('o') => {
            state.staged_decision = FieldDecision::Undecided;
            state.active_field = Field::Stdout;
            state.scroll = 0;
        }
        KeyCode::Char('e') => {
            state.staged_decision = FieldDecision::Undecided;
            state.active_field = Field::Stderr;
            state.scroll = 0;
        }
        KeyCode::Char('x') => {
            state.staged_decision = FieldDecision::Undecided;
            state.active_field = Field::ExitCode;
            state.scroll = 0;
        }
        KeyCode::Char('a') => {
            if state.active_field.is_output() && state.failing.is_failing(state.active_field) {
                let committed = state.field_decisions.get(state.active_field);
                state.staged_decision = if committed == FieldDecision::Accepted {
                    FieldDecision::Undecided
                } else {
                    FieldDecision::Accepted
                };
                state.show_enter_error = false;
            }
            return KeyResult::Continue; // skip catch-all so staged is not cleared
        }
        KeyCode::Char('s') => {
            if state.active_field.is_output() && state.failing.is_failing(state.active_field) {
                let committed = state.field_decisions.get(state.active_field);
                state.staged_decision = if committed == FieldDecision::Skipped {
                    FieldDecision::Undecided
                } else {
                    FieldDecision::Skipped
                };
                state.show_enter_error = false;
            }
            return KeyResult::Continue;
        }
        KeyCode::Enter => return KeyResult::TryProceed,
        KeyCode::Char('l') => return KeyResult::Exit(Action::ShowList(state.field_decisions)),
        KeyCode::Char('p') => return KeyResult::Exit(Action::Previous(state.field_decisions)),
        KeyCode::Char('n') if !is_last => {
            return KeyResult::Exit(Action::Proceed(state.field_decisions));
        }
        KeyCode::Esc if watch_mode => {
            return KeyResult::Exit(Action::BackToWatch(state.field_decisions));
        }
        KeyCode::Char('q') => return KeyResult::Exit(Action::Quit),
        _ => {}
    }
    // Field navigation and all other keys clear any staged a/s decision and enter-error.
    state.show_enter_error = false;
    state.staged_decision = FieldDecision::Undecided;
    KeyResult::Continue
}

// ── Decision logic ───────────────────────────────────────────────────────────

/// Returns the first failing field strictly after `current` in output-field order, or `None`
/// if `current` is the last failing field.
fn next_failing_field_after(current: Field, failing: FailingFields) -> Option<Field> {
    let pos = OUTPUT_FIELDS.iter().position(|&f| f == current)?;
    OUTPUT_FIELDS[pos + 1..]
        .iter()
        .copied()
        .find(|&f| failing.is_failing(f))
}

/// Returns the status message shown in the right-hand panel.
pub(super) fn compute_status(
    field_decisions: FieldDecisions,
    active_field: Field,
    staged_decision: FieldDecision,
    show_enter_error: bool,
    failing: FailingFields,
    is_last: bool,
) -> &'static str {
    if show_enter_error {
        return if is_last {
            "You need to accept [a] or skip [s] the current field before finishing."
        } else {
            "You need to accept [a] or skip [s] the current field before continuing to the next."
        };
    }
    match enter_outcome(
        active_field,
        staged_decision,
        field_decisions,
        failing,
        is_last,
    ) {
        EnterOutcome::JumpToFirstFailing | EnterOutcome::NeedsDecision => "",
        EnterOutcome::ConfirmNextField => "Press Enter to confirm and go to the next field.",
        EnterOutcome::ConfirmNextTest => "Press Enter to confirm and go to the next test.",
        EnterOutcome::ConfirmFinish => "Press Enter to confirm and finish.",
        EnterOutcome::NextField => "Press Enter to go to the next field.",
        EnterOutcome::NextTest => "Press Enter to go to the next test.",
        EnterOutcome::Finish => "Press Enter to finish.",
    }
}

/// Commits any staged decision and advances to the next failing field or next test.
/// Returns `Some(Action::Proceed(...))` on the last failing field, or `None` (and navigates
/// to the next failing field) otherwise.
fn try_proceed(state: &mut TuiState, is_last: bool) -> Option<Action> {
    let outcome = enter_outcome(
        state.active_field,
        state.staged_decision,
        state.field_decisions,
        state.failing,
        is_last,
    );
    state.show_enter_error = matches!(outcome, EnterOutcome::NeedsDecision);

    if matches!(
        outcome,
        EnterOutcome::ConfirmNextField
            | EnterOutcome::ConfirmNextTest
            | EnterOutcome::ConfirmFinish
    ) {
        let staged = state.staged_decision;
        state.staged_decision = FieldDecision::Undecided;
        state.field_decisions.set(state.active_field, staged);
    }

    match outcome {
        EnterOutcome::JumpToFirstFailing => {
            state.active_field = state.failing.first();
            state.scroll = 0;
            None
        }
        EnterOutcome::NeedsDecision => None,
        EnterOutcome::ConfirmNextField | EnterOutcome::NextField => {
            if let Some(next) = next_failing_field_after(state.active_field, state.failing) {
                state.active_field = next;
                state.scroll = 0;
            }
            None
        }
        EnterOutcome::ConfirmNextTest
        | EnterOutcome::ConfirmFinish
        | EnterOutcome::NextTest
        | EnterOutcome::Finish => Some(Action::Proceed(state.field_decisions)),
    }
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
    let is_last = ctx.index == ctx.total;
    let mut state = TuiState::new(test_result, initial_decisions.unwrap_or_default());
    let stdin = ctx.run_result.test_case.stdin.as_deref();

    let content =
        diff_content::build_content(test_result, state.active_field, stdin, state.active_tab);
    io.render(ctx, test_result, &state, &content)?;

    loop {
        let Some(key) = io.next_key()? else {
            return Ok(Action::Quit);
        };
        match apply_key(&mut state, key, is_last, ctx.watch_mode) {
            KeyResult::Continue => {}
            KeyResult::TryProceed => {
                if let Some(action) = try_proceed(&mut state, is_last) {
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
