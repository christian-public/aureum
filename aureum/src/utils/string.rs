use diff::Result;

pub fn indent_with(prefix: &str, input: &str) -> String {
    decorate_lines(|line| format!("{}{}", prefix, line), input)
}

pub fn indent_by(indent_level: usize, input: &str) -> String {
    let prefix = " ".repeat(indent_level);
    indent_with(&prefix, input)
}

fn decorate_lines(decorate_line: impl Fn(&str) -> String, input: &str) -> String {
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

pub struct TextBlockOptions {
    pub top_line: String,
    pub bottom_line: String,
    pub format_line: fn(&str) -> String,
}

impl TextBlockOptions {
    pub const CORNER_TOP: &str = "╭";
    pub const BORDER: &str = "│";
    pub const CORNER_BOTTOM: &str = "╰";
}

impl Default for TextBlockOptions {
    fn default() -> Self {
        Self {
            top_line: TextBlockOptions::CORNER_TOP.to_owned(),
            bottom_line: TextBlockOptions::CORNER_BOTTOM.to_owned(),
            format_line: |line| {
                if line.is_empty() {
                    TextBlockOptions::BORDER.to_owned()
                } else {
                    format!("{} {line}", TextBlockOptions::BORDER)
                }
            },
        }
    }
}

#[allow(dead_code)]
pub fn text_block(content: &str) -> String {
    text_block_with_options(content, &TextBlockOptions::default())
}

pub fn text_block_with_options(content: &str, options: &TextBlockOptions) -> String {
    let prefixed = if content.is_empty() {
        (options.format_line)("")
    } else {
        let mut result = String::new();
        for (i, line) in content.lines().enumerate() {
            if i > 0 {
                result.push('\n');
            }
            result.push_str(&(options.format_line)(line));
        }
        if content.ends_with('\n') {
            result.push('\n');
        }
        result
    };

    if content.ends_with('\n') {
        format!("{}\n{prefixed}{}", options.top_line, options.bottom_line)
    } else {
        format!("{}\n{prefixed}\n{}", options.top_line, options.bottom_line)
    }
}

pub fn prefix_text_with_line_numbers(
    content: &str,
    mut format_line: impl FnMut(usize, &str) -> String,
) -> String {
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

        if content.ends_with('\n') {
            let line_count = content.lines().count() + 1;
            result.push('\n');
            result.push_str(&format_line(line_count, ""));
        }

        result
    }
}

pub fn prefix_diff_with_line_numbers(
    expected: &str,
    got: &str,
    mut format_line: impl FnMut(Option<usize>, Option<usize>, &str) -> String,
) -> String {
    let mut left_num: usize = 1;
    let mut right_num: usize = 1;
    let mut diff_output = String::new();

    for diff in diff::lines(expected, got) {
        match diff {
            Result::Left(left) => {
                diff_output.push_str(&format_line(Some(left_num), None, left));
                diff_output.push('\n');
                left_num += 1;
            }
            Result::Both(left, _) => {
                diff_output.push_str(&format_line(Some(left_num), Some(right_num), left));
                diff_output.push('\n');
                left_num += 1;
                right_num += 1;
            }
            Result::Right(right) => {
                diff_output.push_str(&format_line(None, Some(right_num), right));
                diff_output.push('\n');
                right_num += 1;
            }
        }
    }

    diff_output
}

