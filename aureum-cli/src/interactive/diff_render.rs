use aureum::{TestCase, TestResult, ValueComparison};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::interactive::diff_content;
use crate::interactive::diff_view::{self, DiffViewContext, Tab, TuiState};
use crate::interactive::field::{FailingFields, Field, FieldDecisions, OUTPUT_FIELDS};
use crate::interactive::style;
use crate::utils::shell;

// FIELD SELECTOR ROW LAYOUT
// Update label strings when field names change; widths and FIELD_SEP_COL are derived
// automatically, as are the decision-box positions in build_decisions_line.
const FIELD_ROW_PREFIX: usize = 2; // leading "  "
const FIELD_GAP: usize = 3; // "   " between fields
const FIELD_ROW_PADDING: usize = 2; // "  " before |

// Arrow ("▶ "/"  ") and indicator ("●"/"○"/"✓"/"✗") each occupy one display column.
// Label strings are all ASCII, so .len() equals the display column width.
const ARROW_WIDTH: usize = 2;
const INDICATOR_WIDTH: usize = 1;

// Each label is split into (before-key, key, after-key) so the shortcut character
// can be underlined in the UI. The same constants drive both rendering and width math.
const STDIN_PRE: &str = " Std";
const STDIN_KEY: &str = "i";
const STDIN_POST: &str = "n";
const STDOUT_PRE: &str = " Std";
const STDOUT_KEY: &str = "o";
const STDOUT_POST: &str = "ut";
const STDERR_PRE: &str = " Std";
const STDERR_KEY: &str = "e";
const STDERR_POST: &str = "rr";
const EXIT_CODE_PRE: &str = " E";
const EXIT_CODE_KEY: &str = "x";
const EXIT_CODE_POST: &str = "it code";

const FIELD_WIDTH_STDIN: usize =
    ARROW_WIDTH + INDICATOR_WIDTH + STDIN_PRE.len() + STDIN_KEY.len() + STDIN_POST.len();
const FIELD_WIDTH_STDOUT: usize =
    ARROW_WIDTH + INDICATOR_WIDTH + STDOUT_PRE.len() + STDOUT_KEY.len() + STDOUT_POST.len();
const FIELD_WIDTH_STDERR: usize =
    ARROW_WIDTH + INDICATOR_WIDTH + STDERR_PRE.len() + STDERR_KEY.len() + STDERR_POST.len();
const FIELD_WIDTH_EXIT_CODE: usize = ARROW_WIDTH
    + INDICATOR_WIDTH
    + EXIT_CODE_PRE.len()
    + EXIT_CODE_KEY.len()
    + EXIT_CODE_POST.len();

/// Column (inner_area-relative, 0-indexed) of the | separator between fields and status.
const FIELD_SEP_COL: usize = FIELD_ROW_PREFIX
    + FIELD_WIDTH_STDIN
    + FIELD_GAP
    + FIELD_WIDTH_STDOUT
    + FIELD_GAP
    + FIELD_WIDTH_STDERR
    + FIELD_GAP
    + FIELD_WIDTH_EXIT_CODE
    + FIELD_ROW_PADDING; // = 55

// DECISION BOX LAYOUT
// Each failing output field gets a 5-char "[   ]" box in the decisions row.
const DECISION_BOX_WIDTH: usize = 5;
const DECISION_BOX_INDENT: usize = 4; // arrow(2) + indicator(1) + leading space(1)

const DECISIONS_PREFIX: usize =
    FIELD_ROW_PREFIX + FIELD_WIDTH_STDIN + FIELD_GAP + DECISION_BOX_INDENT;
const DECISIONS_BOX_GAP: usize = FIELD_WIDTH_STDOUT + FIELD_GAP - DECISION_BOX_WIDTH;
const DECISIONS_TRAILING_GAP: usize =
    FIELD_ROW_PADDING + FIELD_WIDTH_EXIT_CODE - DECISION_BOX_INDENT - DECISION_BOX_WIDTH;

// ── Render entry point ───────────────────────────────────────────────────────

