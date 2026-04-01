use crate::test_result::{TestResult, ValueComparison};
use crate::utils::string;
use crate::vendor::ascii_tree;
use crate::vendor::ascii_tree::Tree::{self, Leaf, Node};
use std::fmt::Error;

pub fn draw_tree(tree: &Tree) -> Result<String, Error> {
    let mut output = String::new();
    ascii_tree::write_tree(&mut output, tree)?;
    Ok(output)
}

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
    let expected_lines = string_to_lines(&format!(
        "Expected\n{}",
        string::text_block(&string::prefix_with_line_numbers(expected))
    ));
    let got_lines = string_to_lines(&format!(
        "Got\n{}",
        string::text_block(&string::prefix_with_line_numbers(got))
    ));

    let diff_output = string::prefix_diff_with_line_numbers(expected, got, true);

    let diff_lines = string_to_lines(&format!("Diff\n{}", string::text_block(&diff_output)));

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
