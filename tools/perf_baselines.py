#!/usr/bin/env python3
"""Parse Divan medians and compare them with a machine-specific baseline."""

import argparse
import json
import os
import platform
import re
import subprocess
import sys
from pathlib import Path


OPERATIONS = {
    "form_lookup",
    "winning_override_resolve",
    "navmesh_locate",
    "navmesh_find_path",
    "papyrus_call",
    "papyrus_native_call",
    "nif_parse",
    "dds_bc7_decode",
    "bsa_file_read",
}
INSTALLED_OPERATIONS = {"installed_nif_parse", "installed_bsa_file_read"}
BATCH_COUNTS = {
    "form_lookup": 1024,
    "winning_override_resolve": 1024,
    "navmesh_locate": 1024,
    "navmesh_find_path": 128,
    "papyrus_call": 1,
    "papyrus_native_call": 1,
    "nif_parse": 128,
    "dds_bc7_decode": 1,
    "bsa_file_read": 128,
    "installed_nif_parse": 1,
    "installed_bsa_file_read": 1,
}
ALL_OPERATIONS = OPERATIONS | INSTALLED_OPERATIONS
SCALE_NS = {"ns": 1, "µs": 1_000, "us": 1_000, "ms": 1_000_000, "s": 1_000_000_000}
MEDIAN = re.compile(r"^\s*([0-9]+(?:\.[0-9]+)?)\s*(ns|µs|us|ms|s)\s*$")


def machine_identity():
    model = platform.processor() or platform.machine()
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.lower().startswith("model name"):
                model = line.split(":", 1)[1].strip()
                break
    except OSError:
        pass
    return {"architecture": platform.machine(), "cpu": model, "host": platform.node()}


def parse_divan(text):
    found = {}
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped.startswith(("├─", "╰─")) or "│" not in stripped:
            continue
        name = next((op for op in ALL_OPERATIONS if stripped.startswith((f"├─ {op} ", f"╰─ {op} "))), "")
        if name not in ALL_OPERATIONS:
            continue
        columns = [part.strip() for part in stripped.split("│")]
        if len(columns) < 3:
            continue
        match = MEDIAN.match(columns[2])
        if match:
            batch_ns = float(match.group(1)) * SCALE_NS[match.group(2)]
            found[name] = round(batch_ns / BATCH_COUNTS[name])
    return found


def evaluate(current, baseline, machine, include_installed=False, threshold=1.15):
    if baseline.get("machine") != machine:
        return ["baseline machine does not match this host"]
    stored = baseline.get("operations", {})
    errors = []
    required = OPERATIONS | (INSTALLED_OPERATIONS if include_installed else set())
    for name in sorted(required):
        if name not in current:
            errors.append(f"missing current median for {name}")
        elif name not in stored or stored[name] <= 0:
            errors.append(f"missing positive baseline for {name}")
        elif current[name] > stored[name] * threshold:
            increase = (current[name] / stored[name] - 1) * 100
            errors.append(f"{name} is {increase:.1f}% slower than baseline")
    return errors


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("check", "refresh"))
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--baseline", required=True, type=Path)
    args = parser.parse_args()
    try:
        current = parse_divan(args.output.read_text())
        include_installed = bool(os.environ.get("SKYRIM_DATA"))
        required = OPERATIONS | (INSTALLED_OPERATIONS if include_installed else set())
        if not required.issubset(current):
            missing = ", ".join(sorted(required - current.keys()))
            raise ValueError(f"Divan output is missing medians for: {missing}")
        current = {name: value for name, value in current.items() if name in required}
        machine = machine_identity()
        if args.mode == "refresh":
            data = {
                "format": 1,
                "machine": machine,
                "data_mode": "installed" if include_installed else "synthetic",
                "operations": current,
            }
            args.baseline.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
            print(f"stored {len(current)} operation medians in {args.baseline}")
            return 0
        baseline = json.loads(args.baseline.read_text())
        errors = evaluate(current, baseline, machine, include_installed)
        if errors:
            print("\n".join(errors), file=sys.stderr)
            return 1
        print(f"all {len(current)} medians are within 15% of the same-machine baseline")
        return 0
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(error, file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
