#!/bin/bash
# This script is meant to be run as a pre-commit hook.
# It ensures that code is formatted, linted and passes tests before committing.
#
# To install it, from the root of the repository, run:
# ln -s -f ../../scripts/pre-commit.sh .git/hooks/pre-commit

set -e

echo "Running pre-commit hook..."

echo "  cargo fmt"
cargo fmt -- --check

echo "  cargo clippy"
SQLX_OFFLINE=true cargo clippy -- -D warnings

echo "  cargo test"
cargo test

echo "Pre-commit hook passed." 

echo "  sqlx prepare check"
cargo sqlx prepare --workspace --check -- --all-targets