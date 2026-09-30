#!/usr/bin/env python3
"""Isolated capacity sweeps and sustained workload checks, using only the stdlib.

Protocol and sources: docs/performance-limits.md. Child timings exclude this
process's polling and JSON reporting. RSS is sampled separately, including setup.
"""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shlex
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
CPU = ("scripts", "actors", "navigation", "streaming", "mixed")
GPU = ("exterior", "geometry", "textures")


def positive(value):
    number = float(value)
    if not math.isfinite(number) or number <= 0:
        raise argparse.ArgumentTypeError("must be finite and positive")
    return number


def integer(value):
    number = int(value)
    if number < 1:
        raise argparse.ArgumentTypeError("must be positive")
    return number


def parser():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("mode", choices=("check", "sweep", "soak"))
    cli.add_argument("--backend", choices=("cpu", "godot"), default="cpu")
    cli.add_argument("--scenario", default="mixed")
    cli.add_argument("--binary", type=Path, default=ROOT / "target/release/soe-bench")
    cli.add_argument("--godot", default="godot")
    cli.add_argument("--start", type=integer, default=1, help="load for check/soak, first sweep rung")
    cli.add_argument("--max-load", type=integer, default=1024)
    cli.add_argument("--repeats", type=integer, default=3)
    cli.add_argument("--refine", type=int, default=4, help="bisection rounds after a confirmed breach")
    cli.add_argument("--samples", type=integer, default=120)
    cli.add_argument("--warmup", type=integer, default=10)
    cli.add_argument("--seed", type=integer, default=1)
    cli.add_argument("--vm-budget", type=integer, default=100000)
    cli.add_argument("--p99-ms", type=positive, default=16.7)
    cli.add_argument("--worst-ms", type=positive, default=33.0)
    cli.add_argument("--max-rss-mib", type=positive, default=2048.0)
    cli.add_argument("--max-video-mib", type=positive, default=4096.0)
    cli.add_argument("--max-growth-mib", type=positive, default=64.0)
    cli.add_argument("--max-backlog", type=int, default=0)
    cli.add_argument("--max-allocations", type=integer, default=100000,
                     help="maximum allocation calls in any measured CPU tick")
    cli.add_argument("--timeout", type=positive, default=120.0, help="per-process watchdog, excluding requested soak duration")
    cli.add_argument("--duration", type=integer, default=600, help="soak wall seconds, at most 86400")
    cli.add_argument("--out", type=Path, required=True, help="new output directory, never overwritten")
    return cli


def read_samples(path):
    samples = []
    for line in path.read_text(errors="replace").splitlines():
        if line.startswith(("SCRIPT ERROR:", "ERROR:")):
            raise ValueError(f"child reported {line}")
        if line.startswith("SOE_SAMPLE "):
            row = json.loads(line.removeprefix("SOE_SAMPLE "))
            for name in ("samples", "p50_ms", "p99_ms", "p999_ms", "worst_ms"):
                value = row.get(name)
                if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
                    raise ValueError(f"missing or invalid {name}")
            for name in ("backlog_peak", "rss_mib", "video_mib", "peak_allocations",
                         "peak_allocation_bytes"):
                value = row.get(name)
                if value is not None and (isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0):
                    raise ValueError(f"invalid {name}")
            if row["samples"] < 1:
                raise ValueError("empty sample window")
            if not row["p50_ms"] <= row["p99_ms"] <= row["p999_ms"] <= row["worst_ms"]:
                raise ValueError("inconsistent percentile order")
            samples.append(row)
    return samples


