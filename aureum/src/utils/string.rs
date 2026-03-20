use colored::Colorize;
use diff::Result;

pub fn indent_with(prefix: &str, input: &str) -> String {
    decorate_lines(|line| format!("{}{}", prefix, line), input)
}

pub fn indent_by(indent_level: usize, input: &str) -> String {
    let prefix = " ".repeat(indent_level);
    indent_with(&prefix, input)
}

fn decorate_lines<F>(decorate_line: F, input: &str) -> String
where
    F: Fn(&str) -> String,
{
    if input.is_empty() {
        return decorate_line("");
    }

    let mut output = String::new();

    for (i, line) in input.lines().enumerate() {
        if i > 0 {
            output.push('\n')
        }

        output.push_str(&decorate_line(line))
    }

    if input.ends_with('\n') {
        output.push('\n')
    }

    output
}

pub fn text_block(content: &str) -> String {
    let prefix_line = |line: &str| {
        if line.is_empty() {
            String::from("│")
        } else {
            format!("│ {}", line)
        }
    };
    let prefixed = if content.is_empty() {
        prefix_line("")
    } else {
        let mut result = String::new();
        for (i, line) in content.lines().enumerate() {
            if i > 0 {
                result.push('\n');
            }
            result.push_str(&prefix_line(line));
        }
        if content.ends_with('\n') {
            result.push('\n');
        }
        result
    };
    if content.ends_with('\n') {
        format!("╭\n{}╰", prefixed)
    } else {
        format!("╭\n{}\n╰", prefixed)
    }
}

pub fn prefix_with_line_numbers(content: &str) -> String {
    let ends_with_newline = content.ends_with('\n');
    let display_line_count = if content.is_empty() {
        1
    } else {
        content.lines().count() + if ends_with_newline { 1 } else { 0 }
    };
    let width = display_line_count.to_string().len();

    let format_line = |num: usize, line: &str| -> String {
        if line.is_empty() {
            format!("{:>width$} │", num, width = width)
        } else {
            format!("{:>width$} │ {}", num, line, width = width)
        }
    };

    if content.is_empty() {
        format_line(1, "")
    } else {
        let mut result = String::new();
        for (i, line) in content.lines().enumerate() {
            if i > 0 {
                result.push('\n');
            }
            result.push_str(&format_line(i + 1, line));
        }
        if ends_with_newline {
            result.push('\n');
            result.push_str(&format_line(display_line_count, ""));
        }
        result
    }
}

