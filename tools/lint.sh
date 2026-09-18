#!/usr/bin/env bash
# The lint gate. Runs every mechanical check in docs/style.md's "Enforced
# mechanically" table and stops at the first failure. See docs/style.md
# "Running the gate" for how to wire this in as a pre-commit hook and how the
# clippy allow list in the root Cargo.toml was measured.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

step() {
  echo "==> $1"
}

step "cargo fmt --all --check"
cargo fmt --all --check

# --lib --bins, not --all-targets: integration tests and doctests would
# otherwise need the same unwrap_used/expect_used/panic exemption library
# code gets from its #![cfg_attr(test, allow(...))] line, and this scoping
# gets the same production-code guarantee without adding that noise. See
# docs/style.md "Running the gate" for the measurement command this mirrors.
step "cargo clippy --workspace --lib --bins -- -D warnings"
cargo clippy --workspace --lib --bins -- -D warnings

# Code behind an off-by-default feature never compiles in the run above, so it
# rots unseen. The dds encode path drifted that way until 2026-09-18.
step "cargo clippy --workspace --all-features --all-targets -- -D warnings"
cargo clippy --workspace --all-features --all-targets -- -D warnings

# Both ends of the feature range, because a helper gated on one feature while
# its callers need two reads as dead code in between. That is how the viseme
# test helpers slipped through on 2026-09-18: the fork ran only --all-features.
# This does not cover every combination, so a crate with more than two features
# wants cargo hack rather than another line here.
step "cargo clippy --workspace --no-default-features --all-targets -- -D warnings"
cargo clippy --workspace --no-default-features --all-targets -- -D warnings

step "comment-lint crates docs"
if command -v comment-lint >/dev/null 2>&1; then
  comment-lint crates docs
else
  echo "comment-lint not installed, skipping"
fi

step "tools/prose-lint.sh"
tools/prose-lint.sh

step "taplo fmt --check"
if command -v taplo >/dev/null 2>&1; then
  taplo fmt --check
else
  echo "taplo not installed, skipping"
fi

step "markdownlint-cli2"
if command -v markdownlint-cli2 >/dev/null 2>&1; then
  markdownlint-cli2 "docs/**/*.md" "*.md"
else
  echo "markdownlint-cli2 not installed, skipping"
fi

step "cargo test --workspace"
cargo test --workspace

echo "==> lint gate passed"