def assess(samples, args):
    reasons = []
    if not samples:
        return ["no measurements"], True
    for metric, limit in (("p99_ms", args.p99_ms), ("worst_ms", args.worst_ms), ("backlog_peak", args.max_backlog), ("video_mib", args.max_video_mib)):
        values = [row[metric] for row in samples if metric in row]
        if metric == "backlog_peak" and args.backend == "cpu" and len(values) != len(samples):
            return ["missing backlog telemetry"], True
        if values and max(values) > limit:
            reasons.append(f"{metric}={max(values):.6f} exceeds {limit}")
    allocation_peaks = [row["peak_allocations"] for row in samples if "peak_allocations" in row]
    allocation_bytes = [row["peak_allocation_bytes"] for row in samples if "peak_allocation_bytes" in row]
    if args.backend == "cpu" and len(allocation_peaks) != len(samples):
        return ["missing allocation telemetry"], True
    if args.backend == "cpu" and len(allocation_bytes) != len(samples):
        return ["missing allocation byte telemetry"], True
    if allocation_peaks and max(allocation_peaks) > args.max_allocations:
        reasons.append(f"peak_allocations={max(allocation_peaks)} exceeds {args.max_allocations}")
    if args.backend == "godot" and any(row.get("draw_calls", 0) <= 0 or row.get("primitives", 0) <= 0 for row in samples):
        return ["scene produced no rendered work"], True
    if args.mode == "soak":
        rss = [row.get("rss_mib") for row in samples]
        if len(rss) < 2 or any(value is None or not math.isfinite(value) for value in rss):
            if not reasons:
                return ["soak needs at least two windows with RSS observations"], True
        else:
            growth = max(rss) - rss[0]
            if growth > args.max_growth_mib:
                reasons.append(f"RSS growth {growth:.3f} MiB exceeds {args.max_growth_mib}")
        if samples[-1].get("elapsed_s", 0) < args.duration and not reasons:
            return ["soak ended before requested duration"], True
    return reasons, False


def command(args, load):
    seconds = args.duration if args.mode == "soak" else 0
    if args.backend == "cpu":
        return [str(args.binary.resolve()), "stress", args.scenario, "--load", str(load),
                "--samples", str(args.samples), "--warmup", str(args.warmup),
                "--seed", str(args.seed), "--seconds", str(seconds), "--vm-budget", str(args.vm_budget)], {}
    env = {
        "SOE_BENCH_SCENARIO": args.scenario, "SOE_BENCH_LOAD": str(load),
        "SOE_BENCH_P99_MS": str(args.p99_ms), "SOE_BENCH_WORST_MS": str(args.worst_ms),
        "SOE_BENCH_SECONDS": str(max(5, seconds)), "SOE_BENCH_SOAK": "1" if seconds else "0",
        "SOE_BENCH_SEED": str(args.seed),
    }
    if args.scenario == "exterior":
        env.update(SOE_BENCH_CELL="WhiterunOrigin", SOE_BENCH_RADIUS=str(load), SKYRIM_DATA=os.environ["SKYRIM_DATA"])
    script = "bench.gd" if args.scenario == "exterior" else "stress.gd"
    return [args.godot, "--path", str(ROOT / "godot"), "--script", script], env


def process_rss(pid):
    try:
        status = Path(f"/proc/{pid}/status").read_text()
        return next(float(line.split()[1]) / 1024 for line in status.splitlines() if line.startswith("VmRSS:"))
    except (OSError, StopIteration):
        return None


def stop(process):
    if process.poll() is None:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    process.wait()