pub fn prefix_diff_with_line_numbers(expected: &str, got: &str, use_color: bool) -> String {
    let expected_line_count = if expected.is_empty() {
        1
    } else {
        expected.lines().count() + if expected.ends_with('\n') { 1 } else { 0 }
    };
    let got_line_count = if got.is_empty() {
        1
    } else {
        got.lines().count() + if got.ends_with('\n') { 1 } else { 0 }
    };
    let width = expected_line_count.max(got_line_count).to_string().len();
    let blank = " ".repeat(width);

    let mut left_num: usize = 1;
    let mut right_num: usize = 1;
    let mut diff_output = String::new();

    for diff in diff::lines(expected, got) {
        match diff {
            Result::Left(left) => {
                let text = format!("-{}", left);
                let colored_text = if use_color { text.red() } else { text.normal() };

                diff_output.push_str(&format!(
                    "{:>width$} {} │ {}\n",
                    left_num,
                    blank,
                    colored_text,
                    width = width
                ));
                left_num += 1;
            }
            Result::Both(left, _) => {
                let text = if left.is_empty() {
                    String::new()
                } else {
                    format!("  {}", left)
                };

                diff_output.push_str(&format!(
                    "{:>width$} {:>width$} │{}\n",
                    left_num,
                    right_num,
                    text,
                    width = width
                ));
                left_num += 1;
                right_num += 1;
            }
            Result::Right(right) => {
                let text = format!("+{}", right);
                let colored_text = if use_color {
                    text.green()
                } else {
                    text.normal()
                };

                diff_output.push_str(&format!(
                    "{} {:>width$} │ {}\n",
                    blank,
                    right_num,
                    colored_text,
                    width = width
                ));
                right_num += 1;
            }
        }
    }

    diff_output
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_indent_by() {
        let expected = indoc! {"
        - "};

        assert_eq!(indent_with("- ", ""), expected);
    }

    #[test]
    fn test_indent_with_only_newline() {
        let expected = indoc! {"
        - \n"};

        assert_eq!(indent_with("- ", "\n"), expected);
    }

    #[test]
    fn test_text_block_empty() {
        let expected = indoc! {"
            ╭
            │
            ╰"};

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
            ╰"};

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
            ╰"};

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

    #[test]
    fn test_prefix_with_line_numbers_empty() {
        let expected = "1 │";

        assert_eq!(prefix_with_line_numbers(""), expected);
    }

    #[test]
    fn test_prefix_with_line_numbers_only_newline() {
        let expected = indoc! {"
            1 │
            2 │"};

        assert_eq!(prefix_with_line_numbers("\n"), expected);
    }

    #[test]
    fn test_prefix_with_line_numbers_single_line_no_newline() {
        let expected = "1 │ foo";

        assert_eq!(prefix_with_line_numbers("foo"), expected);
    }

    #[test]
    fn test_prefix_with_line_numbers_single_line_with_newline() {
        let expected = indoc! {"
            1 │ foo
            2 │"};

        assert_eq!(prefix_with_line_numbers("foo\n"), expected);
    }

    #[test]
    fn test_prefix_with_line_numbers_multiple_lines_no_newline() {
        let expected = indoc! {"
            1 │ line 1
            2 │ line 2"};

        assert_eq!(prefix_with_line_numbers("line 1\nline 2"), expected);
    }

    #[test]
    fn test_prefix_with_line_numbers_multiple_lines_with_newline() {
        let expected = indoc! {"
            1 │ line 1
            2 │ line 2
            3 │"};

        assert_eq!(prefix_with_line_numbers("line 1\nline 2\n"), expected);
    }

    #[test]
    fn test_prefix_with_line_numbers_pads_line_numbers() {
        let lines: Vec<String> = (1..=10).map(|i| format!("line {}", i)).collect();
        let content = lines.join("\n") + "\n";
        let result = prefix_with_line_numbers(&content);
        assert!(result.contains(" 1 │ line 1"));
        assert!(result.contains("10 │ line 10"));
    }

    #[test]
    fn test_prefix_diff_with_line_numbers_empty() {
        let expected = indoc! {""};

        assert_eq!(prefix_diff_with_line_numbers("", "", false), expected);
    }

    #[test]
    fn test_prefix_diff_with_line_numbers_single_line_no_diff() {
        let expected = indoc! {"
            1 1 │  line 1
            "};

        assert_eq!(
            prefix_diff_with_line_numbers("line 1", "line 1", false),
            expected
        );
    }

    #[test]
    fn test_prefix_diff_with_line_numbers_only_newline() {
        let expected = indoc! {"
            1 1 │
            2 2 │
            "};

        assert_eq!(prefix_diff_with_line_numbers("\n", "\n", false), expected);
    }

    #[test]
    fn test_prefix_diff_with_line_numbers_a_vs_b() {
        let expected = indoc! {"
            1   │ -a
              1 │ +b
            "};

        assert_eq!(prefix_diff_with_line_numbers("a", "b", false), expected);
    }

    #[test]
    fn test_prefix_diff_with_line_numbers_pads_line_numbers() {
        let expected = indoc! {"
             1  1 │  line 1
             2  2 │  line 2
             3  3 │  line 3
             4  4 │  line 4
             5  5 │  line 5
             6  6 │  line 6
             7  7 │  line 7
             8  8 │  line 8
             9  9 │  line 9
            10 10 │  line 10
            "};

        let lines: Vec<String> = (1..=10).map(|i| format!("line {}", i)).collect();
        let content = lines.join("\n");
        let result = prefix_diff_with_line_numbers(&content, &content, false);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_prefix_diff_with_line_numbers_pads_line_numbers_ends_with_newline() {
        let expected = indoc! {"
             1  1 │  line 1
             2  2 │  line 2
             3  3 │  line 3
             4  4 │  line 4
             5  5 │  line 5
             6  6 │  line 6
             7  7 │  line 7
             8  8 │  line 8
             9  9 │  line 9
            10 10 │
            "};

        let lines: Vec<String> = (1..=9).map(|i| format!("line {}", i)).collect();
        let content = lines.join("\n") + "\n";
        let result = prefix_diff_with_line_numbers(&content, &content, false);
        assert_eq!(result, expected);
    }
}
