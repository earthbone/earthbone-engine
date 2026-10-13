#!/usr/bin/env bash
# CONTEXT: The gate retains its exit status even when optional history fails.
set -uo pipefail
[ -n "${SOE_TEST_HISTORY:-}" ] || exit 0
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="${SOE_TEST_HISTORY_BIN:-$root/target/debug/soe-test-history}"
[ -x "$binary" ] || exit 0
kind="$1"
log="$2"
code="$3"
started="$4"
ended="$5"
commit="$(git -C "$root" rev-parse HEAD)"
branch="$(git -C "$root" symbolic-ref --short -q HEAD || echo detached)"
parents="$(git -C "$root" show -s --format=%P HEAD)"
if [ "$kind" = tier1 ]; then
  unset SKYRIM_DATA SOE_TEST_SAVE SOE_TEST_SAVES
fi
"$binary" record --log "$log" --id "$log:$kind:$started:$commit" --kind "$kind" --exit "$code" \
  --commit "$commit" --branch "$branch" --parents "$parents" \
  --started "$started" --ended "$ended" --driver tools/lint.sh
