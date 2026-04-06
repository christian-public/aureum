use crate::test_result::{TestResult, ValueComparison};
use crate::utils::string;
use crate::utils::string::TextBlockOptions;
use crate::vendor::ascii_tree::Tree::{self, Leaf, Node};
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
            format!("{num_str} {blank} {separator}  {line}")
        }
    };

    let format_got_line = |num: usize, line: &str| -> String {
        let num_str = format!("{num:>width$}").dimmed();
        if line.is_empty() {
            format!("{blank} {num_str} {separator}")
        } else {
            format!("{blank} {num_str} {separator}  {line}")
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
                let text = format!("-{line}").red();
                format!("{left_num_str} {blank} {separator} {text}")
            }
            (None, Some(_)) => {
                let text = format!("+{line}").green();
                format!("{blank} {right_num_str} {separator} {text}")
            }
            _ => {
                if line.is_empty() {
                    format!("{left_num_str} {right_num_str} {separator}")
                } else {
                    format!("{left_num_str} {right_num_str} {separator}  {line}")
                }
            }
        }
    };

    let text_block_options = TextBlockOptions {
        top_line: TextBlockOptions::CORNER_TOP.dimmed().to_string(),
        bottom_line: TextBlockOptions::CORNER_BOTTOM.dimmed().to_string(),
        format_line: |line| {
            if line.is_empty() {
                TextBlockOptions::BORDER.dimmed().to_string()
            } else {
                format!("{} {line}", TextBlockOptions::BORDER.dimmed())
            }
        },
    };

    let expected_output = string::text_block_with_options(
        &string::prefix_text_with_line_numbers(expected, format_expected_line),
        &text_block_options,
    );
    let expected_lines = string_to_lines(&format!("Expected\n{expected_output}"));

    let got_output = string::text_block_with_options(
        &string::prefix_text_with_line_numbers(got, format_got_line),
        &text_block_options,
    );
    let got_lines = string_to_lines(&format!("Got\n{got_output}"));

    let diff_output = string::text_block_with_options(
        &string::prefix_diff_with_line_numbers(expected, got, format_diff_line),
        &text_block_options,
    );
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
