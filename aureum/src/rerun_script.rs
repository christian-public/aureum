use crate::test_case::TestCase;
use std::fs;
use std::io;
use std::path::Path;

/// Name of the rerun script dropped into each per-test scratch dir: a POSIX
/// `sh` script on Unix, a PowerShell `.ps1` on Windows. Consumers reference
/// this constant so they get the right name per platform.
#[cfg(not(windows))]
pub const RERUN_SCRIPT_NAME: &str = "aureum-rerun.sh";
#[cfg(windows)]
pub const RERUN_SCRIPT_NAME: &str = "aureum-rerun.ps1";
/// Sidecar holding the test's stdin, piped into the rerun. Extensionless, so
/// it is the same name on every platform.
pub const STDIN_SIDECAR_NAME: &str = ".aureum-stdin";

/// Render the POSIX `sh` rerun script. Pure string assembly so it can be
/// golden-tested without constructing a `TestId` or touching disk.
///
/// The script `cd`s to its own directory before exec'ing, so scratch-relative
/// argument paths and the stdin sidecar resolve exactly as they did under
/// aureum. `program` is always absolute (see [`TestCase::program_path`]), so it
/// resolves regardless of cwd or the rerunner's PATH.
// Used by `write` only on Unix; tests exercise it everywhere. Without this the
// plain (non-test) lib target trips `dead_code` on Windows, where clippy also
// runs with `-D warnings`.
#[cfg_attr(not(unix), allow(dead_code))]
pub fn render_sh(display_id: &str, program: &str, args: &[&str], has_stdin: bool) -> String {
    let mut out = String::new();
    out.push_str("#!/bin/sh\n");
    // `display_id` lands inside a `#` comment. Strip newlines so a crafted id
    // (file names are attacker-adjacent) can't break out into a runnable line.
    out.push_str("# Rerun of aureum test: ");
    out.push_str(&comment_safe(display_id));
    out.push('\n');
    out.push_str(
        "# Left by --keep-scratch. Runs with your current environment;\n\
         # aureum injected none of its own (it inherited yours).\n",
    );
    out.push_str("cd \"$(dirname \"$0\")\" || exit 1\n");
    out.push_str("exec ");
    out.push_str(&sh_quote(program));
    for arg in args {
        out.push(' ');
        out.push_str(&sh_quote(arg));
    }
    if has_stdin {
        out.push_str(" < ");
        out.push_str(&sh_quote(STDIN_SIDECAR_NAME));
    }
    out.push('\n');
    out
}

/// Single-quote a token for POSIX sh, escaping embedded single quotes via the
/// classic `'\''` close-reopen trick. Safe for arbitrary shell-word bytes:
/// spaces, newlines, globs, `$`, quotes.
#[cfg_attr(not(unix), allow(dead_code))]
fn sh_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// Replace newlines with spaces so a value can be embedded in a single-line
/// `#` comment without spilling into an executable line.
fn comment_safe(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect()
}

/// Render the Windows PowerShell rerun script. Pure string assembly (ungated,
/// so it unit-tests on any host, including the Linux dev box).
///
/// PowerShell differs from `sh` in three ways this handles:
/// - No `exec`: the program runs as a child, and we forward its exit code with
///   `exit $LASTEXITCODE`.
/// - No `<` stdin redirection: when there is stdin we pipe the sidecar in with
///   `Get-Content -Raw`. This is *not* byte-exact — PowerShell re-encodes and
///   typically appends a trailing newline — so it is faithful for text, lossy
///   for binary. Acceptable for a debugging aid.
/// - Scripts are not freely runnable under the default execution policy, so a
///   header comment spells out the bypass invocation.
///
/// `program` is absolute (see [`TestCase::program_path`]); `Set-Location
/// $PSScriptRoot` makes scratch-relative argument paths and the sidecar resolve
/// as they did under aureum.
// Mirror of `render_sh`: used by `write` only on Windows, exercised by tests
// everywhere, so it is dead in the non-test lib target on Unix/macOS.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn render_ps1(display_id: &str, program: &str, args: &[&str], has_stdin: bool) -> String {
    let mut out = String::new();
    // No shebang, so the id comment is line 0 (the breakout guard checks it).
    out.push_str("# Rerun of aureum test: ");
    out.push_str(&comment_safe(display_id));
    out.push('\n');
    out.push_str(
        "# Left by --keep-scratch. Runs with your current environment;\n\
         # aureum injected none of its own (it inherited yours).\n\
         # Run with: powershell -NoProfile -ExecutionPolicy Bypass -File aureum-rerun.ps1\n",
    );
    // Opt into sane native-argument passing on PowerShell 7.3+. Harmless (just
    // sets an unused variable) on Windows PowerShell 5.1, which can still
    // mis-marshal arguments containing spaces or quotes to native programs.
    out.push_str("$PSNativeCommandArgumentPassing = 'Standard'\n");
    out.push_str("Set-Location -LiteralPath $PSScriptRoot\n");
    if has_stdin {
        out.push_str("Get-Content -LiteralPath ");
        out.push_str(&ps_quote(STDIN_SIDECAR_NAME));
        out.push_str(" -Raw | ");
    }
    out.push_str("& ");
    out.push_str(&ps_quote(program));
    for arg in args {
        out.push(' ');
        out.push_str(&ps_quote(arg));
    }
    out.push('\n');
    out.push_str("exit $LASTEXITCODE\n");
    out
}

