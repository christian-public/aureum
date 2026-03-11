#!/usr/bin/env bash
set -e

# $PWD refers to the folder where the `.au.toml` config file is located
"$AUREUM_TEST_EXEC" test --show-all-tests "$PWD/helpers/dir1/test1.toml"
