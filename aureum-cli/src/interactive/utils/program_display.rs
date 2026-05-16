use aureum::TestCase;

use crate::utils::shell;

/// Renders a test's program + arguments as a `$`-prefixed shell command, with `.exe`
/// stripped on Windows so the result is pasteable on all platforms.
pub(crate) fn build_program_display(test_case: &TestCase) -> String {
    let path = &test_case.program_path;
    let is_exe = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    let name = if is_exe {
        path.file_stem()
    } else {
        path.file_name()
    }
    .map(|n| shell::shell_quote(&n.to_string_lossy()))
    .unwrap_or_default();
    let display = if test_case.arguments.is_empty() {
        name
    } else {
        let args: Vec<String> = test_case
            .arguments
            .iter()
            .map(|a| shell::shell_quote(a))
            .collect();
        format!("{name} {}", args.join(" "))
    };
    display.replace('\n', "\\n")
}
