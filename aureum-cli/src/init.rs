use crate::utils::toml;
use aureum::{ProgramOutput, string};
use std::collections::BTreeSet;
use std::io;
use std::path::Path;
use std::process;

const TEMPLATE_01_MINIMAL_TEST: &str = include_str!("../assets/01-minimal-test.au.toml");
const TEMPLATE_02_NESTED_TESTS: &str = include_str!("../assets/02-nested-tests.au.toml");
const TEMPLATE_03_ALL_SUPPORTED_FIELDS: &str =
    include_str!("../assets/03-all-supported-fields.au.toml");

pub fn default_template() -> String {
    let t01 = format_template("Minimal test", TEMPLATE_01_MINIMAL_TEST);
    let t02 = format_template("Nested tests", TEMPLATE_02_NESTED_TESTS);
    let t03 = format_template("All supported fields", TEMPLATE_03_ALL_SUPPORTED_FIELDS);

    [t01, comment_lines(&t02), comment_lines(&t03)].join("\n\n")
}

pub fn record_command(program: &str, arguments: &[String]) -> io::Result<ProgramOutput> {
    let program_output = process::Command::new(program).args(arguments).output()?;

    let stdout_content = String::from_utf8(program_output.stdout).map_err(io::Error::other)?;
    let stderr_content = String::from_utf8(program_output.stderr).map_err(io::Error::other)?;
    let exit_code = program_output
        .status
        .code()
        .ok_or_else(|| io::Error::other("process terminated by signal"))?;

    Ok(ProgramOutput {
        stdout: string::normalize_newlines(&stdout_content),
        stderr: string::normalize_newlines(&stderr_content),
        exit_code,
    })
}

pub fn generate_record_toml(
    program: &str,
    arguments: &[String],
    input_files: &[String],
    output: &ProgramOutput,
) -> String {
    let mut lines: Vec<String> = Vec::new();

    if !input_files.is_empty() {
        let inputs_toml = input_files
            .iter()
            .map(|p| toml::string_to_toml_literal(p))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("input_files = [{inputs_toml}]"));
    }

    lines.push(format!(
        "program = {}",
        toml::string_to_toml_literal(program)
    ));

    if !arguments.is_empty() {
        let args_toml = arguments
            .iter()
            .map(|a| toml::string_to_toml_literal(a))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("program_arguments = [{args_toml}]"));
    }

    lines.push(String::new());

    lines.push(format!(
        "expected_stdout = {}",
        toml::string_to_toml_literal(&output.stdout)
    ));
    lines.push(format!(
        "expected_stderr = {}",
        toml::string_to_toml_literal(&output.stderr)
    ));
    lines.push(format!("expected_exit_code = {}", output.exit_code));

    format!("{}\n", lines.join("\n"))
}

/// Scan `program` and `arguments` for tokens that resolve to existing relative
/// paths from `base_dir`. Returns a deduplicated, insertion-ordered list of
/// candidates suitable for `input_files`. Tokens that are absolute, look like
/// flags, contain `..`, or don't exist on disk are skipped.
pub fn detect_input_files(program: &str, arguments: &[String], base_dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for token in std::iter::once(program).chain(arguments.iter().map(String::as_str)) {
        let Some(rel) = as_relative_input_path(token, base_dir) else {
            continue;
        };
        if seen.insert(rel.clone()) {
            out.push(rel);
        }
    }

    out
}

fn as_relative_input_path(token: &str, base_dir: &Path) -> Option<String> {
    if token.is_empty() || token.starts_with('-') {
        return None;
    }
    let normalized = token.strip_prefix("./").unwrap_or(token);
    if normalized.is_empty() || normalized.contains('\\') || normalized.contains("..") {
        return None;
    }
    let path = Path::new(normalized);
    if path.is_absolute() {
        return None;
    }
    if !base_dir.join(path).exists() {
        return None;
    }
    Some(normalized.to_owned())
}

// HELPERS

fn format_template(title: &str, contents: &str) -> String {
    format!("# --- EXAMPLE: {title} ---\n{contents}") // Expect `content` to end with newline
}

