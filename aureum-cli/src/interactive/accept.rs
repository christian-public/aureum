use aureum::{TestCase, TestId, TestResult, ValueComparison};
use std::fs;
use std::io;
use std::path::Path;
use toml_edit::{DocumentMut, Item, Value};

use crate::interactive::field::{FieldDecision, FieldDecisions};

/// Updates test expectations on disk according to per-field decisions.
/// A field is updated only when the decision is `Some(true)` AND the field has a diff.
pub(crate) fn update_test_expectations(
    test_case: &TestCase,
    test_result: &TestResult,
    current_dir: &Path,
    decisions: &FieldDecisions,
) -> io::Result<()> {
    let config_path = test_case.path_to_config_file().to_path(current_dir);
    let containing_dir = test_case.path_to_containing_dir.to_path(current_dir);

    let content = fs::read_to_string(&config_path)?;
    let mut doc: DocumentMut = content
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{e}")))?;

    let mut doc_modified = false;

    if decisions.stdout == FieldDecision::Accepted
        && let ValueComparison::Diff { got, .. } = &test_result.stdout
        && apply_field_update(
            &mut doc,
            &test_case.test_id,
            "expected_stdout",
            &FieldValue::Str(got),
            &containing_dir,
        )?
    {
        doc_modified = true;
    }

    if decisions.stderr == FieldDecision::Accepted
        && let ValueComparison::Diff { got, .. } = &test_result.stderr
        && apply_field_update(
            &mut doc,
            &test_case.test_id,
            "expected_stderr",
            &FieldValue::Str(got),
            &containing_dir,
        )?
    {
        doc_modified = true;
    }

    if decisions.exit_code == FieldDecision::Accepted
        && let ValueComparison::Diff { got, .. } = &test_result.exit_code
        && apply_field_update(
            &mut doc,
            &test_case.test_id,
            "expected_exit_code",
            &FieldValue::Int(*got as i64),
            &containing_dir,
        )?
    {
        doc_modified = true;
    }

    if doc_modified {
        fs::write(&config_path, doc.to_string())?;
    }

    Ok(())
}

enum FieldValue<'a> {
    Str(&'a str),
    Int(i64),
}

impl FieldValue<'_> {
    fn to_toml_value(&self) -> Value {
        match self {
            FieldValue::Str(s) => Value::from(*s),
            FieldValue::Int(n) => Value::from(*n),
        }
    }

    fn to_string_content(&self) -> String {
        match self {
            FieldValue::Str(s) => s.to_string(),
            FieldValue::Int(n) => n.to_string(),
        }
    }
}

/// Updates a field in the document, either updating a file reference's external file
/// or updating the TOML value in-place. Returns `true` if the document itself was modified.
fn apply_field_update(
    doc: &mut DocumentMut,
    test_id: &TestId,
    field: &str,
    new_value: &FieldValue<'_>,
    containing_dir: &Path,
) -> io::Result<bool> {
    if test_id.is_root() {
        return apply_to_section(doc.as_table_mut(), field, new_value, containing_dir);
    }

    let test_name = test_id.to_string();

    // Check if the field is in the subtest section (takes precedence over root).
    let in_subtest = get_subtest_section(doc, &test_name)
        .map(|s| s.contains_key(field))
        .unwrap_or(false);

    if in_subtest {
        let subtest = get_subtest_section_mut(doc, &test_name).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("Subtest '{test_name}' not found"),
            )
        })?;
        return apply_to_section(subtest, field, new_value, containing_dir);
    }

    // Field is inherited from root or not present anywhere; update or insert at root.
    apply_to_section(doc.as_table_mut(), field, new_value, containing_dir)
}

/// Applies the update to a single TOML table section.
///
/// - If the current value is `{ file = "..." }`, writes `new_value` to that file and
///   returns `false` (no doc change).
/// - If the current value is `{ env = "..." }`, skips silently and returns `false`.
/// - Otherwise, replaces the value in-place and returns `true`.
fn apply_to_section(
    section: &mut toml_edit::Table,
    field: &str,
    new_value: &FieldValue<'_>,
    containing_dir: &Path,
) -> io::Result<bool> {
    // Check for special forms ({ file = "..." } or { env = "..." }).
    if let Some(item) = section.get(field)
        && let Some(table) = item.as_inline_table()
    {
        if let Some(file_val) = table.get("file")
            && let Some(file_path) = file_val.as_str()
        {
            let external_path = containing_dir.join(file_path);
            fs::write(external_path, new_value.to_string_content())?;
            return Ok(false);
        }

        if table.contains_key("env") {
            return Ok(false); // Can't update env vars
        }
    }

    // Update or insert a literal value.
    section[field] = Item::Value(new_value.to_toml_value());
    Ok(true)
}

fn get_subtest_section<'a>(doc: &'a DocumentMut, test_name: &str) -> Option<&'a toml_edit::Table> {
    doc.get("tests")?.as_table()?.get(test_name)?.as_table()
}

