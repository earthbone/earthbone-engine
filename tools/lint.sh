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

history_step() {
  local kind="$1"
  shift
  if [ -z "${SOE_TEST_HISTORY:-}" ]; then
    "$@"
    return
  fi
  local directory="${SOE_TEST_HISTORY_LOG_DIR:-${SOE_TEST_HISTORY}.logs}"
  local log started ended code stdout_pid stderr_pid
  if ! mkdir -p "$directory"; then
    "$@"
    return
  fi
  if ! log="$(mktemp "$directory/$kind.XXXXXXXX.log")"; then
    "$@"
    return
  fi
  if ! mkfifo "$log.stdout.pipe" "$log.stderr.pipe"; then
    rm -f "$log.stdout.pipe" "$log.stderr.pipe"
    "$@"
    return
  fi
  tee -a "$log" < "$log.stdout.pipe" &
  stdout_pid=$!
  tee -a "$log" < "$log.stderr.pipe" >&2 &
  stderr_pid=$!
  started="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  if "$@" > "$log.stdout.pipe" 2> "$log.stderr.pipe"; then
    code=0
  else
    code=$?
  fi
  wait "$stdout_pid" || :
  wait "$stderr_pid" || :
  rm -f "$log.stdout.pipe" "$log.stderr.pipe"
  ended="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  tools/test-history-record.sh "$kind" "$log" "$code" "$started" "$ended" >/dev/null 2>&1 || :
  return "$code"
}

step "cargo fmt --all --check"
cargo fmt --all --check

# --lib --bins, not --all-targets: integration tests and doctests would
# otherwise need the same unwrap_used/expect_used/panic exemption library
# code gets from its #![cfg_attr(test, allow(...))] line, and this scoping
# gets the same production-code guarantee without adding that noise. See
# docs/style.md "Running the gate" for the measurement command this mirrors.
step "cargo clippy --workspace --lib --bins -- -D warnings"
history_step clippy-prod cargo clippy --workspace --lib --bins -- -D warnings

step "library logging facade"
cargo clippy --workspace --lib -- -D warnings -D clippy::print_stdout -D clippy::print_stderr

# Code behind an off-by-default feature never compiles in the run above, so it
# rots unseen. The dds encode path drifted that way until 2026-09-18.
step "cargo clippy --workspace --all-features --all-targets -- -D warnings"
history_step clippy-all cargo clippy --workspace --all-features --all-targets -- -D warnings

# Both ends of the feature range, because a helper gated on one feature while
# its callers need two reads as dead code in between. That is how the viseme
# test helpers slipped through on 2026-09-18: the fork ran only --all-features.
# This does not cover every combination, so a crate with more than two features
# wants cargo hack rather than another line here.
step "cargo clippy --workspace --no-default-features --all-targets -- -D warnings"
history_step clippy-nodef cargo clippy --workspace --no-default-features --all-targets -- -D warnings

step "cargo deny check"
if command -v cargo-deny >/dev/null 2>&1; then
  cargo deny check
else
  echo "cargo-deny not installed, skipping"
fi

step "cargo audit"
if command -v cargo-audit >/dev/null 2>&1; then
  cargo audit
else
  echo "cargo-audit not installed, skipping"
fi

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

# The fuzz targets are their own cargo workspace, so every command above walks
# past them. Their seed helpers went unformatted until 2026-09-18 for exactly
# that reason. Building them needs the nightly toolchain cargo-fuzz wants, so
# this checks what a stable toolchain can and says so when it cannot.
step "fuzz workspace formatting"
if [ -f fuzz/Cargo.toml ]; then
  cargo fmt --manifest-path fuzz/Cargo.toml --all --check
else
  echo "no fuzz workspace, skipping"
fi

# Tier 1 is this run: no SKYRIM_DATA, no save corpus, under five minutes. The
# gated runs against the installed game and the 60 save corpus are tier 2 and
# take about twelve minutes in debug, so they are not in the commit gate. Run
# them with SKYRIM_DATA and SOE_TEST_SAVES set, in release, before a merge.
step "performance runner contracts"
python3 -m unittest discover -s tools/tests

step "cargo test --workspace (tier 1, gated tests skipped)"
history_step tier1 env -u SKYRIM_DATA -u SOE_TEST_SAVE -u SOE_TEST_SAVES cargo test --workspace

echo "==> lint gate passed"