fn comment_lines(contents: &str) -> String {
    string::format_lines(contents, |line| {
        if line.is_empty() {
            "".to_owned()
        } else {
            format!("# {line}")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn generate_record_toml_no_args_no_output() {
        let output = ProgramOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
        };
        let result = generate_record_toml("true", &[], &[], &output);
        assert_eq!(
            result,
            "program = \"true\"\n\nexpected_stdout = \"\"\nexpected_stderr = \"\"\nexpected_exit_code = 0\n"
        );
    }

    #[test]
    fn generate_record_toml_with_args_and_stdout() {
        let output = ProgramOutput {
            stdout: "Hello world".to_owned(),
            stderr: String::new(),
            exit_code: 0,
        };
        let args: Vec<String> = vec!["-n".to_owned(), "Hello world".to_owned()];
        let result = generate_record_toml("echo", &args, &[], &output);
        assert_eq!(
            result,
            concat!(
                "program = \"echo\"\n",
                "program_arguments = [\"-n\", \"Hello world\"]\n",
                "\n",
                "expected_stdout = \"Hello world\"\n",
                "expected_stderr = \"\"\n",
                "expected_exit_code = 0\n",
            )
        );
    }

    #[test]
    fn generate_record_toml_with_input_files() {
        let output = ProgramOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
        };
        let inputs = vec!["build.sh".to_owned(), "data".to_owned()];
        let args = vec!["data/in.csv".to_owned()];
        let result = generate_record_toml("./build.sh", &args, &inputs, &output);
        assert_eq!(
            result,
            concat!(
                "input_files = [\"build.sh\", \"data\"]\n",
                "program = \"./build.sh\"\n",
                "program_arguments = [\"data/in.csv\"]\n",
                "\n",
                "expected_stdout = \"\"\n",
                "expected_stderr = \"\"\n",
                "expected_exit_code = 0\n",
            )
        );
    }

    #[test]
    fn generate_record_toml_multiline_stdout() {
        let output = ProgramOutput {
            stdout: "line1\nline2\n".to_owned(),
            stderr: String::new(),
            exit_code: 0,
        };
        let result = generate_record_toml("cat", &[], &[], &output);
        assert_eq!(
            result,
            concat!(
                "program = \"cat\"\n",
                "\n",
                "expected_stdout = \"\"\"\nline1\nline2\n\"\"\"\n",
                "expected_stderr = \"\"\n",
                "expected_exit_code = 0\n",
            )
        );
    }

    #[test]
    fn generate_record_toml_nonzero_exit_code() {
        let output = ProgramOutput {
            stdout: String::new(),
            stderr: "oops\n".to_owned(),
            exit_code: 1,
        };
        let result = generate_record_toml("false", &[], &[], &output);
        assert_eq!(
            result,
            concat!(
                "program = \"false\"\n",
                "\n",
                "expected_stdout = \"\"\n",
                "expected_stderr = \"\"\"\noops\n\"\"\"\n",
                "expected_exit_code = 1\n",
            )
        );
    }

    #[test]
    fn detect_input_files_finds_existing_paths() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("build.sh"), "echo hi").unwrap();
        fs::create_dir(dir.path().join("data")).unwrap();
        fs::write(dir.path().join("data/in.csv"), "x").unwrap();

        let args = vec!["data/in.csv".to_owned(), "-n".to_owned()];
        let detected = detect_input_files("./build.sh", &args, dir.path());
        assert_eq!(
            detected,
            vec!["build.sh".to_owned(), "data/in.csv".to_owned()]
        );
    }

    #[test]
    fn detect_input_files_skips_nonexistent_absolute_and_flags() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("present.txt"), "x").unwrap();

        let args = vec![
            "--flag".to_owned(),
            "/absolute/path".to_owned(),
            "missing.txt".to_owned(),
            "../escape".to_owned(),
            "present.txt".to_owned(),
        ];
        let detected = detect_input_files("echo", &args, dir.path());
        assert_eq!(detected, vec!["present.txt".to_owned()]);
    }

    #[test]
    fn detect_input_files_deduplicates() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.txt"), "x").unwrap();

        let args = vec!["a.txt".to_owned(), "./a.txt".to_owned()];
        let detected = detect_input_files("cat", &args, dir.path());
        assert_eq!(detected, vec!["a.txt".to_owned()]);
    }
}