fn get_subtest_section_mut<'a>(
    doc: &'a mut DocumentMut,
    test_name: &str,
) -> Option<&'a mut toml_edit::Table> {
    doc.get_mut("tests")?
        .as_table_mut()?
        .get_mut(test_name)?
        .as_table_mut()
}

#[cfg(test)]
mod tests {
    use super::super::field::{FieldDecision, FieldDecisions};
    use super::super::test_helpers::{TempDir, make_test_case_root};
    use super::*;
    use aureum::TestId;
    use relative_path::RelativePathBuf;
    use std::path::PathBuf;

    const ACCEPT_ALL: FieldDecisions = FieldDecisions {
        stdout: FieldDecision::Accepted,
        stderr: FieldDecision::Accepted,
        exit_code: FieldDecision::Accepted,
    };

    fn make_test_case_subtest(dir: &str, file: &str, name: &str) -> TestCase {
        TestCase {
            path_to_containing_dir: RelativePathBuf::from(dir),
            file_name: file.to_string(),
            test_id: TestId::new(vec![name.to_string()]),
            description: None,
            program_path: PathBuf::from("/bin/echo"),
            arguments: vec![],
            stdin: None,
        }
    }

    // ── apply_to_section ──────────────────────────────────────────────────────

    #[test]
    fn test_apply_to_section_updates_literal_string() {
        let tmp = TempDir::new("section_str");
        let mut doc: DocumentMut = "expected_stdout = \"old\"\n".parse().unwrap();

        let changed = apply_to_section(
            doc.as_table_mut(),
            "expected_stdout",
            &FieldValue::Str("new"),
            tmp.path(),
        )
        .unwrap();

        assert!(changed);
        assert!(doc.to_string().contains("expected_stdout = \"new\""));
    }

    #[test]
    fn test_apply_to_section_updates_literal_integer() {
        let tmp = TempDir::new("section_int");
        let mut doc: DocumentMut = "expected_exit_code = 99\n".parse().unwrap();

        let changed = apply_to_section(
            doc.as_table_mut(),
            "expected_exit_code",
            &FieldValue::Int(0),
            tmp.path(),
        )
        .unwrap();

        assert!(changed);
        assert!(doc.to_string().contains("expected_exit_code = 0"));
    }

    #[test]
    fn test_apply_to_section_writes_file_reference_and_does_not_modify_doc() {
        let tmp = TempDir::new("file_ref");
        tmp.write("out.txt", "old content");
        let mut doc: DocumentMut = "expected_stdout = { file = \"out.txt\" }\n"
            .parse()
            .unwrap();
        let original_doc = doc.to_string();

        let changed = apply_to_section(
            doc.as_table_mut(),
            "expected_stdout",
            &FieldValue::Str("new content"),
            tmp.path(),
        )
        .unwrap();

        assert!(!changed);
        assert_eq!(doc.to_string(), original_doc); // doc unchanged
        assert_eq!(tmp.read("out.txt"), "new content");
    }

    #[test]
    fn test_apply_to_section_skips_env_reference() {
        let tmp = TempDir::new("env_ref");
        let mut doc: DocumentMut = "expected_stdout = { env = \"MY_VAR\" }\n".parse().unwrap();
        let original_doc = doc.to_string();

        let changed = apply_to_section(
            doc.as_table_mut(),
            "expected_stdout",
            &FieldValue::Str("new"),
            tmp.path(),
        )
        .unwrap();

        assert!(!changed);
        assert_eq!(doc.to_string(), original_doc);
    }

    // ── update_test_expectations ──────────────────────────────────────────────

    #[test]
    fn test_update_root_test_stdout_literal() {
        let tmp = TempDir::new("root_stdout");
        tmp.write(
            "test.toml",
            "program = \"echo\"\nexpected_stdout = \"WRONG\"\n",
        );

        let tc = make_test_case_root("", "test.toml");
        let result = TestResult {
            stdout: ValueComparison::Diff {
                expected: "WRONG".to_string(),
                got: "actual".to_string(),
            },
            stderr: ValueComparison::NotChecked("".to_string()),
            exit_code: ValueComparison::NotChecked(0),
        };

        update_test_expectations(&tc, &result, tmp.path(), &ACCEPT_ALL).unwrap();

        assert!(
            tmp.read("test.toml")
                .contains("expected_stdout = \"actual\"")
        );
    }

    #[test]
    fn test_update_root_test_all_fields() {
        let tmp = TempDir::new("root_all");
        tmp.write(
            "test.toml",
            "program = \"echo\"\nexpected_stdout = \"WRONG_OUT\"\nexpected_stderr = \"WRONG_ERR\"\nexpected_exit_code = 99\n",
        );

        let tc = make_test_case_root("", "test.toml");
        let result = TestResult {
            stdout: ValueComparison::Diff {
                expected: "WRONG_OUT".to_string(),
                got: "out".to_string(),
            },
            stderr: ValueComparison::Diff {
                expected: "WRONG_ERR".to_string(),
                got: "err".to_string(),
            },
            exit_code: ValueComparison::Diff {
                expected: 99,
                got: 0,
            },
        };

        update_test_expectations(&tc, &result, tmp.path(), &ACCEPT_ALL).unwrap();

        let updated = tmp.read("test.toml");
        assert!(updated.contains("expected_stdout = \"out\""));
        assert!(updated.contains("expected_stderr = \"err\""));
        assert!(updated.contains("expected_exit_code = 0"));
    }

