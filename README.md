# Aureum

[![CI](https://github.com/christian-public/aureum/actions/workflows/ci.yml/badge.svg)](https://github.com/christian-public/aureum/actions/workflows/ci.yml)

`aureum` is a golden test runner for executables.

Key functionality:

- Language-agnostic: Configure tests using [TOML](https://toml.io) files.
- A configuration file may contain multiple tests.
- Each test can provide the expected value for `stdout`, `stderr` and `exit code`. See [format](#aureum-configuration-format) below.
- Tests can reference external files and environment variables.
- Supports two output formats: `summary` and [`tap`](https://testanything.org).
- Provides helpful error messages.
- `aureum` is tested by `aureum`. See [`golden/`](golden) directory.
- Runs on Linux, macOS and Windows.

This tool is best suited for testing stateless executables: Given the same input, they always produce the same output.

Inspired by [Idris 2's golden test runner](https://github.com/idris-lang/Idris2/tree/main/tests).

## Installation

1. `git clone https://github.com/christian-public/aureum`
2. `cd aureum`
3. `cargo install --path aureum-cli`

The `aureum` executable should now be available in your `$PATH`.

## Basic usage

```bash
aureum test [OPTIONS] <PATHS>...
```

Detailed usage is shown below:

```bash
$ aureum test --help
Run tests

Usage: aureum test [OPTIONS] <PATHS>...

Arguments:
  <PATHS>...  Paths to config files

Options:
      --format <FORMAT>            Options: summary, tap [default: summary]
      --default-timeout <SECONDS>  Fallback timeout for tests without a timeout [default: 5]
      --parallel                   Run tests in parallel
      --watch                      Re-run tests when config or watched files change
      --interactive                Interactively review and accept new test expectations
      --verbose                    Print extra information about config files
  -h, --help                       Print help
```

When running `aureum test`, you may specify one or more files/directories/[glob patterns](<https://en.wikipedia.org/wiki/Glob_(programming)>). When specifying a directory, `aureum` will search for files with the file extension `.au.toml`. This file extension was chosen to allow other `.toml` files to be located in the same directory structure as the Aureum-specific config files.

## Example

Create a file named `hello.au.toml` with the following contents:

```toml
program = "echo"
program_arguments = ["-n", "Hello world"]

expected_stdout = "Hello world"
```

Running the command `aureum test hello.au.toml` will output the following:

```
🚀 Running 1 test:
.

Test result: OK (1 passed)
```

## Aureum configuration format

The following fields are supported in an Aureum config file:

```toml
skip = "Reason"         # String
program = ""            # String (Required field)
program_arguments = []  # List of strings
stdin = ""              # String

# At least one of the `expected_*` fields are required:
expected_stdout = ""    # String
expected_stderr = ""    # String
expected_exit_code = 0  # Integer
timeout_seconds = 30    # Integer (Must be 0 or greater)
```

In addition to the literal values mentioned above, the following special forms are available:

- `{ env = "MY_ENV_VAR" }` — Read the value from the environment variable named `MY_ENV_VAR`.
- `{ file = "my_test.stdout" }` — Read the external file `my_test.stdout` from the same directory as the config file.

Recommended file extension: `.au.toml`

### Multiple tests per file

An Aureum config file may contain multiple tests. To specify a sub-test you can add a header using the following format: `[[tests]]`, include an `id` field (example: `id = "id_of_test"`) and configure the test as normal.

When specifying multiple tests, the top-level fields are no longer treated as a test itself. Instead, its fields are inherited by each sub-test unless overridden. The following example configures two tests, where both tests run the program `/bin/echo`:

Filename: `multiple_tests.au.toml`

```toml
program = "echo"


[[tests]]
id = "test1"
program_arguments = ["-n", "Test 1"]
expected_stdout = "Test 1"


[[tests]]
id = "test2"
program_arguments = ["-n", "Test 2"]
expected_stdout = "Test 2"
```

Running the command `aureum test multiple_tests.au.toml` will output the following:

```
🚀 Running 2 tests:
..

Test result: OK (2 passed)
```

## AI usage

Commits after March 11, 2026 may contain AI-generated code.

The `--interactive` feature was built almost entirely by Claude Code.

## Alternative tools

- [trycmd](https://github.com/assert-rs/trycmd) [Rust]
- [Golden Tests](https://github.com/jfecher/golden-tests) [Rust]
- [Smoke](https://github.com/SamirTalwar/smoke) [Haskell]
- [goldplate](https://github.com/fugue/goldplate) [Haskell]
- [REPLica](https://github.com/ReplicaTest/REPLica) [Idris]

## License

Aureum is released under the [3-clause BSD license](LICENSE).
