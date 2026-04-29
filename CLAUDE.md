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

## Architecture

Two-crate Rust workspace (Edition 2024):

- **`aureum/`** — core library: config parsing, test execution, result comparison, output formatting
- **`aureum-cli/`** — CLI binary (`aureum`): argument parsing, config file discovery, result reporting

### Rust-specific opinions

`use` statements:

- Prefer using absolute path to modules (i.e. start with `crate::` instead of `super::`). `super::` may still be used in test sections.
- Import data types at the top (merging them in curly braces if necessary).
- Do not import specific functions directly. Instead import the namespace and the refer to the last component of the namespace when calling the function.

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

| Type         | File                        | Purpose                                                                      |
| ------------ | --------------------------- | ---------------------------------------------------------------------------- |
| `TestCase`   | `aureum/src/test_case.rs`   | Program + args + stdin + expected outputs                                    |
| `TestId`     | `aureum/src/test_id.rs`     | Hierarchical dot-notation test identifier                                    |
| `TestRunner` | `aureum/src/test_runner.rs` | Spawns subprocesses, captures I/O, compares results                          |
| `TomlConfig` | `aureum/src/toml/config.rs` | Parsed `.au.toml` — supports literals, `{ file = "..." }`, `{ env = "..." }` |

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
