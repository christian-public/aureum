use aureum::{TestResult, ValueComparison, string};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::interactive::diff_view::Tab;
use crate::interactive::field::Field;
use crate::interactive::theme;

pub(super) enum Side {
    Expected,
    Got,
}

/// Dispatches to the Expected/Got or Diff content builder based on `active_tab`.
pub(super) fn build_content(
    test_result: &TestResult,
    active_field: Field,
    stdin: Option<&str>,
    active_tab: Tab,
) -> Vec<Line<'static>> {
    match active_tab {
        Tab::Expected => {
            build_expected_or_got_content(test_result, active_field, stdin, Side::Expected)
        }
        Tab::Got => build_expected_or_got_content(test_result, active_field, stdin, Side::Got),
        Tab::Diff => build_diff_content(test_result, active_field, stdin),
    }
}

fn build_expected_or_got_content(
    test_result: &TestResult,
    active_field: Field,
    stdin: Option<&str>,
    side: Side,
) -> Vec<Line<'static>> {
    match active_field {
        Field::Stdout => build_text_view(side, &test_result.stdout),
        Field::Stderr => build_text_view(side, &test_result.stderr),
        Field::ExitCode => match side {
            Side::Expected => match &test_result.exit_code {
                ValueComparison::Diff { expected, .. } | ValueComparison::Matches(expected) => {
                    vec![Line::from(format!("  {expected}"))]
                }
                ValueComparison::NotChecked(_) => not_configured_line(),
            },
            Side::Got => vec![Line::from(format!("  {}", test_result.exit_code.got()))],
        },
        Field::Stdin => format_stdin_content(stdin),
    }
}

/// Renders a string `ValueComparison` as Expected (left-column line numbers) or Got
/// (right-column line numbers). Exit code and stdin are handled by the caller.
fn build_text_view(side: Side, comparison: &ValueComparison<String>) -> Vec<Line<'static>> {
    match side {
        Side::Expected => match comparison {
            ValueComparison::Diff { expected, got } => {
                styled_lines_left(expected, string::diff_column_width(expected, got))
            }
            ValueComparison::Matches(value) => {
                styled_lines_left(value, string::displayed_line_count(value).to_string().len())
            }
            ValueComparison::NotChecked(_) => not_configured_line(),
        },
        Side::Got => match comparison {
            ValueComparison::Diff { expected, got } => {
                styled_lines_right(got, string::diff_column_width(expected, got))
            }
            ValueComparison::Matches(got) | ValueComparison::NotChecked(got) => {
                styled_lines_right(got, string::displayed_line_count(got).to_string().len())
            }
        },
    }
}

fn build_text_diff(comparison: &ValueComparison<String>) -> Vec<Line<'static>> {
    match comparison {
        ValueComparison::Diff { expected, got } => diff_lines_colored(expected, got),
        ValueComparison::NotChecked(_) => not_configured_line(),
        ValueComparison::Matches(_) => vec![Line::from(vec![
            Span::raw("  "),
            theme::checkmark_span(),
            Span::raw(" No difference"),
        ])],
    }
}

fn build_diff_content(
    test_result: &TestResult,
    active_field: Field,
    stdin: Option<&str>,
) -> Vec<Line<'static>> {
    match active_field {
        Field::Stdout => build_text_diff(&test_result.stdout),
        Field::Stderr => build_text_diff(&test_result.stderr),
        Field::ExitCode => match &test_result.exit_code {
            ValueComparison::Diff { expected, got } => {
                vec![
                    Line::from(vec![
                        Span::styled("  Expected: ", theme::dim()),
                        Span::styled(expected.to_string(), Style::default().fg(Color::Red)),
                    ]),
                    Line::from(vec![
                        Span::styled("       Got: ", theme::dim()),
                        Span::styled(got.to_string(), Style::default().fg(Color::Green)),
                    ]),
                ]
            }
            ValueComparison::NotChecked(_) => not_configured_line(),
            ValueComparison::Matches(_) => vec![Line::from(vec![
                Span::raw("  "),
                theme::checkmark_span(),
                Span::raw(" No difference"),
            ])],
        },
        Field::Stdin => format_stdin_content(stdin),
    }
}

