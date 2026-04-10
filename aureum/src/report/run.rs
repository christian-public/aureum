use crate::report::label;
use crate::test_case::TestCase;
use crate::test_runner::ProgramOutput;

pub fn print_verbose_is_not_supported_in_passthrough() {
    eprintln!(
        "{} `--verbose` is not supported in passthrough mode",
        label::error()
    );
    eprintln!(
        "{} You may want to use `--format toml` instead",
        label::hint()
    );
}

pub fn print_failed_to_run_program() {
    eprintln!("{} Failed to run program", label::error());
}

pub fn print_one_or_more_programs_failed_to_run() {
    eprintln!("{} One or more programs failed to run", label::error());
}

pub fn print_test_case_id_as_toml_comment(test_case: &TestCase) {
    println!("# TEST: {}", test_case.id());
}

pub fn print_failed_to_run_program_as_toml() {
    println!("# ERROR: Failed to run program");
}

pub fn print_output_as_toml(output: &ProgramOutput) {
    println!("expected_stdout = {}", format_toml_string(&output.stdout));
    println!("expected_stderr = {}", format_toml_string(&output.stderr));
    println!("expected_exit_code = {}", output.exit_code);
}

fn format_toml_string(s: &str) -> String {
    if s.contains('\n') {
        let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"\"\"\n{escaped}\"\"\"")
    } else {
        let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    }
}
