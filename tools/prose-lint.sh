#!/usr/bin/env bash
# House style, checked with plain grep/awk so it runs even where comment-lint
# is not installed (see docs/style.md and tools/lint.sh): no em dash, no en
# dash, and no semicolon used as a prose separator in Markdown. comment-lint,
# when present, checks the same dash rule on Rust comments plus more; this
# script is the dependency-free fallback and the only Markdown-prose check.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

fail=0

# Markdown and Rust files, skipping the two crates another workstream owns
# (see CLAUDE.md "Working rules"): flagging their prose would ask this repo's
# gate to fix comments this pass has no license to touch.
mapfile -t md_files < <(find . \( -path './target' -o -path './.git' \) -prune -o \
  -name '*.md' -print | sed 's#^\./##')
mapfile -t rs_files < <(find crates -name '*.rs' \
  -not -path 'crates/game/*' -not -path 'crates/papyrus/src/*')

check_dashes() {
  local file="$1"
  local hits
  hits=$(grep -nP '[\x{2014}\x{2013}]' "$file" || true)
  if [[ -n "$hits" ]]; then
    fail=1
    while IFS= read -r line; do
      echo "$file:$line: em dash or en dash, use a comma, a period, or a restructure"
    done <<<"$hits"
  fi
}

for f in "${md_files[@]}"; do
  [[ -f "$f" ]] || continue
  check_dashes "$f"
done

for f in "${rs_files[@]}"; do
  # Only the comment text, not code that happens to contain the bytes.
  hits=$(grep -nP '//.*[\x{2014}\x{2013}]' "$f" || true)
  if [[ -n "$hits" ]]; then
    fail=1
    while IFS= read -r line; do
      echo "$f:$line: em dash or en dash in a comment, use a comma, a period, or a restructure"
    done <<<"$hits"
  fi
done

# Semicolons as a Markdown prose separator, outside fenced code blocks,
# inline code spans, and HTML entities (&nbsp; and friends).
for f in "${md_files[@]}"; do
  [[ -f "$f" ]] || continue
  bad="$(awk '
    BEGIN { fence = 0 }
    /^(```|~~~)/ { fence = !fence; next }
    fence { next }
    {
      line = $0
      gsub(/`[^`]*`/, "", line)
      gsub(/&[A-Za-z0-9#]+;/, "", line)
      if (line ~ /;/) print NR ": " $0
    }
  ' "$f")"
  if [[ -n "$bad" ]]; then
    fail=1
    while IFS= read -r line; do
      echo "$f:$line: semicolon in Markdown prose, use a period, a comma, or a restructure"
    done <<<"$bad"
  fi
done

if [[ "$fail" -ne 0 ]]; then
  echo "prose-lint: house style violations found"
  exit 1
fi
echo "prose-lint: clean"