    #[test]
    fn test_update_subtest_stdout_in_subtest_section() {
        let tmp = TempDir::new("subtest_in_sub");
        tmp.write(
            "test.toml",
            "program = \"echo\"\n\n[tests.t1]\nprogram_arguments = [\"-n\", \"x\"]\nexpected_stdout = \"WRONG\"\n",
        );

        let tc = make_test_case_subtest("", "test.toml", "t1");
        let result = TestResult {
            stdout: ValueComparison::Diff {
                expected: "WRONG".to_string(),
                got: "actual".to_string(),
            },
            stderr: ValueComparison::NotChecked("".to_string()),
            exit_code: ValueComparison::NotChecked(0),
        };

        update_test_expectations(&tc, &result, tmp.path(), &ACCEPT_ALL).unwrap();

        let updated = tmp.read("test.toml");
        assert!(updated.contains("expected_stdout = \"actual\""));
        assert!(updated.contains("program = \"echo\""));
    }

    #[test]
    fn test_update_subtest_field_inherited_from_root_updates_root() {
        let tmp = TempDir::new("subtest_inherited");
        tmp.write(
            "test.toml",
            "program = \"echo\"\nexpected_exit_code = 99\n\n[tests.t1]\nprogram_arguments = [\"-n\", \"x\"]\nexpected_stdout = \"x\"\n",
        );

        let tc = make_test_case_subtest("", "test.toml", "t1");
        let result = TestResult {
            stdout: ValueComparison::NotChecked("".to_string()),
            stderr: ValueComparison::NotChecked("".to_string()),
            exit_code: ValueComparison::Diff {
                expected: 99,
                got: 0,
            },
        };

        update_test_expectations(&tc, &result, tmp.path(), &ACCEPT_ALL).unwrap();

        let updated = tmp.read("test.toml");
        assert!(updated.contains("expected_exit_code = 0"));
    }

    #[test]
    fn test_update_file_reference_writes_external_file() {
        let tmp = TempDir::new("file_ref_update");
        tmp.write("expected_out.txt", "old content");
        tmp.write(
            "test.toml",
            "program = \"echo\"\nexpected_stdout = { file = \"expected_out.txt\" }\n",
        );

        let tc = make_test_case_root("", "test.toml");
        let result = TestResult {
            stdout: ValueComparison::Diff {
                expected: "old content".to_string(),
                got: "new content".to_string(),
            },
            stderr: ValueComparison::NotChecked("".to_string()),
            exit_code: ValueComparison::NotChecked(0),
        };

        update_test_expectations(&tc, &result, tmp.path(), &ACCEPT_ALL).unwrap();

        assert_eq!(tmp.read("expected_out.txt"), "new content");
        assert!(
            tmp.read("test.toml")
                .contains("expected_stdout = { file = \"expected_out.txt\" }")
        );
    }

    #[test]
    fn test_update_no_diffs_does_not_write_file() {
        let tmp = TempDir::new("no_diff");
        let original = "program = \"echo\"\nexpected_stdout = \"ok\"\n";
        tmp.write("test.toml", original);

        let tc = make_test_case_root("", "test.toml");
        let result = TestResult {
            stdout: ValueComparison::Matches("ok".to_string()),
            stderr: ValueComparison::NotChecked("".to_string()),
            exit_code: ValueComparison::NotChecked(0),
        };

        update_test_expectations(&tc, &result, tmp.path(), &ACCEPT_ALL).unwrap();

        assert_eq!(tmp.read("test.toml"), original);
    }

    #[test]
    fn test_update_preserves_other_fields() {
        let tmp = TempDir::new("preserves");
        tmp.write(
            "test.toml",
            "program = \"echo\"\nprogram_arguments = [\"-n\", \"Hello\"]\nexpected_stdout = \"WRONG\"\nexpected_exit_code = 0\n",
        );

        let tc = make_test_case_root("", "test.toml");
        let result = TestResult {
            stdout: ValueComparison::Diff {
                expected: "WRONG".to_string(),
                got: "Hello".to_string(),
            },
            stderr: ValueComparison::NotChecked("".to_string()),
            exit_code: ValueComparison::Matches(0),
        };

        update_test_expectations(&tc, &result, tmp.path(), &ACCEPT_ALL).unwrap();

        let updated = tmp.read("test.toml");
        assert!(updated.contains("program = \"echo\""));
        assert!(updated.contains("program_arguments"));
        assert!(updated.contains("expected_exit_code = 0"));
        assert!(updated.contains("expected_stdout = \"Hello\""));
    }
}