pub(super) fn render_tui(
    frame: &mut Frame,
    ctx: &DiffViewContext<'_>,
    test_result: &TestResult,
    state: &TuiState,
    content: &[Line<'static>],
) {
    let active_tab = state.active_tab;
    let active_field = state.active_field;
    let scroll = state.scroll;
    let field_decisions = &state.field_decisions;
    let show_enter_error = state.show_enter_error;
    let failing = FailingFields::of(test_result);
    let area = frame.area();

    // Outer layout: box + footer
    let outer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // entire box
            Constraint::Length(2), // footer
        ])
        .split(area);

    // Draw outer border and get inner area
    let outer_block = Block::default().borders(Borders::ALL);
    let inner_area = outer_block.inner(outer_chunks[0]);
    frame.render_widget(outer_block, outer_chunks[0]);

    // Helper: renders a full-width divider (├─...─┤) that overrides the │ border chars.
    let render_divider = |frame: &mut Frame, slot: Rect| {
        let w = inner_area.width as usize;
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

    // Inner layout: stats, divider, title, program, divider, content, divider, tabs, divider, fields, decisions
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // stats: index/total + passed/failed
            Constraint::Length(1), // divider
            Constraint::Length(1), // title: filename
            Constraint::Length(1), // program name + arguments
            Constraint::Length(1), // divider
            Constraint::Min(1),    // scrollable content
            Constraint::Length(1), // divider
            Constraint::Length(1), // [E]xpected / [G]ot / [D]iff tabs
            Constraint::Length(1), // divider
            Constraint::Length(1), // field selector: Stdin / Stdout / Stderr / Exit code
            Constraint::Length(1), // field decisions: [ ] [✓] [⊘]
        ])
        .split(inner_area);

    let w = inner_area.width as usize;

    // Stats row — index/total on the left, passed/failed on the right
    let stats_line = build_stats_line(ctx, w);
    frame.render_widget(Paragraph::new(stats_line), inner_chunks[0]);

    // Title row — test path
    let title_line = build_title_line(ctx);
    frame.render_widget(Paragraph::new(title_line), inner_chunks[2]);

    // Program row — program name on the left, Stdin tab on the right (if present)
    let program = build_program_display(&ctx.run_result.test_case);
    let program_line = build_program_line(&program);
    frame.render_widget(Paragraph::new(program_line), inner_chunks[3]);

    let field_sep_col = FIELD_SEP_COL;

    // Divider rows
    render_divider(frame, inner_chunks[1]);
    render_divider(frame, inner_chunks[4]);
    render_divider(frame, inner_chunks[6]);
    // Divider above field line: ┬ junction at the separator column
    frame.render_widget(
        Paragraph::new(format!(
            "├{}┬{}┤",
            "─".repeat(field_sep_col),
            "─".repeat(w.saturating_sub(field_sep_col + 1))
        )),
        Rect {
            x: outer_chunks[0].x,
            y: inner_chunks[8].y,
            width: outer_chunks[0].width,
            height: 1,
        },
    );
    // Bottom border: ┴ junction at the separator column
    let bottom_y = outer_chunks[0].y + outer_chunks[0].height - 1;
    frame.render_widget(
        Paragraph::new("┴"),
        Rect {
            x: outer_chunks[0].x + 1 + field_sep_col as u16,
            y: bottom_y,
            width: 1,
            height: 1,
        },
    );

    // Tabs row — hidden when Stdin is selected (tabs are not relevant for stdin content)
    if active_field != Field::Stdin {
        let tab_line = build_tab_line(active_tab, w);
        frame.render_widget(Paragraph::new(tab_line), inner_chunks[7]);
    }

    // Field selector row
    let field_line = build_field_line(
        active_field,
        test_result,
        ctx.run_result.test_case.stdin.is_some(),
    );
    frame.render_widget(Paragraph::new(field_line), inner_chunks[9]);

    // Apply pending decision for display: show the tentative y/n value in the current field's box.
    let mut display_decisions = *field_decisions;
    if let Some(pending) = state.pending_decision {
        display_decisions.set(active_field, Some(pending));
    }

    // Field decisions row
    let decisions_line = build_decisions_line(display_decisions, failing);
    frame.render_widget(Paragraph::new(decisions_line), inner_chunks[10]);

    // Status area: right of the │ separator, spanning both the field selector and decisions rows.
    let status_text = diff_view::compute_status(
        *field_decisions,
        state.active_field,
        state.pending_decision,
        show_enter_error,
        failing,
        ctx.index == ctx.total,
    );
    if !status_text.is_empty() {
        let status_x = inner_area.x + field_sep_col as u16 + 1 + 2; // │ + 2 space indent
        let status_w = inner_area.width.saturating_sub(field_sep_col as u16 + 3);
        frame.render_widget(
            Paragraph::new(status_text).wrap(Wrap { trim: true }),
            Rect {
                x: status_x,
                y: inner_chunks[9].y,
                width: status_w,
                height: 2,
            },
        );
    }

    // Scrollable content
    let content_height = inner_chunks[5].height as usize;
    let mut all_lines: Vec<Line<'static>> = content.to_vec();
    let sep = diff_content::sep_col_from_lines(content);
    if sep > 0 {
        let needed = (scroll as usize + content_height).saturating_sub(content.len());
        if needed > 0 {
            let pad = Line::from(vec![
                Span::raw(" ".repeat(sep)),
                Span::styled("│", style::dim()),
            ]);
            all_lines.extend(std::iter::repeat_n(pad, needed));
        }
    }

    let paragraph = Paragraph::new(Text::from(all_lines)).scroll((scroll, 0));
    frame.render_widget(paragraph, inner_chunks[5]);

    // Footer
    let enter = enter_label(
        active_field,
        state.pending_decision,
        *field_decisions,
        failing,
    );
    let footer = Paragraph::new(format!(
        "  ←→/ioex: switch field   1/2/3: switch view   ↑↓: scroll   a: accept   s: skip   Enter: {enter}\n  p: previous test   n: next test   l: list tests   q: quit"
    ));
    frame.render_widget(footer, outer_chunks[1]);
}

