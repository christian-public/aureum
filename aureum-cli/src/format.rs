use std::fs;
use std::io;
use std::path::Path;
use toml_edit::DocumentMut;

const ROOT_FIELD_ORDER: &[&str] = &[
    "watch_files",
    "skip",
    "program",
    "program_arguments",
    "stdin",
    "expected_stdout",
    "expected_stderr",
    "expected_exit_code",
    "tests",
];

const TEST_FIELD_ORDER: &[&str] = &[
    "id",
    "skip",
    "program",
    "program_arguments",
    "stdin",
    "expected_stdout",
    "expected_stderr",
    "expected_exit_code",
];

/// Formats a file in-place. Returns `true` if the file was modified.
pub fn format_file(path: &Path) -> io::Result<bool> {
    let bytes = fs::read(path)?;
    let line_ending = LineEnding::detect(&bytes);
    let content = lf_content(&bytes)?;
    let formatted = format_content(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let output = line_ending.apply(&formatted);

    if output == bytes {
        return Ok(false);
    }

    fs::write(path, output)?;
    Ok(true)
}

/// Checks if a file needs formatting without writing. Returns `true` if the file would change.
pub fn check_file(path: &Path) -> io::Result<bool> {
    let bytes = fs::read(path)?;
    let line_ending = LineEnding::detect(&bytes);
    let content = lf_content(&bytes)?;
    let formatted = format_content(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let output = line_ending.apply(&formatted);
    Ok(output != bytes)
}

#[derive(Clone, Copy)]
enum LineEnding {
    Lf,
    CrLf,
    Cr,
}

impl LineEnding {
    fn detect(bytes: &[u8]) -> Self {
        let mut crlf = 0usize;
        let mut lf = 0usize;
        let mut cr = 0usize;
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\r' if i + 1 < bytes.len() && bytes[i + 1] == b'\n' => {
                    crlf += 1;
                    i += 2;
                    continue;
                }
                b'\r' => cr += 1,
                b'\n' => lf += 1,
                _ => {}
            }
            i += 1;
        }
        if crlf >= lf && crlf >= cr && crlf > 0 {
            Self::CrLf
        } else if cr > lf {
            Self::Cr
        } else {
            Self::Lf
        }
    }

    fn apply(self, content: &str) -> Vec<u8> {
        match self {
            Self::Lf => content.as_bytes().to_vec(),
            Self::CrLf => content.replace('\n', "\r\n").into_bytes(),
            Self::Cr => content.replace('\n', "\r").into_bytes(),
        }
    }
}

fn lf_content(bytes: &[u8]) -> io::Result<String> {
    let s = String::from_utf8(bytes.to_vec())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(s.replace("\r\n", "\n").replace('\r', "\n"))
}

pub fn format_content(content: &str) -> Result<String, toml_edit::TomlError> {
    let mut doc: DocumentMut = content.parse()?;

    reorder_table(doc.as_table_mut(), ROOT_FIELD_ORDER);

    if let Some(tests_item) = doc.get_mut("tests")
        && let Some(tests) = tests_item.as_array_of_tables_mut()
    {
        for table in tests.iter_mut() {
            reorder_table(table, TEST_FIELD_ORDER);
        }
    }

    let serialized = doc.to_string();
    let stripped = strip_blank_lines(&serialized);
    let spaced = insert_group_spacing(&stripped);
    Ok(normalize_document_boundaries(&spaced))
}

fn reorder_table(table: &mut toml_edit::Table, order: &[&str]) {
    table.sort_values_by(|a, _, b, _| {
        let pos_a = order
            .iter()
            .position(|&o| o == a.get())
            .unwrap_or(usize::MAX);
        let pos_b = order
            .iter()
            .position(|&o| o == b.get())
            .unwrap_or(usize::MAX);
        pos_a.cmp(&pos_b)
    });
}

/// Removes all blank lines from the document, leaving multiline string contents untouched.
fn strip_blank_lines(content: &str) -> String {
    let mut result: Vec<&str> = Vec::new();
    let mut in_multiline = false;

    for line in content.split('\n') {
        if in_multiline {
            result.push(line);
            if closes_multiline(line) {
                in_multiline = false;
            }
        } else if line.is_empty() {
            // skip
        } else {
            result.push(line);
            if opens_multiline(line) {
                in_multiline = true;
            }
        }
    }

    result.join("\n")
}

