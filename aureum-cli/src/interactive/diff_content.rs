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

/// True when the given field has an expected value configured in the test.
pub(super) fn is_field_configured(test_result: &TestResult, field: Field) -> bool {
    match field {
        Field::Stdout => !matches!(test_result.stdout, ValueComparison::NotChecked(_)),
        Field::Stderr => !matches!(test_result.stderr, ValueComparison::NotChecked(_)),
        Field::ExitCode => !matches!(test_result.exit_code, ValueComparison::NotChecked(_)),
        Field::Stdin => false,
    }
}

/// Dispatches to the Expected/Got or Diff content builder based on `active_tab`.
/// When the field is unconfigured, always uses the Got view regardless of `active_tab`.
pub(super) fn build_content(
    test_result: &TestResult,
    active_field: Field,
    stdin: Option<&str>,
    active_tab: Tab,
) -> Vec<Line<'static>> {
    let effective_tab = if !is_field_configured(test_result, active_field) {
        Tab::Got
    } else {
        active_tab
    };
    match effective_tab {
        Tab::Expected => {
            build_expected_or_got_content(test_result, active_field, stdin, Side::Expected)
        }
        Tab::Got => build_expected_or_got_content(test_result, active_field, stdin, Side::Got),
        Tab::Diff => build_diff_content(test_result, active_field),
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
                ValueComparison::NotChecked(_) => {
                    unreachable!("Expected view is only shown for configured fields")
                }
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
            ValueComparison::NotChecked(_) => {
                unreachable!("Expected view is only shown for configured fields")
            }
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
        ValueComparison::Matches(_) => vec![Line::from(vec![
            Span::raw("  "),
            theme::success_span(),
            Span::raw(" No difference"),
        ])],
        ValueComparison::NotChecked(_) => {
            unreachable!("Diff view is only shown for configured fields")
        }
    }
}

fn build_diff_content(test_result: &TestResult, active_field: Field) -> Vec<Line<'static>> {
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
            ValueComparison::Matches(_) => vec![Line::from(vec![
                Span::raw("  "),
                theme::success_span(),
                Span::raw(" No difference"),
            ])],
            ValueComparison::NotChecked(_) => {
                unreachable!("Diff view is only shown for configured fields")
            }
        },
        Field::Stdin => unreachable!("Diff view is only shown for configured fields"),
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
                let trimmed_len = line.trim_end().len();
                let red = Style::default().fg(Color::Red);
                let mut spans = vec![
                    Span::styled(
                        format!(
                            " {:>width$} {blank} │ ",
                            left_num.expect("Removed line always has left_num")
                        ),
                        theme::dim(),
                    ),
                    Span::styled(format!("-{}", &line[..trimmed_len]), red),
                ];
                if trimmed_len < line.len() {
                    spans.push(Span::styled(
                        line[trimmed_len..].to_owned(),
                        Style::default().bg(Color::Red),
                    ));
                }
                result.push(Line::from(spans));
            }
            Unchanged => {
                let prefix = if line.is_empty() {
                    format!(
                        " {:>width$} {:>width$} │",
                        left_num.expect("Unchanged line always has left_num"),
                        right_num.expect("Unchanged line always has right_num")
                    )
                } else {
                    format!(
                        " {:>width$} {:>width$} │  ",
                        left_num.expect("Unchanged line always has left_num"),
                        right_num.expect("Unchanged line always has right_num")
                    )
                };
                if line.is_empty() {
                    result.push(Line::from(Span::styled(prefix, theme::dim())));
                } else {
                    let mut spans = vec![Span::styled(prefix, theme::dim())];
                    spans.extend(theme::highlight_trailing_whitespace(line));
                    result.push(Line::from(spans));
                }
            }
            Added => {
                let trimmed_len = line.trim_end().len();
                let green = Style::default().fg(Color::Green);
                let mut spans = vec![
                    Span::styled(
                        format!(
                            " {blank} {:>width$} │ ",
                            right_num.expect("Added line always has right_num")
                        ),
                        theme::dim(),
                    ),
                    Span::styled(format!("+{}", &line[..trimmed_len]), green),
                ];
                if trimmed_len < line.len() {
                    spans.push(Span::styled(
                        line[trimmed_len..].to_owned(),
                        Style::default().bg(Color::Red),
                    ));
                }
                result.push(Line::from(spans));
            }
        }
    });

    result
}

/// Expected view: line number on the LEFT column, blank right column.
fn styled_lines_left(content: &str, col_width: usize) -> Vec<Line<'static>> {
    let blank = " ".repeat(col_width);
    numbered_lines(content)
        .map(|(num, line)| {
            if line.is_empty() {
                Line::from(Span::styled(
                    format!(" {num:>col_width$} {blank} │"),
                    theme::dim(),
                ))
            } else {
                let mut spans = vec![Span::styled(
                    format!(" {num:>col_width$} {blank} │  "),
                    theme::dim(),
                )];
                spans.extend(theme::highlight_trailing_whitespace(line));
                Line::from(spans)
            }
        })
        .collect()
}