// ── Row builders ─────────────────────────────────────────────────────────────

/// Stats row: index/total on the left, pass/fail counts right-aligned.
fn build_stats_line(ctx: &DiffViewContext<'_>, width: usize) -> Line<'static> {
    let failed_count = ctx.total_count - ctx.passed_count;
    let left = format!("  Failed test {} of {}", ctx.index, ctx.total);
    let passed_str = format!("{} passed", ctx.passed_count);
    let failed_str = format!("{} failed", failed_count);
    let right_len = passed_str.len() + 2 + failed_str.len() + 2;
    let gap = width.saturating_sub(left.len() + right_len).max(1);

    Line::from(vec![
        Span::raw(left),
        Span::raw(" ".repeat(gap)),
        Span::styled(passed_str, Style::default().fg(Color::Green)),
        Span::raw("  "),
        Span::styled(failed_str, Style::default().fg(Color::Red)),
        Span::raw("  "),
    ])
}

/// Title row: test path.
fn build_title_line(ctx: &DiffViewContext<'_>) -> Line<'static> {
    let path = ctx.run_result.test_case.id().to_string();
    Line::from(vec![
        Span::raw("  "),
        Span::styled(path, Style::default().add_modifier(Modifier::BOLD)),
    ])
}

/// Builds the [1] Expected / [2] Got / [3] Diff tab row.
fn build_tab_line(active_tab: Tab, _width: usize) -> Line<'static> {
    let tabs = [
        ("Expected", Tab::Expected),
        ("Got", Tab::Got),
        ("Diff", Tab::Diff),
    ];

    let active_style = Style::default().add_modifier(Modifier::BOLD);
    let inactive_style = Style::default().fg(Color::DarkGray);

    let mut spans: Vec<Span<'static>> = vec![Span::raw("  ")];
    for (i, (name, tab)) in tabs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        let style = if *tab == active_tab {
            active_style
        } else {
            inactive_style
        };
        if *tab == active_tab {
            spans.push(Span::styled("▶ ", style));
        } else {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(format!("[{}] {name}", i + 1), style));
    }

    Line::from(spans)
}

/// Program row: `$ program args`.
fn build_program_line(program: &str) -> Line<'static> {
    let program_style = Style::default().fg(Color::DarkGray);
    Line::from(vec![
        Span::raw("  "),
        Span::styled("$ ".to_owned(), program_style),
        Span::styled(program.to_owned(), program_style),
    ])
}

/// Builds the decisions row with 5-wide boxes `[   ]` for failing fields only.
fn build_decisions_line(decisions: FieldDecisions, failing: FailingFields) -> Line<'static> {
    let gaps = [DECISIONS_BOX_GAP, DECISIONS_BOX_GAP, DECISIONS_TRAILING_GAP];

    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ".repeat(DECISIONS_PREFIX))];
    for (i, &field) in OUTPUT_FIELDS.iter().enumerate() {
        if failing.is_failing(field) {
            spans.push(Span::styled("[", style::dim()));
            let inner = match decisions.get(field) {
                None => "   ",
                Some(true) => " ✓ ",
                Some(false) => " ⊘ ",
            };
            spans.push(Span::raw(inner));
            spans.push(Span::styled("]", style::dim()));
        } else {
            spans.push(Span::raw(" ".repeat(DECISION_BOX_WIDTH)));
        }
        spans.push(Span::raw(" ".repeat(gaps[i])));
    }
    spans.push(Span::raw("│"));
    Line::from(spans)
}

