#!/usr/bin/env bash

set -e


# Build the program
cargo build ${RELEASE:+--release}

# Set the program path
if [[ -n $RELEASE ]]; then
    PROGRAM_PATH="$PWD/target/release/aureum"
else
    PROGRAM_PATH="$PWD/target/debug/aureum"
fi

# Export the environment variables required by the tests
# GitHub's Windows runner has multiple Bash shells: https://github.com/actions/runner-images/blob/751fe08d9840d2273fb1986980c5f18f3a920e64/images/win/Windows2022-Readme.md#shells
export AUREUM_TEST_BASH="${SHELL:-bash}" # Use the same shell that is executing this file
export AUREUM_TEST_EXEC="$PROGRAM_PATH"
export AUREUM_TEST_HELLO_WORLD="Hello world" # Required by `basic/read_env_var.au.toml`

# Run the tests
"$PROGRAM_PATH" "${@:-spec}"
