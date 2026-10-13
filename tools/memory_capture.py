#!/usr/bin/env python3
"""Godot launch adapter: sample only the owned process tree's VmRSS on Linux."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def processes(root):
    pending = [root]
    seen = set()
    while pending:
        pid = pending.pop()
        if pid in seen:
            continue
        seen.add(pid)
        yield pid
        try:
            pending.extend(int(p) for p in Path(f"/proc/{pid}/task/{pid}/children").read_text().split())
        except (OSError, ValueError):
            pass


def sample(pid):
    try:
        path = Path(f"/proc/{pid}")
        executable = (path / "exe").resolve().name
        if not executable.startswith("Godot_"):
            return None
        fields = dict(line.split(":", 1) for line in (path / "status").read_text().splitlines())
        return {key: int(fields[key].split()[0]) for key in ("VmRSS", "VmHWM")}
    except (OSError, KeyError, ValueError):
        return None


def main():
    output = Path(os.environ["SOE_RSS_OUTPUT"])
    output.mkdir(parents=True, exist_ok=True)
    script = next((arg for arg in sys.argv[1:] if arg.endswith(".gd")), "godot")
    name = Path(script).stem
    wrapper = os.environ.get("SOE_MEMORY_GODOT_WRAPPER", "/run/media/pengolodh/linux-data-games/tmp/soe-run/bin/godot")
    if os.environ.get("DISPLAY") != ":99":
        raise SystemExit("memory captures require Xvfb :99")
    started = time.monotonic()
    child = subprocess.Popen([wrapper, *sys.argv[1:]], close_fds=False)
    peaks = {}
    with (output / (name + ".rss.tsv")).open("w") as trace:
        trace.write("elapsed_s\tpid\tVmRSS_kib\tVmHWM_kib\n")
        while child.poll() is None:
            for pid in processes(child.pid):
                status = sample(pid)
                if status is None:
                    continue
                peak = peaks.setdefault(pid, {"VmRSS": 0, "VmHWM": 0})
                for key, value in status.items():
                    peak[key] = max(peak[key], value)
                trace.write(f"{time.monotonic()-started:.3f}\t{pid}\t{status['VmRSS']}\t{status['VmHWM']}\n")
            trace.flush()
            time.sleep(0.02)
    report = {"script": script, "display": os.environ["DISPLAY"], "exit": child.returncode,
              "elapsed_s": time.monotonic()-started, "processes": peaks,
              "peak_rss_gib": max((p["VmRSS"] for p in peaks.values()), default=0) / 1048576,
              "peak_hwm_gib": max((p["VmHWM"] for p in peaks.values()), default=0) / 1048576}
    report["rss_limit_gib"] = 8
    report["within_rss_limit"] = bool(peaks) and report["peak_rss_gib"] < 8
    (output / (name + ".rss.json")).write_text(json.dumps(report, indent=2) + "\n")
    print("MEMORY_RSS " + json.dumps(report), file=sys.stderr)
    return child.returncode


if __name__ == "__main__":
    sys.exit(main())
