#!/usr/bin/env python3
"""Fixed PRF-16 CPU scenarios, sourced from PRF-12 and godotwind's manual.

The final sweep consumes follow-up-rows.csv, importing its rows into the
backlog. This runner writes the proposal without modifying BACKLOG.csv.
"""

import argparse
import csv
import datetime
import json
import math
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SCENARIOS = ("bannered-mare", "walk-out", "thousand-actors")
SYSTEMS = (
    "papyrus_vm", "ai_planning", "ai_movement", "combat", "magic",
    "quests_story_manager", "dialogue", "physics_sync", "cell_streaming",
    "audio", "save", "input", "host_render_submission", "other",
)
METRICS = ("p50_ms", "p99_ms", "worst_ms")


def number(value, name):
    if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
        raise ValueError(f"invalid {name}")
    return value


def budgets(path):
    text = path.read_text().split("<!-- PRF-16 budgets -->", 1)[1].split("<!-- /PRF-16 budgets -->", 1)[0]
    result = {}
    for name, p50, p99, worst in re.findall(
        r"^\| ([a-z_]+) \| ([\d.]+) \| ([\d.]+) \| ([\d.]+) \|$", text, re.M
    ):
        if name in result:
            raise ValueError(f"duplicate budget {name}")
        values = [number(float(raw), name) for raw in (p50, p99, worst)]
        if not 0 < values[0] <= values[1] <= values[2]:
            raise ValueError(f"unordered budget {name}")
        result[name] = dict(zip(METRICS, values))
    if set(result) != {"frame", *SYSTEMS}:
        raise ValueError("incomplete PRF-16 budget table")
    return result


def assess(row, scenario, limits):
    if row.get("scenario") != scenario:
        raise ValueError("scenario identity mismatch")
    if type(row.get("samples")) is not int or not 3600 <= row["samples"] <= 4201:
        raise ValueError("scenario needs at least 3600 bounded samples")
    if type(row.get("game_ticks")) is not int or row["game_ticks"] != row["samples"]:
        raise ValueError("scenario needs exactly one Game tick per measured sample")
    for name in ("elapsed_s", "simulated_s"):
        if number(row.get(name), name) < 60:
            raise ValueError(f"scenario needs 60 {name}")
    if type(row.get("actors")) is not int or row["actors"] < 5:
        raise ValueError("NPC workload missing")
    if type(row.get("evaluations")) is not int or row["evaluations"] <= 0 or row.get("navmesh") is not True:
        raise ValueError("AI packages or navigation missing")
    if scenario == "thousand-actors" and (row["actors"] != 1000 or row["evaluations"] < 1000):
        raise ValueError("thousand-actor workload missing")
    if scenario == "walk-out" and (row.get("walked") is not True or not row.get("cell")
                                   or type(row.get("exterior_actors")) is not int or row["exterior_actors"] < 5
                                   or row.get("world_editor_id") != "WhiterunWorld"):
        raise ValueError("scripted exterior walk missing")
    if not isinstance(row.get("systems"), dict) or set(row["systems"]) != set(SYSTEMS):
        raise ValueError("incomplete per-system report")
    reasons = []
    for name, sample in {"frame": row.get("frame"), **row["systems"]}.items():
        if not isinstance(sample, dict) or set(sample) != set(METRICS):
            raise ValueError(f"incomplete {name} timings")
        values = [number(sample[metric], f"{name}.{metric}") for metric in METRICS]
        if values != sorted(values):
            raise ValueError(f"unordered {name} percentiles")
        for metric in METRICS:
            if sample[metric] > limits[name][metric]:
                reasons.append(f"{name}.{metric}={sample[metric]:.6f} exceeds {limits[name][metric]:.6f}")
    for active in ("ai_planning", "ai_movement"):
        if row["systems"][active]["worst_ms"] <= 0:
            raise ValueError(f"inactive {active}")
    return reasons


def parse_output(output, scenario, limits, returncode):
    if returncode != 0 or re.search(r"(?:SCRIPT ERROR|Parse Error|PRF-16 ERROR|ERROR:)", output):
        raise ValueError(f"scenario process failed with exit {returncode}")
    samples = [line.removeprefix("SOE_BUDGET ") for line in output.splitlines() if line.startswith("SOE_BUDGET ")]
    if len(samples) != 1:
        raise ValueError("expected exactly one complete scenario report")
    row = json.loads(samples[0])
    if not isinstance(row, dict):
        raise ValueError("scenario report is not an object")
    return row, assess(row, scenario, limits)


