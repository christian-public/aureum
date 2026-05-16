#!/usr/bin/env bash

set -e


cargo build ${RELEASE:+--release}

cargo test --quiet
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
RELEASE=$RELEASE SKIP_BUILD=1 ./check-golden.sh format --check golden examples aureum-cli/assets

default_command=(test --parallel golden examples aureum-cli/assets)
RELEASE=$RELEASE SKIP_BUILD=1 ./check-golden.sh "${@:-${default_command[@]}}"