/// Builds the field selector row with Stdin / Stdout / Stderr / Exit code,
/// with a status area to the right separated by a dimmed `│`.
fn build_field_line(
    active_field: Field,
    test_result: &TestResult,
    stdin_present: bool,
) -> Line<'static> {
    // (before_key, key_char, after_key, field, status)
    // status: None = not configured, Some(false) = passed, Some(true) = failed
    let output_fields: &[(&str, &str, &str, Field, Option<bool>)] = &[
        (
            STDOUT_PRE,
            STDOUT_KEY,
            STDOUT_POST,
            Field::Stdout,
            match &test_result.stdout {
                ValueComparison::Diff { .. } => Some(true),
                ValueComparison::Matches(_) => Some(false),
                ValueComparison::NotChecked(_) => None,
            },
        ),
        (
            STDERR_PRE,
            STDERR_KEY,
            STDERR_POST,
            Field::Stderr,
            match &test_result.stderr {
                ValueComparison::Diff { .. } => Some(true),
                ValueComparison::Matches(_) => Some(false),
                ValueComparison::NotChecked(_) => None,
            },
        ),
        (
            EXIT_CODE_PRE,
            EXIT_CODE_KEY,
            EXIT_CODE_POST,
            Field::ExitCode,
            match &test_result.exit_code {
                ValueComparison::Diff { .. } => Some(true),
                ValueComparison::Matches(_) => Some(false),
                ValueComparison::NotChecked(_) => None,
            },
        ),
    ];

    let active_style = Style::default().add_modifier(Modifier::BOLD);
    let inactive_style = Style::default().fg(Color::DarkGray);

    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ".repeat(FIELD_ROW_PREFIX))];

    // Stdin tab first
    {
        let is_active = active_field == Field::Stdin;
        let base_style = if is_active {
            active_style
        } else {
            inactive_style
        };
        if is_active {
            spans.push(Span::styled("▶ ", base_style));
        } else {
            spans.push(Span::raw("  "));
        }
        let stdin_indicator = if stdin_present {
            style::configured_span()
        } else {
            style::not_configured_span()
        };
        spans.push(if is_active {
            stdin_indicator
        } else {
            Span::styled(stdin_indicator.content, style::dim())
        });
        spans.push(Span::styled(STDIN_PRE, base_style));
        spans.push(Span::styled(
            STDIN_KEY,
            base_style.add_modifier(Modifier::UNDERLINED),
        ));
        spans.push(Span::styled(STDIN_POST, base_style));
    }

    for (before, key, after, field, status) in output_fields.iter() {
        spans.push(Span::raw(" ".repeat(FIELD_GAP)));
        let is_active = *field == active_field;
        let base_style = match (is_active, status) {
            (true, Some(true)) => active_style.fg(Color::Red),
            (true, _) => active_style,
            (false, _) => inactive_style,
        };

        if is_active {
            spans.push(Span::styled("▶ ", active_style));
        } else {
            spans.push(Span::raw("  "));
        }
        let indicator = match status {
            Some(true) => style::cross_span(),
            Some(false) => style::checkmark_span(),
            None => style::not_configured_span(),
        };
        spans.push(if is_active {
            indicator
        } else {
            Span::styled(indicator.content, style::dim())
        });
        spans.push(Span::styled(*before, base_style));
        spans.push(Span::styled(
            *key,
            base_style.add_modifier(Modifier::UNDERLINED),
        ));
        spans.push(Span::styled(*after, base_style));
    }

    spans.push(Span::raw(" ".repeat(FIELD_ROW_PADDING)));
    spans.push(Span::raw("│"));

    Line::from(spans)
}

fn enter_label(
    active_field: Field,
    pending_decision: Option<bool>,
    field_decisions: FieldDecisions,
    failing: FailingFields,
) -> &'static str {
    let needs_confirm = pending_decision.is_some()
        || (failing.is_failing(active_field) && field_decisions.get(active_field).is_none());
    if needs_confirm {
        "confirm"
    } else if diff_view::proceeds_to_next_test(
        active_field,
        pending_decision,
        field_decisions,
        failing,
    ) {
        "next test"
    } else {
        "next field"
    }
}

fn build_program_display(test_case: &TestCase) -> String {
    let path = &test_case.program_path;
    // On Windows, resolved paths carry a .exe suffix that is implicit at the command line.
    // Strip it so the displayed command is pasteable on all platforms.
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
