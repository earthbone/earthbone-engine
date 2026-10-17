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
            if row.get("pending_animation_waits", 0) and row["cause"] != "animation_wait_not_completed":
                causes["animation_wait_not_completed"] += 1
            for cause in row.get("visual", {}).get("causes", []):
                causes[cause] += 1
    return {"by_base_type": dict(types), "by_failure_cause": dict(causes)}


def compact_host(data):
    initial = {}
    for index, row in enumerate(data.get("references", [])):
        before, after = row.get("before", {}), row.get("after", {})
        for field in ("papyrus_errors", "native_errors", "animation_requests"):
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


def write_report(path, report):
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


def host_pass(report, args, work):
    for index, cell in enumerate(report["cells"]):
        source = work / f"cell-{index}.json"
        output = work / f"host-{index}.json"
        log = work / f"host-{index}.log"
        source.write_text(json.dumps(cell))
        env = dict(os.environ, SOE_ACTIVATION_CELL=str(source),
                   SOE_ACTIVATION_OUTPUT=str(output), SOE_THREADED_SIMULATION="0",
                   DISPLAY=":99", SOE_GODOT_TIMEOUT=str(args.cell_timeout))
        command = [args.godot, "--headless", "--path", str(ROOT / "godot"),
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
        data = compact_host(json.loads(output.read_text())) if output.exists() else {}
        log_text = log.read_text(errors="replace")
        complete = status == 0 and data.get("complete", False) and "ACTIVATION_SWEEP complete" in log_text and "SCRIPT ERROR" not in log_text
        cell["host"] = {"exit_status": status, "complete": complete,
                        "seconds": round(time.monotonic() - started, 3), "initial": data.get("initial", {})}
        observed = {r["reference"]["id"]: r for r in data.get("references", [])}
        for row in cell["references"]:
            row.setdefault("headless_status", row["status"])
            row.setdefault("headless_cause", row["cause"])
            row["status"] = row["headless_status"]
            row["cause"] = row["headless_cause"]
            row["visual"] = observed.get(row["reference"]["id"],
                                         {"status": "error", "causes": ["host_run_incomplete"]})
            if not complete:
                row["visual"].setdefault("causes", []).append("host_run_incomplete")
            if row["visual"].get("causes") and row["status"] == "pass":
                row["status"] = "wrong outcome"
                row["cause"] = row["visual"]["causes"][0]
        write_report(args.report, report)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=ROOT / "docs/activation-sweep.json")
    parser.add_argument("--godot", default=os.environ.get("SOE_GODOT_BIN", "godot"))
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
    return int(not report.get("complete", False) or not report["cells"] or any(not cell.get("host", {}).get("complete", False)
               or any(row["status"] != "pass" for row in cell["references"]) for cell in report["cells"]))


if __name__ == "__main__":
    raise SystemExit(main())