fn format_stdin_content(stdin: Option<&str>) -> Vec<Line<'static>> {
    match stdin {
        None => not_configured_line(),
        Some(content) => {
            let w = string::displayed_line_count(content).to_string().len();
            styled_lines_right(content, w)
        }
    }
}

fn diff_lines_colored(expected: &str, got: &str) -> Vec<Line<'static>> {
    let width = string::diff_column_width(expected, got);
    let blank = " ".repeat(width);
    let mut result: Vec<Line<'static>> = Vec::new();

    string::for_each_diff_line(expected, got, |kind, left_num, right_num, line| {
        use aureum::string::DiffLineType::*;
        match kind {
            Removed => {
                result.push(Line::from(vec![
                    Span::styled(
                        format!(" {:>width$} {blank} │ ", left_num.unwrap()),
                        theme::dim(),
                    ),
                    Span::styled(format!("-{line}"), Style::default().fg(Color::Red)),
                ]));
            }
            Unchanged => {
                let prefix = if line.is_empty() {
                    format!(
                        " {:>width$} {:>width$} │",
                        left_num.unwrap(),
                        right_num.unwrap()
                    )
                } else {
                    format!(
                        " {:>width$} {:>width$} │  ",
                        left_num.unwrap(),
                        right_num.unwrap()
                    )
                };
                if line.is_empty() {
                    result.push(Line::from(Span::styled(prefix, theme::dim())));
                } else {
                    result.push(Line::from(vec![
                        Span::styled(prefix, theme::dim()),
                        Span::raw(line.to_owned()),
                    ]));
                }
            }
            Added => {
                result.push(Line::from(vec![
                    Span::styled(
                        format!(" {blank} {:>width$} │ ", right_num.unwrap()),
                        theme::dim(),
                    ),
                    Span::styled(format!("+{line}"), Style::default().fg(Color::Green)),
                ]));
            }
        }
    });

    result
}

/// Expected view: line number on the LEFT column, blank right column.
fn styled_lines_left(content: &str, col_width: usize) -> Vec<Line<'static>> {
    let blank = " ".repeat(col_width);
    let mut lines = Vec::new();
    each_numbered_line(content, |num, line| {
        if line.is_empty() {
            lines.push(Line::from(Span::styled(
                format!(" {num:>col_width$} {blank} │"),
                theme::dim(),
            )));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!(" {num:>col_width$} {blank} │  "), theme::dim()),
                Span::raw(line.to_owned()),
            ]));
        }
    });
    lines
}

/// Got view: blank left column, line number on the RIGHT column.
fn styled_lines_right(content: &str, col_width: usize) -> Vec<Line<'static>> {
    let blank = " ".repeat(col_width);
    let mut lines = Vec::new();
    each_numbered_line(content, |num, line| {
        if line.is_empty() {
            lines.push(Line::from(Span::styled(
                format!(" {blank} {num:>col_width$} │"),
                theme::dim(),
            )));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!(" {blank} {num:>col_width$} │  "), theme::dim()),
                Span::raw(line.to_owned()),
            ]));
        }
    });
    lines
}

fn not_configured_line() -> Vec<Line<'static>> {
    vec![Line::from(vec![
        Span::raw("  "),
        theme::not_configured_span(),
        Span::raw(" Not configured"),
    ])]
}

/// Calls `f(line_number, line_text)` for each displayed line in `content`.
fn each_numbered_line(content: &str, mut f: impl FnMut(usize, &str)) {
    if content.is_empty() {
        f(1, "");
    } else {
        for (i, line) in content.lines().enumerate() {
            f(i + 1, line);
        }
        if content.ends_with('\n') {
            f(content.lines().count() + 1, "");
        }
    }
}

/// Returns the display column of the `│` separator in content lines.
/// All characters before `│` are ASCII so byte offset == display column.
pub(super) fn sep_col_from_lines(lines: &[Line<'static>]) -> usize {
    for line in lines {
        let mut col = 0usize;
        for span in &line.spans {
            if let Some(pos) = span.content.find('│') {
                return col + pos;
            }
            col += span.content.len();
        }
    }
    0
}