/// Inserts canonical blank lines between groups:
/// - one blank line after the `watch_files` block
/// - one blank line before the first `expected_*` field in each group
/// - two blank lines before each `[[...]]` section
///
/// In all cases the blank lines are placed above any comment lines that
/// immediately precede the group, so comments stay attached to their group.
fn insert_group_spacing(content: &str) -> String {
    let mut result: Vec<&str> = Vec::new();
    let mut in_watch_files = false;
    let mut bracket_depth: i32 = 0;
    let mut watch_files_just_ended = false;
    let mut in_multiline_string = false;
    let mut last_field_was_expected = false;

    for line in content.split('\n') {
        if in_multiline_string {
            if closes_multiline(line) {
                in_multiline_string = false;
            }
            result.push(line);
            continue;
        }

        // After the watch_files block, insert one blank line before the next content.
        // The [[tests]] and expected_* handlers below will top this up if needed.
        if watch_files_just_ended && !result.is_empty() {
            result.push("");
            watch_files_just_ended = false;
        }

        if line.starts_with("watch_files") {
            in_watch_files = true;
            bracket_depth = net_brackets(line);
            if bracket_depth <= 0 {
                in_watch_files = false;
                watch_files_just_ended = true;
            }
        } else if in_watch_files {
            bracket_depth += net_brackets(line);
            if bracket_depth <= 0 {
                in_watch_files = false;
                watch_files_just_ended = true;
            }
        }

        // Two blank lines before each [[...]] section (above any leading comments).
        if line.starts_with("[[") && !result.is_empty() {
            insert_blanks_before_comment_block(&mut result, 2);
        }

        // One blank line before the first expected_* field in a group (above any leading comments).
        if is_expected_field(line) && !last_field_was_expected && !result.is_empty() {
            insert_blanks_before_comment_block(&mut result, 1);
        }

        // One blank line before the first unrecognized field following the expected_* group.
        if is_field_start(line)
            && !is_expected_field(line)
            && last_field_was_expected
            && !result.is_empty()
        {
            insert_blanks_before_comment_block(&mut result, 1);
        }

        if line.starts_with("[[") {
            last_field_was_expected = false;
        } else if is_field_start(line) {
            last_field_was_expected = is_expected_field(line);
            if opens_multiline(line) {
                in_multiline_string = true;
            }
        }

        result.push(line);
    }

    result.join("\n")
}

/// Inserts `count` blank lines before the leading comment block at the end of `result`,
/// or before the last non-comment line if there are no trailing comments.
/// If blank lines already exist at the insertion point, only the missing ones are added.
fn insert_blanks_before_comment_block(result: &mut Vec<&str>, count: usize) {
    let mut insert_pos = result.len();
    while insert_pos > 0 && result[insert_pos - 1].starts_with('#') {
        insert_pos -= 1;
    }
    let existing = result[..insert_pos]
        .iter()
        .rev()
        .take_while(|&&l| l.is_empty())
        .count();
    for _ in 0..count.saturating_sub(existing) {
        result.insert(insert_pos, "");
    }
}

/// Strips leading blank lines and ensures the file ends with exactly one newline.
fn normalize_document_boundaries(content: &str) -> String {
    let trimmed = content.trim_start_matches('\n').trim_end_matches('\n');
    format!("{trimmed}\n")
}

fn net_brackets(line: &str) -> i32 {
    line.chars()
        .map(|c| match c {
            '[' => 1,
            ']' => -1,
            _ => 0,
        })
        .sum()
}

fn is_expected_field(line: &str) -> bool {
    line.starts_with("expected_")
}

fn is_field_start(line: &str) -> bool {
    !line.is_empty()
        && !line.starts_with('#')
        && !line.starts_with('[')
        && !line.starts_with(' ')
        && !line.starts_with('\t')
        && line.contains('=')
}

fn opens_multiline(line: &str) -> bool {
    let t = line.trim_end();
    t.ends_with("= \"\"\"") || t.ends_with("= '''")
}

