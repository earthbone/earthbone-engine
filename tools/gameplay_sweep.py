#!/usr/bin/env python3
"""Run the consumable, magic and combat sweeps and write their reports.

The headless pass is the soe-gameplay-sweep binary. The installed Godot pass
writes <kind>-host.json under --host-dir with {"checks": [{"check": name,
"status": "pass" | "fail", "reason": text}]}. A visual check without a passing
host entry stays "not run", so a missing host pass fails the sweep.
"""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
KINDS = {
    "consumables": "consumable-sweep",
    "magic": "magic-sweep",
    "combat": "combat-sweep",
}


def merge_host(report, host_path):
    """Apply the Godot pass results to the visual checks of one report."""
    if not host_path.is_file():
        return
    try:
        checks = {c["check"]: c for c in json.loads(host_path.read_text())["checks"]}
    except (OSError, ValueError, KeyError, TypeError) as error:
        for item in report["visual"]:
            item["cause"] = f"host_report_unreadable: {error}"
        return
    for item in report["visual"]:
        seen = checks.get(item["check"])
        if seen is None:
            continue
        item["status"] = "pass" if seen.get("status") == "pass" else "fail"
        item["cause"] = "" if item["status"] == "pass" else seen.get("reason", "host_check_failed")


def visual_counts(report):
    causes = {}
    for item in report["visual"]:
        if item["status"] != "pass":
            causes[item["cause"]] = causes.get(item["cause"], 0) + 1
    return causes


def write_summary(report, path):
    lines = path.read_text().splitlines() if path.is_file() else []
    lines += ["", "| Visual cause | Checks |", "| --- | ---: |"]
    for cause, count in sorted(visual_counts(report).items()):
        lines.append(f"| {cause} | {count} |")
    path.write_text("\n".join(lines) + "\n")


def run_kind(kind, args):
    stem = KINDS[kind]
    json_path = args.out / f"{stem}.json"
    command = [str(args.game_bin), kind, str(json_path)]
    if args.only:
        command += ["--only", args.only]
    if args.limit:
        command += ["--limit", str(args.limit)]
    print(f"gameplay sweep: {kind}", flush=True)
    status = subprocess.run(command, cwd=ROOT).returncode
    if status == 2 or not json_path.is_file():
        print(f"gameplay sweep setup failed for {kind}", file=sys.stderr)
        return 2
    report = json.loads(json_path.read_text())
    merge_host(report, args.host_dir / f"{kind}-host.json")
    json_path.write_text(json.dumps(report, indent=2) + "\n")
    write_summary(report, json_path.with_suffix(".md"))
    rows_ok = all(r["status"] == "pass" for r in report["rows"])
    visual_ok = all(v["status"] == "pass" for v in report["visual"])
    return 0 if rows_ok and visual_ok else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kinds", nargs="*", choices=list(KINDS), default=list(KINDS))
    parser.add_argument("--out", type=Path, default=ROOT / "docs")
    parser.add_argument("--host-dir", type=Path, default=ROOT / "target/gameplay-sweep")
    parser.add_argument("--game-bin", type=Path, default=ROOT / "target/release/soe-gameplay-sweep")
    parser.add_argument("--only", help="editor id prefix")
    parser.add_argument("--limit", type=int)
    args = parser.parse_args()
    if not os.environ.get("SKYRIM_DATA"):
        parser.error("SKYRIM_DATA is required")
    if not args.game_bin.is_file():
        parser.error("build the release soe-gameplay-sweep binary first")
    args.out.mkdir(parents=True, exist_ok=True)
    worst = 0
    for kind in args.kinds or list(KINDS):
        worst = max(worst, run_kind(kind, args))
    return worst


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"gameplay sweep setup: {error}", file=sys.stderr)
        raise SystemExit(2)
