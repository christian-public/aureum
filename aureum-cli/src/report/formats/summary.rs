use crate::report::theme;
use crate::vendor::ascii_tree::Tree::{self, Leaf, Node};
use aureum::string;
use aureum::{TestResult, ValueComparison};
use colored::Colorize;

// ERROR FORMATTING

pub fn nodes_from_test_result(test_result: &TestResult) -> Vec<Tree> {
    let mut categories = vec![];

    if let ValueComparison::Diff { expected, got } = &test_result.stdout {
        categories.push(Node(
            String::from("Standard output"),
            format_string_diff(expected, got),
        ));
    }

    if let ValueComparison::Diff { expected, got } = &test_result.stderr {
        categories.push(Node(
            String::from("Standard error"),
            format_string_diff(expected, got),
        ));
    }

    if let ValueComparison::Diff { expected, got } = test_result.exit_code {
        categories.push(Node(
            String::from("Exit code"),
            format_i32_diff(expected, got),
        ));
    }

    categories
}

/// Highlights trailing whitespace with a red ANSI background.
/// The interactive (ratatui) equivalent is `interactive::theme::highlight_trailing_whitespace`.
fn highlight_trailing_whitespace(line: &str) -> String {
    let trimmed_len = line.trim_end().len();
    if trimmed_len == line.len() {
        return line.to_owned();
    }
    format!("{}{}", &line[..trimmed_len], line[trimmed_len..].on_red())
}

fn format_string_diff(expected: &str, got: &str) -> Vec<Tree> {
    let width = string::displayed_line_count(expected)
        .max(string::displayed_line_count(got))
        .to_string()
        .len();
    let blank = " ".repeat(width);
    let separator = "│".dimmed();

    let format_expected_line = |num: usize, line: &str| -> String {
        let num_str = format!("{num:>width$}").dimmed();
        if line.is_empty() {
            format!("{num_str} {blank} {separator}")
        } else {
            format!(
                "{num_str} {blank} {separator}  {}",
                highlight_trailing_whitespace(line)
            )
        }
    };

    let format_got_line = |num: usize, line: &str| -> String {
        let num_str = format!("{num:>width$}").dimmed();
        if line.is_empty() {
            format!("{blank} {num_str} {separator}")
        } else {
            format!(
                "{blank} {num_str} {separator}  {}",
                highlight_trailing_whitespace(line)
            )
        }
    };

    let format_diff_line = |left_num: Option<usize>, right_num: Option<usize>, line: &str| {
        let left_num_str = left_num
            .map_or(blank.clone(), |num| format!("{num:>width$}"))
            .dimmed();
        let right_num_str = right_num
            .map_or(blank.clone(), |num| format!("{num:>width$}"))
            .dimmed();
        match (left_num, right_num) {
            (Some(_), None) => {
                let t = line.trim_end().len();
                let text = format!("-{}", &line[..t]).red();
                let trailing = line[t..].on_red();
                format!("{left_num_str} {blank} {separator} {text}{trailing}")
            }
            (None, Some(_)) => {
                let t = line.trim_end().len();
                let text = format!("+{}", &line[..t]).green();
                let trailing = line[t..].on_red();
                format!("{blank} {right_num_str} {separator} {text}{trailing}")
            }
            _ => {
                if line.is_empty() {
                    format!("{left_num_str} {right_num_str} {separator}")
                } else {
                    format!(
                        "{left_num_str} {right_num_str} {separator}  {}",
                        highlight_trailing_whitespace(line)
                    )
                }
            }
        }
    };

    let expected_output = theme::dimmed_border_text_block(&string::prefix_text_with_line_numbers(
        expected,
        format_expected_line,
    ));
    let expected_lines = string_to_lines(&format!("Expected\n{expected_output}"));

    let got_output = theme::dimmed_border_text_block(&string::prefix_text_with_line_numbers(
        got,
        format_got_line,
    ));
    let got_lines = string_to_lines(&format!("Got\n{got_output}"));

    let diff_output = theme::dimmed_border_text_block(&string::prefix_diff_with_line_numbers(
        expected,
        got,
        format_diff_line,
    ));
    let diff_lines = string_to_lines(&format!("Diff\n{diff_output}"));

    vec![Leaf(expected_lines), Leaf(got_lines), Leaf(diff_lines)]
}

fn string_to_lines(str: &str) -> Vec<String> {
    str.lines().map(|x| x.to_owned()).collect()
}

fn format_i32_diff(expected: i32, got: i32) -> Vec<Tree> {
    format_single_line_diff(expected.to_string(), got.to_string())
}

fn format_single_line_diff(expected: String, got: String) -> Vec<Tree> {
    vec![
        Node(String::from("Expected"), vec![Leaf(vec![expected])]),
        Node(String::from("Got"), vec![Leaf(vec![got])]),
    ]
}

#[cfg(test)]
mod tests {
    use super::highlight_trailing_whitespace;

    #[test]
    fn no_trailing_whitespace_unchanged() {
        assert_eq!(highlight_trailing_whitespace("hello"), "hello");
    }

    #[test]
    fn trailing_spaces_get_red_bg_ansi() {
        use colored::Colorize;
        colored::control::set_override(true);
        let result = highlight_trailing_whitespace("hello   ");
        let expected = format!("hello{}", "   ".on_red());
        colored::control::unset_override();
        assert_eq!(result, expected);
    }

    #[test]
    fn empty_line_unchanged() {
        assert_eq!(highlight_trailing_whitespace(""), "");
    }
}
