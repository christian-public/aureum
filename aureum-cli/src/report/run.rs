use crate::report::theme;
use crate::utils::toml;
use aureum::{ProgramOutput, TestId};

pub fn print_verbose_is_not_supported_in_passthrough() {
    eprintln!(
        "{} `--verbose` is not supported in passthrough mode",
        theme::error()
    );
    eprintln!(
        "{} use `--format toml` for detailed output or remove `--verbose`",
        theme::hint()
    );
}

pub fn print_default_timeout_is_not_supported_in_passthrough() {
    eprintln!(
        "{} `--default-timeout` is not supported in passthrough mode",
        theme::error()
    );
    eprintln!(
        "{} use `--format toml`, or set `timeout_seconds` in the config",
        theme::hint()
    );
}

pub fn print_failed_to_run_program() {
    eprintln!("{} failed to run program", theme::error());
}

pub fn print_one_or_more_programs_failed_to_run() {
    eprintln!("{} one or more programs failed to run", theme::error());
}

pub fn print_test_id_as_toml_comment(test_id: &TestId) {
    println!("# TEST: {}", test_id.display_id());
}

pub fn print_failed_to_run_program_as_toml() {
    println!("# ERROR: Failed to run program");
}

pub fn print_output_as_toml(output: &ProgramOutput) {
    println!(
        "expected_stdout = {}",
        toml::string_to_toml_literal(&output.stdout)
    );
    println!(
        "expected_stderr = {}",
        toml::string_to_toml_literal(&output.stderr)
    );
    println!("expected_exit_code = {}", output.exit_code);
}
