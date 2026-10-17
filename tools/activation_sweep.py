#!/usr/bin/env python3
"""Activation evidence runner, using CK contracts and installed walkthrough cells."""

import argparse
import collections
import contextlib
import fcntl
import json
import os
import re
from pathlib import Path
import subprocess
import signal
import sys
import time

ROOT = Path(__file__).resolve().parents[1]


@contextlib.contextmanager
def installed_slot():
    default = Path("/run/media/pengolodh/linux-data-games/tmp/soe-run/locks/installed-0.lock")
    path = os.environ.get("SOE_INSTALLED_LOCK", str(default) if default.exists() else "")
    if not path:
        yield ()
        return
    with open(path, "a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        yield (lock.fileno(),)


def summary(report):
    types = collections.defaultdict(collections.Counter)
    causes = collections.Counter()
    for cell in report["cells"]:
        for row in cell["references"]:
            types[row["base_type"]][row["status"]] += 1
            if row["status"] != "pass":
                causes[row["cause"]] += 1
            sites = set()
            for message in row.get("logs", []):
                site = re.search(r"\(([\w.]+)(?: line| instruction)", message)
                if site:
                    sites.add("papyrus:" + site.group(1))
            causes.update(sites)
            if any(message.startswith("Headless animation ") for message in row.get("logs", [])) and row["cause"] != "headless_animation_error":
                causes["headless_animation_error"] += 1
            if row.get("pending_animation_waits", 0) and row["cause"] != "animation_wait_not_completed":
                causes["animation_wait_not_completed"] += 1
            for cause in row.get("visual", {}).get("causes", []):
                causes[cause] += 1
    return {"by_base_type": dict(types), "by_failure_cause": dict(causes)}


def compact_host(data):
    initial = data.get("initial", {})
    for index, row in enumerate(data.get("references", [])):
        before, after = row.get("before", {}), row.get("after", {})
        for field in ("papyrus_errors", "native_errors", "animation_requests"):
            if field + "_count" in before:
                continue
            prior, current = before.get(field, []), after.get(field, [])
            if index == 0:
                initial[field] = prior
            row[field] = current[len(prior):]
            before[field + "_count"] = len(prior)
            after[field + "_count"] = len(current)
            before.pop(field, None)
            after.pop(field, None)
    data["initial"] = initial
    return data


def read_host(path):
    try:
        data = json.loads(path.read_text())
        if not isinstance(data, dict) or not isinstance(data.get("complete"), bool) or not isinstance(data.get("references"), list):
            raise ValueError("invalid host report structure")
        for row in data["references"]:
            reference = row.get("reference") if isinstance(row, dict) else None
            if not isinstance(reference, dict) or not isinstance(reference.get("id"), str) or not isinstance(row.get("before"), dict) or not isinstance(row.get("after"), dict):
                raise ValueError("invalid reference observation")
        return compact_host(data), None
    except (OSError, ValueError) as error:
        return {}, str(error)


def write_report(path, report):
    for cell in report["cells"]:
        for row in cell["references"]:
            row["logs"] = [message for message in row.get("logs", []) if any(
                word in message.lower() for word in ("error", "cannot", "unsupported", "missing", "unknown", "headless animation", "has no function"))]
    report["counts"] = summary(report)
    path.write_text(json.dumps(report, indent=2) + "\n")
    lines = ["# Activation sweep", "", report["oracle"], "",
             "A pass covers the stated observation. Unknown script effects and missing host runs fail.", "",
             "| Base | pass | wrong outcome | no outcome | error |",
             "| --- | ---: | ---: | ---: | ---: |"]
    for kind, counts in sorted(report["counts"]["by_base_type"].items()):
        lines.append("| " + kind + " | " + " | ".join(str(counts.get(s, 0)) for s in
                     ["pass", "wrong outcome", "no outcome", "error"]) + " |")
    lines += ["", "| Failure cause | References |", "| --- | ---: |"]
    for cause, count in sorted(report["counts"]["by_failure_cause"].items()):
        lines.append(f"| {cause} | {count} |")
    lines += ["", f"Cells: {len(report['cells'])}. Host complete: "
              f"{sum(c.get('host', {}).get('complete', False) for c in report['cells'])}.", "",
              "Fresh world per cell, sequential activation within each cell, three simulated seconds per reference.",
              "The host uses the walkthrough activation entry point. Target selection rays and full vanilla script effects need separate oracles.",
              "Headless transforms establish RID motion. They do not establish correct pixels or behavior graph event timing.", ""]
    if report.get("measurement_revision"):
        lines += [report["measurement_revision"] + ".", ""]
    path.with_suffix(".md").write_text("\n".join(lines))


def model_key(path):
    return "/".join(part for part in path.replace("\\", "/").lower().split("/") if part).removeprefix("meshes/")


def merge_host(cell, data, status, log_text, seconds, report_error=None):
    behavior_errors = [line for line in log_text.splitlines() if "host::object_behavior" in line and "ERROR" in line]
    expected = {row["reference"]["id"] for row in cell["references"]}
    actual = {row["reference"]["id"] for row in data.get("references", [])}
    complete = status == 0 and data.get("complete", False) and "ACTIVATION_SWEEP complete" in log_text and "SCRIPT ERROR" not in log_text and expected == actual and len(data.get("references", [])) == len(cell["references"]) and data.get("cell", {}).get("id") == cell["cell"]["id"]
    cell["host"] = {"exit_status": status, "complete": complete, "required": True,
                    "seconds": seconds, "initial": data.get("initial", {}),
                    "report_error": report_error, "behavior_errors": behavior_errors}
    observed = {row["reference"]["id"]: row for row in data.get("references", [])}
    for row in cell["references"]:
        row.setdefault("headless_status", row["status"])
        row.setdefault("headless_cause", row["cause"])
        row["status"] = row["headless_status"]
        row["cause"] = row["headless_cause"]
        if any(message.startswith("Headless animation ") for message in row.get("logs", [])):
            row["status"], row["cause"] = "error", "headless_animation_error"
        row["visual"] = observed.get(row["reference"]["id"], {"status": "error", "causes": ["host_run_incomplete"]})
        causes = row["visual"].setdefault("causes", [])
        causes[:] = [cause for cause in causes if cause != "host_run_incomplete"]
        for error in behavior_errors:
            model = re.search(r'model=("(?:\\.|[^"\\])*")', error)
            if model and model_key(json.loads(model.group(1))) == model_key(row.get("model") or ""):
                if "host_behavior_binding_error" not in causes:
                    causes.append("host_behavior_binding_error")
                row["visual"].setdefault("behavior_errors", []).append(error)
        if not complete:
            causes.append("host_run_incomplete")
        if causes:
            row["visual"]["status"] = "fail"
        if causes and row["status"] == "pass":
            row["status"] = "error" if any("error" in cause or cause == "host_run_incomplete" for cause in causes) else "wrong outcome"
            row["cause"] = causes[0]


def host_pass(report, args, work):
    for index, cell in enumerate(report["cells"]):
        if not cell["references"]:
            cell["host"] = {"required": False, "complete": False, "status": "no eligible references", "exit_status": None, "seconds": 0}
            write_report(args.report, report)
            continue
        source = work / f"cell-{index}.json"
        output = work / f"host-{index}.json"
        log = work / f"host-{index}.log"
        source.write_text(json.dumps(cell))
        env = dict(os.environ, SOE_ACTIVATION_CELL=str(source),
                   SOE_ACTIVATION_OUTPUT=str(output), SOE_THREADED_SIMULATION="0",
                   DISPLAY=":99", SOE_GODOT_TIMEOUT=str(args.cell_timeout))
        command = [args.godot, "--headless", "--path", str(args.project),
                   "--script", "activation_sweep.gd"]
        print(f"activation host: {index + 1}/{len(report['cells'])} {cell['cell']}", flush=True)
        with installed_slot() as descriptors, log.open("w") as stream:
            started = time.monotonic()
            child = subprocess.Popen(command, cwd=ROOT, env=env, stdout=stream, stderr=stream,
                                     pass_fds=descriptors, start_new_session=True)
            try:
                status = child.wait(timeout=args.cell_timeout + 120)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait()
                status = child.returncode
        data, report_error = read_host(output)
        log_text = log.read_text(errors="replace")
        merge_host(cell, data, status, log_text, round(time.monotonic() - started, 3), report_error)
        write_report(args.report, report)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=ROOT / "docs/activation-sweep.json")
    parser.add_argument("--godot", default=os.environ.get("SOE_GODOT_BIN", "godot"))
    parser.add_argument("--project", type=Path, default=ROOT / "godot")
    parser.add_argument("--game-bin", type=Path, default=ROOT / "target/release/soe-activation-sweep")
    parser.add_argument("--cell")
    parser.add_argument("--all-cells", action="store_true")
    parser.add_argument("--headless-only", action="store_true")
    parser.add_argument("--reuse", action="store_true", help="reuse the game's existing JSON report")
    parser.add_argument("--cell-timeout", type=int, default=300)
    args = parser.parse_args()
    if not os.environ.get("SKYRIM_DATA"):
        parser.error("SKYRIM_DATA is required")
    args.report = args.report.resolve()
    work = ROOT / "target/activation-sweep" / args.report.stem
    work.mkdir(parents=True, exist_ok=True)
    if not args.reuse:
        if not args.game_bin.is_file():
            parser.error("prepare the release soe-activation-sweep binary before running the survey")
        command = [str(args.game_bin.resolve()), str(args.report)]
        if args.all_cells:
            command.append("--all-cells")
        elif args.cell:
            command.append(args.cell)
        with installed_slot() as descriptors:
            status = subprocess.run(command, cwd=ROOT, check=False, pass_fds=descriptors).returncode
        if status not in (0, 1):
            return status
    report = json.loads(args.report.read_text())
    if not args.headless_only:
        host_pass(report, args, work)
    write_report(args.report, report)
    return int(not report.get("complete", False) or not report["cells"] or any((bool(cell["references"]) and not cell.get("host", {}).get("complete", False))
               or any(row["status"] != "pass" for row in cell["references"]) for cell in report["cells"]))


if __name__ == "__main__":
    try:
        exit_status = main()
    except (OSError, ValueError, RuntimeError) as error:
        print(f"activation sweep setup: {error}", file=sys.stderr)
        exit_status = 2
    raise SystemExit(exit_status)