def run_once(args, load, index):
    argv, overrides = command(args, load)
    log = args.out / f"run-{index:04d}-load-{load}.log"
    result = {"load": load, "command": argv, "environment": overrides, "log": log.name}
    reproduction = " ".join(f"{key}={shlex.quote(value)}" for key, value in overrides.items())
    result["reproduce_child"] = f"{reproduction} {shlex.join(argv)}".strip()
    peak = 0.0
    observations = 0
    window_rss = []
    forced = None
    started = time.monotonic()
    env = os.environ.copy()
    # Parent benchmark knobs must not silently change a published workload.
    env = {key: value for key, value in env.items() if not key.startswith("SOE_BENCH_")}
    env.update(overrides)
    try:
        with log.open("w") as output:
            process = subprocess.Popen(argv, cwd=ROOT, env=env, stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
            reader = log.open()
            pending = ""
            try:
                while process.poll() is None:
                    rss = process_rss(process.pid)
                    if rss is not None:
                        observations += 1
                        peak = max(peak, rss)
                    pending += reader.read()
                    while "\n" in pending:
                        line, pending = pending.split("\n", 1)
                        if line.startswith("SOE_SAMPLE "):
                            window_rss.append(rss)
                    if peak > args.max_rss_mib:
                        forced = "memory_ceiling"
                    elif time.monotonic() - started > args.timeout + (args.duration if args.mode == "soak" else 0):
                        forced = "timeout"
                    if forced:
                        stop(process)
                        break
                    time.sleep(0.05)
            finally:
                stop(process)
                reader.close()
        result.update(exit_code=process.returncode, wall_s=time.monotonic() - started, sampled_peak_rss_mib=peak if observations else None)
        samples = read_samples(log)
        if samples and len(window_rss) < len(samples):
            window_rss.extend([rss if observations else None] * (len(samples) - len(window_rss)))
        for sample_index, row in enumerate(samples):
            if row.get("rss_mib") is None and sample_index < len(window_rss):
                row["rss_mib"] = window_rss[sample_index]
                row["rss_source"] = "parent sampled /proc on window arrival"
        result["samples"] = samples
        rss_values = [row["rss_mib"] for row in samples if row.get("rss_mib") is not None]
        if rss_values:
            peak = max(peak, max(rss_values))
        if forced:
            result.update(status="breach" if forced == "memory_ceiling" else "error", reasons=[forced])
        elif process.returncode not in (0, 2):
            result.update(status="error", reasons=[f"child exited {process.returncode}"])
        else:
            reasons, invalid = assess(samples, args)
            if not observations and not rss_values:
                reasons.append("process RSS was not measured")
                invalid = True
            if peak > args.max_rss_mib:
                reasons.append(f"RSS {peak:.3f} MiB exceeds {args.max_rss_mib}")
            if process.returncode == 2 and not reasons:
                reasons.append("child performance gate failed, see log")
            result.update(status="error" if invalid else "breach" if reasons else "pass", reasons=reasons)
    except (OSError, ValueError, KeyError, TypeError) as error:
        result.update(status="error", reasons=[str(error)])
    print(f"load={load} run={index} {result['status']}: {', '.join(result['reasons']) or 'within budgets'}", flush=True)
    return result


def classify(runs):
    statuses = {run["status"] for run in runs}
    if "error" in statuses:
        return "error"
    return statuses.pop() if len(statuses) == 1 else "unstable"


def sweep(measure, start, maximum, refine):
    """Only unanimous repeats can bound a limit. Noise stops refinement."""
    passed = None
    load = start
    while True:
        status = measure(load)
        if status in ("error", "unstable"):
            return {"status": status, "last_pass": passed, "unresolved_load": load}
        if status == "breach":
            failed = load
            break
        passed = load
        if load == maximum:
            return {"status": "limit_not_found", "last_pass": passed, "tested_maximum": maximum}
        load = min(maximum, load * 2)
    if passed is not None:
        for _ in range(refine):
            if failed - passed <= 1:
                break
            middle = (failed + passed) // 2
            status = measure(middle)
            if status in ("error", "unstable"):
                return {"status": status, "last_pass": passed, "first_breach": failed, "unresolved_load": middle}
            if status == "pass":
                passed = middle
            else:
                failed = middle
    return {"status": "boundary_found", "last_pass": passed, "first_breach": failed}


def texture_capacity_sweep(measure, start, maximum, repeats, conditions):
    """Run every configured texture rung and report the confirmed prefix."""
    loads = []
    load = start
    while True:
        status = measure(load)
        loads.append({"load": load, "status": status})
        if load == maximum:
            break
        load = min(maximum, load * 2)

    capacity = 0
    first_nonpass = None
    for rung in loads:
        if rung["status"] != "pass":
            first_nonpass = rung
            break
        capacity = rung["load"]

    if first_nonpass is None:
        status = "limit_not_found"
    elif first_nonpass["status"] == "error":
        status = "error"
    elif first_nonpass["status"] == "unstable":
        status = "unstable"
    else:
        status = "boundary_found"

    return {
        "status": status,
        "capacity_textures": capacity,
        "capacity_definition": "highest contiguous rung where every repeat passed",
        "first_nonpass": first_nonpass,
        "completed_rungs": loads,
        "conditions": {**conditions, "repeats_per_rung": repeats},
    }


def write_report(path, report):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
    temporary.replace(path)


def main(argv=None):
    cli = parser()
    args = cli.parse_args(argv)
    if args.scenario not in (CPU if args.backend == "cpu" else GPU):
        cli.error("scenario does not belong to the selected backend")
    if (args.mode == "sweep" and args.start > args.max_load) or max(args.start, args.max_load, args.samples, args.warmup) > 100000 or args.refine < 0 or args.max_backlog < 0 or args.duration > 86400:
        cli.error("invalid load, refinement, backlog or duration range")
    if args.backend == "godot" and args.scenario == "exterior" and not os.environ.get("SKYRIM_DATA"):
        cli.error("exterior requires SKYRIM_DATA")
    args.out = args.out.resolve()
    try:
        args.out.mkdir(parents=True, exist_ok=False)
    except OSError as error:
        cli.error(str(error))
    metadata = {key: str(value) if isinstance(value, Path) else value for key, value in vars(args).items()}
    commit = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, capture_output=True, check=False).stdout.strip()
    dirty = subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, text=True, capture_output=True, check=False).stdout.strip()
    report = {"schema": 1, "arguments": metadata, "commit": commit, "dirty": bool(dirty),
              "machine": {"platform": platform.platform(), "cpu": platform.processor(), "logical_cpus": os.cpu_count()},
              "runner_command": [sys.executable, str(Path(__file__).resolve()), *(sys.argv[1:] if argv is None else argv)],
              "runs": [], "result": {"status": "incomplete"}}
    sources = [*sorted((ROOT / "crates/bench/src").rglob("*.rs")),
               *sorted((ROOT / "godot").glob("*bench*.gd")), ROOT / "godot/stress.gd", Path(__file__).resolve()]
    report["source_sha256"] = {str(source.relative_to(ROOT)): hashlib.sha256(source.read_bytes()).hexdigest() for source in sources}
    if args.backend == "cpu" and args.binary.is_file():
        report["binary_sha256"] = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    extension = ROOT / "target/release/libsoe_godot_host.so"
    if args.backend == "godot" and extension.is_file():
        report["extension_sha256"] = hashlib.sha256(extension.read_bytes()).hexdigest()
    path = args.out / "report.json"
    write_report(path, report)

    def measure(load):
        runs = []
        for _ in range(1 if args.mode == "soak" else args.repeats):
            run = run_once(args, load, len(report["runs"]))
            report["runs"].append(run)
            runs.append(run)
            write_report(path, report)
            if run["status"] == "error":
                break
        return classify(runs)

    if args.mode == "sweep" and args.backend == "godot" and args.scenario == "textures":
        report["result"] = texture_capacity_sweep(
            measure,
            args.start,
            args.max_load,
            args.repeats,
            {
                "backend": args.backend,
                "scenario": args.scenario,
                "texture_size": "512x512 RGBA8 with mipmaps",
                "resolution": "1600x900",
                "p99_ms": args.p99_ms,
                "worst_ms": args.worst_ms,
                "max_rss_mib": args.max_rss_mib,
                "max_video_mib": args.max_video_mib,
                "vsync": "disabled",
                "warmup_frames": 10,
                "minimum_samples": 600,
                "minimum_seconds": 5,
                "start": args.start,
                "max_load": args.max_load,
                "seed": args.seed,
            },
        )
    elif args.mode == "sweep":
        report["result"] = sweep(measure, args.start, args.max_load, args.refine)
    else:
        report["result"] = {"status": measure(args.start), "load": args.start}
    # Ratios are observations. They do not assume monotonic performance or
    # claim an asymptotic cliff from a noisy adjacent pair.
    costs = {}
    for run in report["runs"]:
        if run.get("samples"):
            costs.setdefault(run["load"], []).append(max(row["p99_ms"] for row in run["samples"]))
    report["scaling"] = []
    previous = None
    for load, values in sorted(costs.items()):
        value = sorted(values)[len(values) // 2]
        if previous and previous[1] > 0:
            ratio = (value / previous[1]) / (load / previous[0])
            report["scaling"].append({"from_load": previous[0], "to_load": load, "cost_per_unit_ratio": ratio, "nonlinear_candidate": ratio >= 2.0})
        previous = (load, value)
    write_report(path, report)
    print(json.dumps(report["result"], sort_keys=True))
    print(f"Report: {path}")
    status = report["result"]["status"]
    return 0 if status in ("pass", "limit_not_found") else 2 if status in ("breach", "boundary_found") else 1


if __name__ == "__main__":
    sys.exit(main())
