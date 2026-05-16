# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Project Is

**Aureum** is a language-agnostic golden test runner for executables. It runs programs and verifies their output (stdout, stderr, exit codes) against expected values defined in `.au.toml` config files.

## Commands

### Build

```bash
cargo build           # debug build
cargo build --release # optimized build
```

### Test

```bash
cargo test                          # all unit tests
cargo test <test_name>              # single test by name
./test_spec.sh test spec            # all golden tests (uses debug build)
RELEASE=1 ./test_spec.sh test spec  # golden tests with release build
./test_spec.sh test spec/basic/expect_stdout.au.toml  # single golden test file
```

### Lint & Format

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
```

### Before reporting work done

After any Rust code change, run all four of these and fix any failures or warnings before reporting the task complete:

1. `cargo fmt`
2. `cargo test`
3. `cargo clippy --all-targets --all-features -- -D warnings`
4. `./test_spec.sh test spec`

## Architecture

Two-crate Rust workspace (Edition 2024):

- **`aureum/`** — core library: config parsing, test execution, result comparison, output formatting
- **`aureum-cli/`** — CLI binary (`aureum`): argument parsing, config file discovery, result reporting

### Rust-specific opinions

`use` statements:

- Prefer using absolute path to modules (i.e. start with `crate::` instead of `super::`). `super::` may still be used in test sections.
- Import data types at the top (merging them in curly braces if necessary).
- Do not import specific functions directly. Instead import the namespace and the refer to the last component of the namespace when calling the function.
- Don't write inline `crate::foo::bar(...)` calls in function bodies. Import the module at the top (`use crate::foo;`) and call via the short name (`foo::bar(...)`).

Module structure:

- `mod.rs` is structural only — module declarations and `pub use` re-exports. No logic.
- Shared helpers used by submodules go in a dedicated named file (e.g. `commands/common.rs`), not in `mod.rs`. Files are cheap.

### CLI Subcommands

The `aureum` binary exposes these subcommands (see `aureum-cli/src/args.rs`):

- `init` — create a new `.au.toml` from a recorded command invocation
- `validate` — check that config files parse and pass validation
- `list` — list tests discovered in config files (supports `--tree`, `--show all|runnable|skipped`)
- `run` — run the programs from a config file, forwarding their output (`--format passthrough|toml`)
- `test` — run tests and compare against expectations
- `format` — rewrite `.au.toml` files in canonical form (`--check` exits non-zero if changes would be made)
- `version` — print the version

Notable `test` flags:

- `--parallel` — run tests concurrently via rayon
- `--watch` — re-run tests when config files or referenced files change
- `--interactive` — review diffs in a TUI and accept new expectations (writes back via `toml_edit`)
- `--format summary|tap` — choose between human-readable summary or TAP output
- `--default-timeout SECONDS` — fallback per-test timeout (default 5)

### Core Data Flow

```
.au.toml files
  → Config parsing (aureum/src/toml/)
  → Requirement gathering (external files, env vars)
  → TestCase construction (aureum/src/test_case.rs)
  → Subprocess execution (aureum/src/test_runner.rs)
  → Result comparison with diff
  → Output: tree summary or TAP format
  → (Optional) Interactive TUI review session (aureum-cli/src/interactive/)
```

### Key Types

| Type              | File                              | Purpose                                                                      |
| ----------------- | --------------------------------- | ---------------------------------------------------------------------------- |
| `TestCase`        | `aureum/src/test_case.rs`         | Program + args + stdin + expected outputs                                    |
| `SubtestPath`     | `aureum/src/subtest_path.rs`      | Within-file dot-notation path identifying a subtest                          |
| `TestId`          | `aureum/src/test_id.rs`           | Globally unique test id (config dir + file name + `SubtestPath`)             |
| `TestOutcome`     | `aureum/src/test_outcome.rs`      | Per-test result: stdout / stderr / exit_code as `FieldOutcome<T>`            |
| `FieldOutcome<T>` | `aureum/src/test_outcome.rs`      | `NotChecked(T)` / `Matches(T)` / `Diff { expected, got }`                    |
| `TestRunner`      | `aureum/src/test_runner.rs`       | Spawns subprocesses, captures I/O, compares results                          |
| `TomlConfigFile`  | `aureum/src/toml/config.rs`       | Parsed `.au.toml` — supports literals, `{ file = "..." }`, `{ env = "..." }` |

### Test Configuration Format

```toml
program = "echo"
program_arguments = ["-n", "Hello"]
expected_stdout = "Hello"       # literal, or { file = "path" }, or { env = "VAR" }
expected_exit_code = 0

[[tests]] # Multiple tests in one file
id = "subtest1"
program_arguments = ["arg"]
expected_stdout = "output"
```

### Exit Codes

- `0` — all tests passed
- `1` — test failures, run failures, or general errors
- `2` — invalid usage (bad arguments)
- `3` — config error

### Self-Testing

Aureum is tested by Aureum — the `spec/` directory contains the golden test suite. CI runs `./test_spec.sh test spec` on Ubuntu, macOS, and Windows.