def followup_rows(backlog, results):
    with backlog.open(newline="") as source:
        reader = csv.DictReader(source)
        fields = reader.fieldnames
        existing = list(reader)
    next_id = 1 + max(int(row["id"].split("-")[1]) for row in existing if row["id"].startswith("PRF-"))
    rows = []
    for result in results:
        if result["status"] == "pass":
            continue
        row = dict.fromkeys(fields, "-")
        row.update(
            id=f"PRF-{next_id:02d}", area="PRF", title=f"Fix {result['scenario']} frame budget {result['status']}",
            status="open", branch=f"prf-budget-{next_id:02d}", depends_on="PRF-16",
            crates="game,godot-host", read_first="docs/performance.md,docs/game.md,docs/godot-host.md,tools/frame_budget.py",
            done_check=f"tools/perf-budget.sh passes {result['scenario']} with the fixed PRF-16 budgets",
            evidence="-", updated=datetime.date.today().isoformat(),
        )
        if "difficulty" in row:
            row["difficulty"] = "medium"
        rows.append(row)
        next_id += 1
    return fields, rows


def execute(command, env, log, timeout=600):
    try:
        with log.open("w") as target:
            result = subprocess.run(command, cwd=ROOT, env=env, text=True, stdout=target,
                                    stderr=subprocess.STDOUT, timeout=timeout, check=False)
        return result.returncode, log.read_text(errors="replace")
    except subprocess.TimeoutExpired:
        with log.open("a") as target:
            target.write("\nPRF-16 ERROR: process timed out and was killed\n")
        return 1, log.read_text()


def run_suite(godot, binary, env, out, sweep, execute_process=execute):
    out.mkdir(parents=True, exist_ok=True)
    env = {**env, "XDG_DATA_HOME": str(out / "user")}
    limits = budgets(ROOT / "docs/performance.md")
    import_command = [str(godot), "--headless", "--path", "godot", "--import"]
    try:
        code, output = execute_process(import_command, env, out / "import.log")
    except OSError as error:
        code, output = 1, str(error)
        (out / "import.log").write_text(output)
    import_failed = code != 0 or "ERROR:" in output
    results = []
    for scenario in SCENARIOS:
        command = ([str(binary), "frame-budget"] if scenario == "thousand-actors" else
                   [str(godot), "--headless", "--path", "godot", "--script",
                    "prf_16_walkthrough_test.gd", "--", scenario])
        result = {"scenario": scenario, "command": command, "log": f"{scenario}.log"}
        try:
            if import_failed and scenario != "thousand-actors":
                raise ValueError("extension import failed, see import.log")
            code, output = execute_process(command, env, out / result["log"])
            row, reasons = parse_output(output, scenario, limits, code)
            result.update(sample=row, status="breach" if reasons else "pass", reasons=reasons)
        except (ValueError, OSError) as error:
            result.update(status="error", reasons=[str(error)])
        results.append(result)
        print(f"PRF-16 {scenario}: {result['status']}: {', '.join(result['reasons']) or 'within budgets'}", flush=True)
    fields, rows = followup_rows(ROOT / "BACKLOG.csv", results)
    with (out / "follow-up-rows.csv").open("w", newline="") as target:
        writer = csv.DictWriter(target, fieldnames=fields, quoting=csv.QUOTE_ALL, lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
    report = {"row": "PRF-16", "budgets": limits, "results": results,
              "followup_rows": rows, "sweep": sweep, "data": env.get("SKYRIM_DATA")}
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    if sweep:
        return 0
    if any(result["status"] == "error" for result in results):
        return 1
    return 2 if rows else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sweep", action="store_true", help="record follow-up rows and return zero after all scenarios")
    parser.add_argument("--out", type=Path, default=ROOT / "target/perf-budget")
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "release/soe-bench")
    parser.add_argument("--godot", type=Path, default=Path(os.environ.get("SOE_GODOT_BIN", "godot")))
    args = parser.parse_args()
    env = dict(os.environ)
    if not env.get("SKYRIM_DATA"):
        parser.error("SKYRIM_DATA is required for the fixed installed-data suite")
    for master in ("Skyrim.esm", "Update.esm", "Dawnguard.esm", "HearthFires.esm", "Dragonborn.esm"):
        if not (Path(env["SKYRIM_DATA"]) / master).is_file():
            parser.error(f"installed data missing {master}")
    return run_suite(args.godot, args.binary.resolve(), env, args.out.resolve(), args.sweep)


if __name__ == "__main__":
    raise SystemExit(main())
