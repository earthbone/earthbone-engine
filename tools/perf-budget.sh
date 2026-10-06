#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
if [[ "${1:-}" == --no-build ]]; then
  shift
else
  rtk cargo build -p soe-godot-host --release
  rtk cargo build -p soe-bench --release
fi
exec rtk proxy python3 tools/frame_budget.py "$@"