/// Got view: blank left column, line number on the RIGHT column.
fn styled_lines_right(content: &str, col_width: usize) -> Vec<Line<'static>> {
    let blank = " ".repeat(col_width);
    numbered_lines(content)
        .map(|(num, line)| {
            if line.is_empty() {
                Line::from(Span::styled(
                    format!(" {blank} {num:>col_width$} │"),
                    theme::dim(),
                ))
            } else {
                let mut spans = vec![Span::styled(
                    format!(" {blank} {num:>col_width$} │  "),
                    theme::dim(),
                )];
                spans.extend(theme::highlight_trailing_whitespace(line));
                Line::from(spans)
            }
        })
        .collect()
}

fn not_configured_line() -> Vec<Line<'static>> {
    vec![Line::from(vec![
        Span::raw("  "),
        theme::not_configured_span(),
        Span::raw(" Not configured"),
    ])]
}

/// Returns an iterator of `(1-based line number, line text)` for every displayed
/// line in `content`, including a trailing empty line when `content` ends with `\n`.
fn numbered_lines(content: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut v: Vec<(usize, &str)> = Vec::new();
    if content.is_empty() {
        v.push((1, ""));
    } else {
        let mut count = 0;
        for (i, line) in content.lines().enumerate() {
            v.push((i + 1, line));
            count = i + 1;
        }
        if content.ends_with('\n') {
            v.push((count + 1, ""));
        }
    }
    v.into_iter()
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::widgets::Paragraph;

    // Helper: collect (content, bg) pairs from a Line's spans.
    fn span_bgs<'a>(line: &'a Line<'static>) -> Vec<(&'a str, Option<Color>)> {
        line.spans
            .iter()
            .map(|s| (s.content.as_ref(), s.style.bg))
            .collect()
    }

    mod expected_got_views {
        use super::*;

        #[test]
        fn no_trailing_whitespace_produces_single_content_span() {
            let lines = styled_lines_left("hello", 1);
            let bgs = span_bgs(&lines[0]);
            assert!(bgs.iter().all(|(_, bg)| *bg != Some(Color::Red)));
        }

        #[test]
        fn trailing_whitespace_gets_red_bg() {
            let lines = styled_lines_left("hello   ", 1);
            let bgs = span_bgs(&lines[0]);
            let last = bgs.last().unwrap();
            assert_eq!(last.0, "   ");
            assert_eq!(last.1, Some(Color::Red));
        }

        #[test]
        fn got_view_trailing_whitespace_gets_red_bg() {
            let lines = styled_lines_right("hi  ", 1);
            let bgs = span_bgs(&lines[0]);
            let last = bgs.last().unwrap();
            assert_eq!(last.0, "  ");
            assert_eq!(last.1, Some(Color::Red));
        }
    }

    mod diff_view {
        use super::*;

        #[test]
        fn unchanged_trailing_whitespace_gets_red_bg() {
            let lines = diff_lines_colored("same   ", "same   ");
            let bgs = span_bgs(&lines[0]);
            let last = bgs.last().unwrap();
            assert_eq!(last.0, "   ");
            assert_eq!(last.1, Some(Color::Red));
        }

        #[test]
        fn removed_trailing_whitespace_gets_red_bg_not_red_fg() {
            let lines = diff_lines_colored("old   ", "new");
            let removed = &lines[0];
            let bgs = span_bgs(removed);
            let last = bgs.last().unwrap();
            assert_eq!(last.0, "   ");
            assert_eq!(last.1, Some(Color::Red));
            // The fg of trailing ws span should NOT be red (red fg is only for the content)
            assert_ne!(removed.spans.last().unwrap().style.fg, Some(Color::Red));
        }

        #[test]
        fn added_trailing_whitespace_gets_red_bg() {
            let lines = diff_lines_colored("old", "new  ");
            let added = lines
                .iter()
                .find(|l| l.spans.iter().any(|s| s.content.starts_with('+')))
                .unwrap();
            let bgs = span_bgs(added);
            let last = bgs.last().unwrap();
            assert_eq!(last.0, "  ");
            assert_eq!(last.1, Some(Color::Red));
        }
    }

    // TestBackend test: verify rendered cells carry the red background at the
    // exact terminal columns where trailing whitespace appears.
    #[test]
    fn rendered_trailing_whitespace_cells_have_red_bg() {
        // "hello   " with col_width=1 produces prefix " 1   │  " (8 cols)
        // then "hello" (cols 8-12) + "   " (cols 13-15) = total 16 cols
        let lines = styled_lines_left("hello   ", 1);
        let backend = TestBackend::new(16, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    Paragraph::new(ratatui::text::Text::from(lines)),
                    frame.area(),
                );
            })
            .unwrap();
        let buf = terminal.backend().buffer();

        // Content columns must not be red
        for col in 8u16..=12 {
            assert_ne!(
                buf[(col, 0)].bg,
                Color::Red,
                "col {col} (content) should not have red bg"
            );
        }
        // Trailing whitespace columns must be red
        for col in 13u16..=15 {
            assert_eq!(
                buf[(col, 0)].bg,
                Color::Red,
                "col {col} (trailing ws) should have red bg"
            );
        }
    }
}