/// Quote a token as a PowerShell single-quoted string literal: wrap in `'…'`
/// and double any embedded `'`. Single-quoted strings are fully literal in
/// PowerShell — no variable, escape, or backslash processing — mirroring the
/// intent of [`sh_quote`]. NB: this only controls how PowerShell *parses* the
/// token; how PowerShell then hands it to a native program is a separate layer
/// (see the `$PSNativeCommandArgumentPassing` note in [`render_ps1`]).
#[cfg_attr(not(windows), allow(dead_code))]
fn ps_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// Write the rerun script (and stdin sidecar, if any) into `dir`.
///
/// Best-effort by contract: a missing debugging convenience must never turn an
/// otherwise-passing test red, so the caller ignores the result. Emits a POSIX
/// `sh` script on Unix and a PowerShell `.ps1` on Windows; a no-op stub on any
/// other platform. The renderer choice and the Unix executable bit are the
/// only platform-specific parts — everything else is shared.
#[cfg(any(unix, windows))]
pub fn write(test_case: &TestCase, dir: &Path) -> io::Result<()> {
    let args: Vec<&str> = test_case.arguments.iter().map(String::as_str).collect();
    let program = test_case.program_path.to_string_lossy();
    let has_stdin = test_case.stdin.is_some();

    #[cfg(unix)]
    let script = render_sh(&test_case.display_id(), &program, &args, has_stdin);
    #[cfg(windows)]
    let script = render_ps1(&test_case.display_id(), &program, &args, has_stdin);

    let script_path = dir.join(RERUN_SCRIPT_NAME);
    fs::write(&script_path, script)?;

    // Mark the `sh` script executable so `./aureum-rerun.sh` works. The `.ps1`
    // needs no equivalent — it is run via the PowerShell interpreter.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))?;
    }

    if let Some(stdin) = &test_case.stdin {
        fs::write(dir.join(STDIN_SIDECAR_NAME), stdin)?;
    }
    Ok(())
}

/// No-op on platforms that are neither Unix nor Windows: we have no rerun
/// script dialect to emit there. Kept as a same-signature stub so the runner's
/// call site needs no `cfg`.
#[cfg(not(any(unix, windows)))]
pub fn write(_test_case: &TestCase, _dir: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_args_and_pipes_stdin_from_sidecar() {
        let script = render_sh(
            "golden/echo.au.toml:hello",
            "/usr/bin/echo",
            &["-n", "a b", "it's"],
            true,
        );
        assert_eq!(
            script,
            "#!/bin/sh\n\
             # Rerun of aureum test: golden/echo.au.toml:hello\n\
             # Left by --keep-scratch. Runs with your current environment;\n\
             # aureum injected none of its own (it inherited yours).\n\
             cd \"$(dirname \"$0\")\" || exit 1\n\
             exec '/usr/bin/echo' '-n' 'a b' 'it'\\''s' < '.aureum-stdin'\n",
        );
    }

    #[test]
    fn omits_redirect_when_no_stdin() {
        let script = render_sh("t", "/bin/true", &[], false);
        assert!(script.trim_end().ends_with("exec '/bin/true'"));
        assert!(!script.contains(STDIN_SIDECAR_NAME));
    }

    #[test]
    fn id_comment_cannot_break_out_to_a_runnable_line() {
        let script = render_sh("evil\nrm -rf /tmp/x", "/bin/true", &[], false);
        assert!(!script.contains("\nrm -rf"));
        assert_eq!(
            script.lines().nth(1).unwrap(),
            "# Rerun of aureum test: evil rm -rf /tmp/x",
        );
    }

    #[test]
    fn sh_quote_wraps_and_escapes_single_quotes() {
        assert_eq!(sh_quote("plain"), "'plain'");
        assert_eq!(sh_quote("a b"), "'a b'");
        assert_eq!(sh_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn ps1_quotes_args_and_pipes_stdin_from_sidecar() {
        let script = render_ps1(
            "golden/echo.au.toml:hello",
            "C:\\bin\\echo.exe",
            &["-n", "a b", "it's"],
            true,
        );
        assert_eq!(
            script,
            "# Rerun of aureum test: golden/echo.au.toml:hello\n\
             # Left by --keep-scratch. Runs with your current environment;\n\
             # aureum injected none of its own (it inherited yours).\n\
             # Run with: powershell -NoProfile -ExecutionPolicy Bypass -File aureum-rerun.ps1\n\
             $PSNativeCommandArgumentPassing = 'Standard'\n\
             Set-Location -LiteralPath $PSScriptRoot\n\
             Get-Content -LiteralPath '.aureum-stdin' -Raw | & 'C:\\bin\\echo.exe' '-n' 'a b' 'it''s'\n\
             exit $LASTEXITCODE\n",
        );
    }

    #[test]
    fn ps1_omits_pipe_when_no_stdin() {
        let script = render_ps1("t", "C:\\p.exe", &[], false);
        assert!(!script.contains("Get-Content"));
        assert!(!script.contains(STDIN_SIDECAR_NAME));
        assert!(script.contains("\n& 'C:\\p.exe'\nexit $LASTEXITCODE\n"));
    }

    #[test]
    fn ps1_id_comment_cannot_break_out_to_a_runnable_line() {
        // No shebang, so the id comment is the first line.
        let script = render_ps1("evil\nRemove-Item x", "C:\\p.exe", &[], false);
        assert!(!script.contains("\nRemove-Item"));
        assert_eq!(
            script.lines().next().unwrap(),
            "# Rerun of aureum test: evil Remove-Item x",
        );
    }

    #[test]
    fn ps_quote_wraps_and_doubles_single_quotes() {
        assert_eq!(ps_quote("plain"), "'plain'");
        assert_eq!(ps_quote("a b"), "'a b'");
        assert_eq!(ps_quote("it's"), "'it''s'");
    }
}
