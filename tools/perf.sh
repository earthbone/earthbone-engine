#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
mode="${1:-check}"
if [[ "$mode" != check && "$mode" != refresh ]]; then
  printf 'usage: tools/perf.sh [check|refresh]\n' >&2
  exit 2
fi

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
rtk cargo bench -p soe-bench --bench prf_15 -- --color never --sample-count 30 --min-time 0.1 >"$tmp"
if [[ -n "${SKYRIM_DATA:-}" ]]; then
  printf 'Installed-data NIF and BSA read benchmarks ran from SKYRIM_DATA.\n'
fi
rtk python3 tools/perf_baselines.py "$mode" --output "$tmp" --baseline docs/perf-baselines.json
