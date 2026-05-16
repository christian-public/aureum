use aureum::{TestCase, TestOutcome};
use crossterm::event::{KeyCode, KeyEvent};
use std::io;

use crate::counts::TestCounts;
use crate::interactive::action::Action;
use crate::interactive::field::{
    FailingFields, Field, FieldDecision, FieldDecisions, OUTPUT_FIELDS,
};
use crate::interactive::keys;
use crate::interactive::tty::Tty;
use crate::interactive::views::diff_content;
use crate::interactive::views::diff_render;

// ── Tab enum ─────────────────────────────────────────────────────────────────

/// The three content tabs.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(super) enum Tab {
    Expected,
    Got,
    Diff,
}

// ── Context ──────────────────────────────────────────────────────────────────

pub(crate) struct DiffViewContext<'a> {
    pub index: usize,
    pub total: usize,
    pub test_case: &'a TestCase,
    pub test_outcome: &'a TestOutcome,
    pub counts: TestCounts,
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
    /// Commits the staged decision, then advances.
    Confirm(Advance),
    /// Already decided; just advances.
    Advance(Advance),
}

/// Where Enter takes you after handling the current field's decision.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(super) enum Advance {
    NextField,
    NextTest,
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
    let advance = if next_failing_field_after(active_field, failing).is_some() {
        Advance::NextField
    } else if is_last {
        Advance::Finish
    } else {
        Advance::NextTest
    };
    if has_staged {
        EnterOutcome::Confirm(advance)
    } else {
        EnterOutcome::Advance(advance)
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
    fn new(test_outcome: &TestOutcome, initial_decisions: FieldDecisions) -> Self {
        let failing = FailingFields::of(test_outcome);
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
/// `is_field_configured` must reflect whether the current field has an expected value.
fn apply_key(
    state: &mut TuiState,
    key: KeyEvent,
    is_last: bool,
    watch_mode: bool,
    is_field_configured: bool,
) -> KeyResult {
    if keys::is_quit_key(&key) {
        return KeyResult::Exit(Action::Quit);
    }
    match key.code {
        KeyCode::Right => {
            if let Some(next) = state.active_field.next() {
                switch_field(state, next);
            }
        }
        KeyCode::Left => {
            if let Some(prev) = state.active_field.prev() {
                switch_field(state, prev);
            }
        }
        KeyCode::Up => {
            state.scroll = state.scroll.saturating_sub(1);
        }
        KeyCode::Down => {
            state.scroll = state.scroll.saturating_add(1);
        }
        KeyCode::Char('1') if state.active_field != Field::Stdin && is_field_configured => {
            state.active_tab = Tab::Expected;
            state.scroll = 0;
        }
        KeyCode::Char('2') if state.active_field != Field::Stdin => {
            state.active_tab = Tab::Got;
            state.scroll = 0;
        }
        KeyCode::Char('3') if state.active_field != Field::Stdin && is_field_configured => {
            state.active_tab = Tab::Diff;
            state.scroll = 0;
        }
        KeyCode::Char('i') => switch_field(state, Field::Stdin),
        KeyCode::Char('o') => switch_field(state, Field::Stdout),
        KeyCode::Char('e') => switch_field(state, Field::Stderr),
        KeyCode::Char('x') => switch_field(state, Field::ExitCode),
        KeyCode::Char('a')
            if state.active_field.is_output() && state.failing.is_failing(state.active_field) =>
        {
            let committed = state.field_decisions.get(state.active_field);
            state.staged_decision = if committed == FieldDecision::Accepted {
                FieldDecision::Undecided
            } else {
                FieldDecision::Accepted
            };
            state.show_enter_error = false;
        }
        KeyCode::Char('s')
            if state.active_field.is_output() && state.failing.is_failing(state.active_field) =>
        {
            let committed = state.field_decisions.get(state.active_field);
            state.staged_decision = if committed == FieldDecision::Skipped {
                FieldDecision::Undecided
            } else {
                FieldDecision::Skipped
            };
            state.show_enter_error = false;
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
        _ => {}
    }
    KeyResult::Continue
}

/// Switches the active field, resetting staged decision, enter-error, and scroll.
fn switch_field(state: &mut TuiState, field: Field) {
    state.staged_decision = FieldDecision::Undecided;
    state.show_enter_error = false;
    state.active_field = field;
    state.scroll = 0;
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
    let outcome = enter_outcome(
        active_field,
        staged_decision,
        field_decisions,
        failing,
        is_last,
    );
    let (confirm, advance) = match outcome {
        EnterOutcome::JumpToFirstFailing | EnterOutcome::NeedsDecision => return "",
        EnterOutcome::Confirm(a) => (true, a),
        EnterOutcome::Advance(a) => (false, a),
    };
    match (confirm, advance) {
        (true, Advance::NextField) => "Press Enter to confirm and go to the next field.",
        (true, Advance::NextTest) => "Press Enter to confirm and go to the next test.",
        (true, Advance::Finish) => "Press Enter to confirm and finish.",
        (false, Advance::NextField) => "Press Enter to go to the next field.",
        (false, Advance::NextTest) => "Press Enter to go to the next test.",
        (false, Advance::Finish) => "Press Enter to finish.",
    }
}

/// Commits any staged decision and advances to the next failing field or next test.
/// Returns `Some(Action::Proceed(...))` once the current test is done, or `None` (and
/// updates `state`) when navigation stays within the same test.
fn try_proceed(state: &mut TuiState, is_last: bool) -> Option<Action> {
    let outcome = enter_outcome(
        state.active_field,
        state.staged_decision,
        state.field_decisions,
        state.failing,
        is_last,
    );
    state.show_enter_error = matches!(outcome, EnterOutcome::NeedsDecision);

    let advance = match outcome {
        EnterOutcome::JumpToFirstFailing => {
            state.active_field = state.failing.first();
            state.scroll = 0;
            return None;
        }
        EnterOutcome::NeedsDecision => return None,
        EnterOutcome::Confirm(a) => {
            let staged = state.staged_decision;
            state.staged_decision = FieldDecision::Undecided;
            state.field_decisions.set(state.active_field, staged);
            a
        }
        EnterOutcome::Advance(a) => a,
    };

    match advance {
        Advance::NextField => {
            if let Some(next) = next_failing_field_after(state.active_field, state.failing) {
                state.active_field = next;
                state.scroll = 0;
            }
            None
        }
        Advance::NextTest | Advance::Finish => Some(Action::Proceed(state.field_decisions)),
    }
}

// ── Public entry point ───────────────────────────────────────────────────────

/// Renders the diff view for one failing test and runs its event loop until the user
/// presses a key that ends the view (Enter/n/p/l/Esc/q).
pub(crate) fn run_diff_view(
    tty: &mut dyn Tty,
    ctx: &DiffViewContext<'_>,
    initial_decisions: Option<FieldDecisions>,
) -> io::Result<Action> {
    let test_outcome = ctx.test_outcome;
    let is_last = ctx.index == ctx.total;
    let mut state = TuiState::new(test_outcome, initial_decisions.unwrap_or_default());
    let stdin = ctx.test_case.stdin.as_deref();

    let mut content =
        diff_content::build_content(test_outcome, state.active_field, stdin, state.active_tab);
    tty.draw(&mut |frame| diff_render::render_tui(frame, ctx, test_outcome, &state, &content))?;

    loop {
        let Some(key) = tty.next_key()? else {
            return Ok(Action::Quit);
        };
        let is_field_configured =
            diff_content::is_field_configured(test_outcome, state.active_field);
        match apply_key(
            &mut state,
            key,
            is_last,
            ctx.watch_mode,
            is_field_configured,
        ) {
            KeyResult::Continue => {}
            KeyResult::TryProceed => {
                if let Some(action) = try_proceed(&mut state, is_last) {
                    return Ok(action);
                }
            }
            KeyResult::Exit(action) => return Ok(action),
        }
        content =
            diff_content::build_content(test_outcome, state.active_field, stdin, state.active_tab);
        state.scroll = state.scroll.min(content.len().saturating_sub(1) as u16);
        tty.draw(&mut |frame| diff_render::render_tui(frame, ctx, test_outcome, &state, &content))?;
    }
}