fn closes_multiline(line: &str) -> bool {
    let t = line.trim_end();
    t == "\"\"\"" || t == "'''"
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn already_formatted_file_is_unchanged() {
        let input = indoc! {r#"
            program = "echo"
            program_arguments = ["-n", "hello"]

            expected_stdout = "hello"
        "#};
        assert_eq!(format_content(input).unwrap(), input);
    }

    #[test]
    fn reorders_root_fields_to_canonical_order() {
        let input = indoc! {r#"
            expected_stdout = "hello"
            program = "echo"
            program_arguments = ["-n", "hello"]
        "#};
        let expected = indoc! {r#"
            program = "echo"
            program_arguments = ["-n", "hello"]

            expected_stdout = "hello"
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn reorders_test_fields_and_puts_id_first() {
        let input = indoc! {r#"
            program = "echo"


            [[tests]]
            expected_stdout = "a"
            program_arguments = ["-n", "a"]
            id = "t1"
        "#};
        let expected = indoc! {r#"
            program = "echo"


            [[tests]]
            id = "t1"
            program_arguments = ["-n", "a"]

            expected_stdout = "a"
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn normalizes_blank_lines_between_tests_to_two() {
        let input = indoc! {r#"
            program = "echo"


            [[tests]]
            id = "t1"
            expected_stdout = "a"



            [[tests]]
            id = "t2"
            expected_stdout = "b"
        "#};
        let expected = indoc! {r#"
            program = "echo"


            [[tests]]
            id = "t1"

            expected_stdout = "a"


            [[tests]]
            id = "t2"

            expected_stdout = "b"
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn adds_two_blank_lines_before_tests_if_missing() {
        let input = indoc! {r#"
            program = "echo"
            [[tests]]
            id = "t1"
            expected_stdout = "a"
            [[tests]]
            id = "t2"
            expected_stdout = "b"
        "#};
        let expected = indoc! {r#"
            program = "echo"


            [[tests]]
            id = "t1"

            expected_stdout = "a"


            [[tests]]
            id = "t2"

            expected_stdout = "b"
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn comment_before_tests_moves_with_section() {
        let input = indoc! {r#"
            program = "echo"
            # first test
            [[tests]]
            id = "t1"
            expected_stdout = "a"
            # second test
            [[tests]]
            id = "t2"
            expected_stdout = "b"
        "#};
        let expected = indoc! {r#"
            program = "echo"


            # first test
            [[tests]]
            id = "t1"

            expected_stdout = "a"


            # second test
            [[tests]]
            id = "t2"

            expected_stdout = "b"
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn expected_fields_are_grouped_without_blank_lines_between_them() {
        let input = indoc! {r#"
            program = "echo"

            expected_stdout = "hello"

            expected_stderr = ""

            expected_exit_code = 0
        "#};
        let expected = indoc! {r#"
            program = "echo"

            expected_stdout = "hello"
            expected_stderr = ""
            expected_exit_code = 0
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn program_comes_before_program_arguments() {
        let input = indoc! {r#"
            program_arguments = ["-n", "hi"]
            program = "echo"
            expected_stdout = "hi"
        "#};
        let expected = indoc! {r#"
            program = "echo"
            program_arguments = ["-n", "hi"]

            expected_stdout = "hi"
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn watch_files_is_first_with_blank_line_after() {
        let input = indoc! {r#"
            program = "echo"
            watch_files = ["src/*.rs"]

            expected_stdout = "hello"
        "#};
        let expected = indoc! {r#"
            watch_files = ["src/*.rs"]

            program = "echo"

            expected_stdout = "hello"
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn skip_field_is_ordered_before_program() {
        let input = indoc! {r#"
            program = "echo"
            expected_stdout = "hello"
            skip = "not ready"
        "#};
        let expected = indoc! {r#"
            skip = "not ready"
            program = "echo"

            expected_stdout = "hello"
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn skip_field_in_subtest_is_ordered_after_id() {
        let input = indoc! {r#"
            program = "echo"


            [[tests]]
            expected_stdout = "hello"
            skip = "not ready"
            id = "t1"
        "#};
        let expected = indoc! {r#"
            program = "echo"


            [[tests]]
            id = "t1"
            skip = "not ready"

            expected_stdout = "hello"
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn unknown_fields_are_sorted_to_end_with_blank_line() {
        let input = indoc! {r#"
            custom_field = "x"
            program = "echo"
            expected_stdout = "hi"
        "#};
        let expected = indoc! {r#"
            program = "echo"

            expected_stdout = "hi"

            custom_field = "x"
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn multiple_unknown_fields_are_grouped_together() {
        let input = indoc! {r#"
            z_field = "z"
            a_field = "a"
            program = "echo"
            expected_stdout = "hi"
        "#};
        let expected = indoc! {r#"
            program = "echo"

            expected_stdout = "hi"

            z_field = "z"
            a_field = "a"
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn leading_blank_lines_are_removed() {
        let input = "\nprogram = \"echo\"\n\nexpected_stdout = \"hi\"\n";
        let expected = "program = \"echo\"\n\nexpected_stdout = \"hi\"\n";
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn trailing_newline_is_normalized_to_one() {
        let input = "program = \"echo\"\n\nexpected_stdout = \"hi\"\n\n\n";
        let expected = "program = \"echo\"\n\nexpected_stdout = \"hi\"\n";
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn multiline_expected_fields_grouped_without_extra_blank() {
        let input = indoc! {r#"
            program = "echo"

            expected_stdout = """
            hello
            """
            expected_stderr = ""
            expected_exit_code = 0
        "#};
        assert_eq!(format_content(input).unwrap(), input);
    }

    #[test]
    fn comment_before_expected_group_has_blank_line_above() {
        let input = indoc! {r#"
            program = "echo"
            # about output
            expected_stdout = "hello"
        "#};
        let expected = indoc! {r#"
            program = "echo"

            # about output
            expected_stdout = "hello"
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn blank_lines_between_comments_before_expected_group_are_removed() {
        let input = indoc! {r#"
            program = "echo"
            # comment 1

            # comment 2
            expected_stdout = "hello"
        "#};
        let expected = indoc! {r#"
            program = "echo"

            # comment 1
            # comment 2
            expected_stdout = "hello"
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn blank_lines_between_comment_and_next_field_are_removed() {
        let input = indoc! {r#"
            # comment 1
            program = "echo"
            # comment 2

            program_arguments = ["-n", "hello"]

            expected_stdout = "hello"
        "#};
        let expected = indoc! {r#"
            # comment 1
            program = "echo"
            # comment 2
            program_arguments = ["-n", "hello"]

            expected_stdout = "hello"
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn blank_lines_between_comment_lines_are_removed() {
        let input = indoc! {r#"
            # comment 1

            # comment 2
            program = "echo"

            expected_stdout = "hello"
        "#};
        let expected = indoc! {r#"
            # comment 1
            # comment 2
            program = "echo"

            expected_stdout = "hello"
        "#};
        assert_eq!(format_content(input).unwrap(), expected);
    }

    #[test]
    fn idempotent_when_applied_twice() {
        let input = indoc! {r#"
            expected_exit_code = 0
            expected_stderr = ""
            expected_stdout = "hello"
            program_arguments = ["-n", "hello"]
            program = "echo"


            [[tests]]
            expected_stdout = "a"
            id = "t1"


            [[tests]]
            expected_stdout = "b"
            id = "t2"
        "#};
        let once = format_content(input).unwrap();
        let twice = format_content(&once).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn line_ending_detect_lf() {
        assert!(matches!(LineEnding::detect(b"a\nb\n"), LineEnding::Lf));
    }

    #[test]
    fn line_ending_detect_crlf() {
        assert!(matches!(
            LineEnding::detect(b"a\r\nb\r\n"),
            LineEnding::CrLf
        ));
    }

    #[test]
    fn line_ending_detect_cr() {
        assert!(matches!(LineEnding::detect(b"a\rb\r"), LineEnding::Cr));
    }

    #[test]
    fn line_ending_detect_no_newlines_defaults_to_lf() {
        assert!(matches!(LineEnding::detect(b"abc"), LineEnding::Lf));
    }

    #[test]
    fn line_ending_detect_mixed_prefers_dominant() {
        // 2 CRLF, 1 LF → CRLF wins
        assert!(matches!(
            LineEnding::detect(b"a\r\nb\r\nc\n"),
            LineEnding::CrLf
        ));
        // 2 LF, 1 CRLF → LF wins
        assert!(matches!(LineEnding::detect(b"a\nb\nc\r\n"), LineEnding::Lf));
    }

    #[test]
    fn crlf_file_round_trips() {
        let crlf_input = "program = \"echo\"\r\n\r\nexpected_stdout = \"hi\"\r\n";
        let bytes = crlf_input.as_bytes();
        let line_ending = LineEnding::detect(bytes);
        let content = lf_content(bytes).unwrap();
        let formatted = format_content(&content).unwrap();
        let output = line_ending.apply(&formatted);
        assert_eq!(output, bytes);
    }

    #[test]
    fn crlf_file_is_reformatted_with_crlf_endings() {
        // Fields out of order — formatter should fix order and preserve CRLF
        let input = "expected_stdout = \"hi\"\r\nprogram = \"echo\"\r\n";
        let expected = "program = \"echo\"\r\n\r\nexpected_stdout = \"hi\"\r\n";
        let bytes = input.as_bytes();
        let line_ending = LineEnding::detect(bytes);
        let content = lf_content(bytes).unwrap();
        let formatted = format_content(&content).unwrap();
        let output = line_ending.apply(&formatted);
        assert_eq!(output, expected.as_bytes());
    }
}