pub fn displayed_line_count(content: &str) -> usize {
    if content.is_empty() {
        1
    } else {
        content.lines().count() + if content.ends_with('\n') { 1 } else { 0 }
    }
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

    mod text_block {
        use super::text_block;
        use indoc::indoc;

        #[test]
        fn test_empty() {
            let expected = indoc! {"
                ╭
                │
                ╰"};

            assert_eq!(text_block(""), expected);
        }

        #[test]
        fn test_only_newline() {
            let expected = indoc! {"
                ╭
                │
                ╰"};

            assert_eq!(text_block("\n"), expected);
        }

        #[test]
        fn test_single_line_no_newline() {
            let expected = indoc! {"
                ╭
                │ foo
                ╰"};

            assert_eq!(text_block("foo"), expected);
        }

        #[test]
        fn test_single_line_with_newline() {
            let expected = indoc! {"
                ╭
                │ foo
                ╰"};

            assert_eq!(text_block("foo\n"), expected);
        }

        #[test]
        fn test_multiple_lines_no_newline() {
            let expected = indoc! {"
                ╭
                │ line 1
                │ line 2
                ╰"};

            assert_eq!(text_block("line 1\nline 2"), expected);
        }

        #[test]
        fn test_multiple_lines_with_newline() {
            let expected = indoc! {"
                ╭
                │ line 1
                │ line 2
                ╰"};

            assert_eq!(text_block("line 1\nline 2\n"), expected);
        }

        #[test]
        fn test_multiple_lines_including_empty_lines() {
            let expected = indoc! {"
                ╭
                │ line 1
                │
                │ line 3
                ╰"};

            assert_eq!(text_block("line 1\n\nline 3\n"), expected);
        }
    }

    mod prefix_text_with_line_numbers {
        use super::super::*;
        use indoc::indoc;

        fn format_line(width: usize) -> impl Fn(usize, &str) -> String {
            move |line_number, line| {
                if line.is_empty() {
                    format!("{line_number:>width$} │")
                } else {
                    format!("{line_number:>width$} │ {line}")
                }
            }
        }

        #[test]
        fn test_empty() {
            let expected = "1 │";

            assert_eq!(prefix_text_with_line_numbers("", format_line(1)), expected);
        }

        #[test]
        fn test_only_newline() {
            let expected = indoc! {"
                1 │
                2 │"};

            assert_eq!(
                prefix_text_with_line_numbers("\n", format_line(1)),
                expected
            );
        }

        #[test]
        fn test_single_line_no_newline() {
            let expected = "1 │ foo";

            assert_eq!(
                prefix_text_with_line_numbers("foo", format_line(1)),
                expected
            );
        }

        #[test]
        fn test_single_line_with_newline() {
            let expected = indoc! {"
                1 │ foo
                2 │"};

            assert_eq!(
                prefix_text_with_line_numbers("foo\n", format_line(1)),
                expected
            );
        }

        #[test]
        fn test_multiple_lines_no_newline() {
            let expected = indoc! {"
                1 │ line 1
                2 │ line 2"};

            assert_eq!(
                prefix_text_with_line_numbers("line 1\nline 2", format_line(1)),
                expected
            );
        }

        #[test]
        fn test_multiple_lines_with_newline() {
            let expected = indoc! {"
                1 │ line 1
                2 │ line 2
                3 │"};

            assert_eq!(
                prefix_text_with_line_numbers("line 1\nline 2\n", format_line(1)),
                expected
            );
        }

        #[test]
        fn test_pads_line_numbers() {
            let expected = indoc! {"
                 1 │ line 1
                 2 │ line 2
                 3 │ line 3
                 4 │ line 4
                 5 │ line 5
                 6 │ line 6
                 7 │ line 7
                 8 │ line 8
                 9 │ line 9
                10 │"};

            let lines: Vec<String> = (1..=9).map(|i| format!("line {i}",)).collect();
            let content = lines.join("\n") + "\n";
            let width = displayed_line_count(&content).to_string().len();
            let result = prefix_text_with_line_numbers(&content, format_line(width));

            assert_eq!(result, expected);
        }

        #[test]
        fn test_format_line_receives_correct_values_regardless_of_format() {
            let mut calls: Vec<(usize, String)> = Vec::new();
            prefix_text_with_line_numbers("a\nb\n", |num, line| {
                calls.push((num, line.to_owned()));
                String::new()
            });
            assert_eq!(
                calls,
                vec![(1, "a".to_owned()), (2, "b".to_owned()), (3, "".to_owned())]
            );
        }
    }

    mod prefix_diff_with_line_numbers {
        use super::super::*;
        use indoc::indoc;

        fn format_line(width: usize) -> impl FnMut(Option<usize>, Option<usize>, &str) -> String {
            let blank = " ".repeat(width);
            move |left_num, right_num, line| {
                let left_str = left_num.map_or(blank.clone(), |n| format!("{:>width$}", n));
                let right_str = right_num.map_or(blank.clone(), |n| format!("{:>width$}", n));
                match (left_num, right_num) {
                    (Some(_), None) => format!("{left_str} {blank} │ -{line}"),
                    (None, Some(_)) => format!("{blank} {right_str} │ +{line}"),
                    _ => {
                        if line.is_empty() {
                            format!("{left_str} {right_str} │")
                        } else {
                            format!("{left_str} {right_str} │  {line}")
                        }
                    }
                }
            }
        }

        #[test]
        fn test_empty() {
            let expected = indoc! {""};

            assert_eq!(
                prefix_diff_with_line_numbers("", "", format_line(1)),
                expected
            );
        }

        #[test]
        fn test_single_line_no_diff() {
            let expected = indoc! {"
                1 1 │  line 1
                "};

            assert_eq!(
                prefix_diff_with_line_numbers("line 1", "line 1", format_line(1)),
                expected
            );
        }

        #[test]
        fn test_only_newline() {
            let expected = indoc! {"
                1 1 │
                2 2 │
                "};

            assert_eq!(
                prefix_diff_with_line_numbers("\n", "\n", format_line(1)),
                expected
            );
        }

        #[test]
        fn test_a_vs_b() {
            let expected = indoc! {"
                1   │ -a
                  1 │ +b
                "};

            assert_eq!(
                prefix_diff_with_line_numbers("a", "b", format_line(1)),
                expected
            );
        }

        #[test]
        fn test_pads_line_numbers() {
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

            let lines: Vec<String> = (1..=10).map(|i| format!("line {i}")).collect();
            let content = lines.join("\n");
            let result = prefix_diff_with_line_numbers(&content, &content, format_line(2));
            assert_eq!(result, expected);
        }

        #[test]
        fn test_pads_line_numbers_ends_with_newline() {
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

            let lines: Vec<String> = (1..=9).map(|i| format!("line {i}")).collect();
            let content = lines.join("\n") + "\n";
            let result = prefix_diff_with_line_numbers(&content, &content, format_line(2));
            assert_eq!(result, expected);
        }
    }
}
