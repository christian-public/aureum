use crate::utils::string;
use diff::Result;

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

#[cfg_attr(debug_assertions, derive(Debug))]
pub enum DiffLineType {
    Removed,
    Unchanged,
    Added,
}

/// Iterates diff lines between `expected` and `got`, calling `f` for each one.
/// Manages left/right line counters so callers don't have to.
pub fn for_each_diff_line(
    expected: &str,
    got: &str,
    mut f: impl FnMut(DiffLineType, Option<usize>, Option<usize>, &str),
) {
    let mut left_num = 1usize;
    let mut right_num = 1usize;

    for d in diff::lines(expected, got) {
        match d {
            Result::Left(line) => {
                f(DiffLineType::Removed, Some(left_num), None, line);
                left_num += 1;
            }
            Result::Both(line, _) => {
                f(
                    DiffLineType::Unchanged,
                    Some(left_num),
                    Some(right_num),
                    line,
                );
                left_num += 1;
                right_num += 1;
            }
            Result::Right(line) => {
                f(DiffLineType::Added, None, Some(right_num), line);
                right_num += 1;
            }
        }
    }
}

/// Returns the width of the line-number column needed to display diffs between
/// `expected` and `got`.
pub fn diff_column_width(expected: &str, got: &str) -> usize {
    string::displayed_line_count(expected)
        .max(string::displayed_line_count(got))
        .to_string()
        .len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    fn format_line(width: usize) -> impl FnMut(Option<usize>, Option<usize>, &str) -> String {
        let blank = " ".repeat(width);
        move |left_num, right_num, line| {
            let left_str = left_num.map_or(blank.clone(), |num| format!("{num:>width$}"));
            let right_str = right_num.map_or(blank.clone(), |num| format!("{num:>width$}"));
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
