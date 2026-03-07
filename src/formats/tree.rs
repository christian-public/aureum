use crate::test_result::{TestResult, ValueComparison};
use crate::utils::string;
use crate::vendor::ascii_tree;
pub use crate::vendor::ascii_tree::Tree::{self, Leaf, Node};
use colored::Colorize;
use std::fmt::Error;

pub fn draw_tree(tree: &Tree) -> Result<String, Error> {
    let mut output = String::new();
    ascii_tree::write_tree(&mut output, tree)?;
    Ok(output)
}

pub fn text_block(content: &str) -> String {
    let prefixed_content = string::indent_with("│ ", content);

    if content.ends_with('\n') {
        format!("╭\n{}╰", prefixed_content)
    } else {
        format!("╭\n{}\n╰ (No newline at end)", prefixed_content)
    }
}

// ERROR FORMATTING

pub fn nodes_from_test_result(test_result: &TestResult) -> Vec<Tree> {
    let mut categories = vec![];

    if let ValueComparison::Diff { expected, got } = &test_result.stdout {
        categories.push(Node(
            String::from("Standard output"),
            show_string_diff(expected, got),
        ));
    }

    if let ValueComparison::Diff { expected, got } = &test_result.stderr {
        categories.push(Node(
            String::from("Standard error"),
            show_string_diff(expected, got),
        ));
    }

    if let ValueComparison::Diff { expected, got } = test_result.exit_code {
        categories.push(Node(
            String::from("Exit code"),
            show_i32_diff(expected, got),
        ));
    }

    categories
}

fn show_string_diff(expected: &str, got: &str) -> Vec<Tree> {
    let expected_lines = string_to_lines(&format!("Expected\n{}", text_block(expected)));
    let got_lines = string_to_lines(&format!("Got\n{}", text_block(got)));

    let mut diff_output = String::new();
    for diff in diff::lines(expected, got) {
        match diff {
            diff::Result::Left(l) => {
                diff_output.push_str(&format!("{}\n", (String::from("-") + l).red()))
            }
            diff::Result::Both(l, _) => diff_output.push_str(&format!(" {}\n", l)),
            diff::Result::Right(r) => {
                diff_output.push_str(&format!("{}\n", (String::from("+") + r).green()))
            }
        }
    }

    let diff_lines = string_to_lines(&format!("Diff\n{}", text_block(&diff_output)));

    vec![Leaf(expected_lines), Leaf(got_lines), Leaf(diff_lines)]
}

fn string_to_lines(str: &str) -> Vec<String> {
    str.lines().map(|x| x.to_owned()).collect()
}

fn show_i32_diff(expected: i32, got: i32) -> Vec<Tree> {
    show_single_line_diff(expected.to_string(), got.to_string())
}

fn show_single_line_diff(expected: String, got: String) -> Vec<Tree> {
    vec![
        Node(String::from("Expected"), vec![Leaf(vec![expected])]),
        Node(String::from("Got"), vec![Leaf(vec![got])]),
    ]
}

// TESTS

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_text_block_empty() {
        let expected = indoc! {"
            ╭
            │ 
            ╰ (No newline at end)"};

        assert_eq!(text_block(""), expected);
    }

    #[test]
    fn test_text_block_only_newline() {
        let expected = indoc! {"
            ╭
            │ 
            ╰"};

        assert_eq!(text_block("\n"), expected);
    }

    #[test]
    fn test_text_block_single_line_no_newline() {
        let expected = indoc! {"
            ╭
            │ foo
            ╰ (No newline at end)"};

        assert_eq!(text_block("foo"), expected);
    }

    #[test]
    fn test_text_block_single_line_with_newline() {
        let expected = indoc! {"
            ╭
            │ foo
            ╰"};

        assert_eq!(text_block("foo\n"), expected);
    }

    #[test]
    fn test_text_block_multiple_lines_no_newline() {
        let expected = indoc! {"
            ╭
            │ line 1
            │ line 2
            ╰ (No newline at end)"};

        assert_eq!(text_block("line 1\nline 2"), expected);
    }

    #[test]
    fn test_text_block_multiple_lines_with_newline() {
        let expected = indoc! {"
            ╭
            │ line 1
            │ line 2
            ╰"};

        assert_eq!(text_block("line 1\nline 2\n"), expected);
    }

    #[test]
    fn test_text_block_multiple_lines_including_empty_lines() {
        let expected = indoc! {"
            ╭
            │ line 1
            │ 
            │ line 3
            ╰"};

        assert_eq!(text_block("line 1\n\nline 3\n"), expected);
    }
}
