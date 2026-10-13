#!/usr/bin/env python3
"""Exercise independent Turso processes and verify acknowledged transactions."""

import argparse
import json
import pathlib
import subprocess
import tempfile
import time


def call(binary, database, mode, *args):
    result = subprocess.run(
        [binary, *args, "--db", str(database), "--mode", mode],
        check=True, capture_output=True, text=True,
    )
    return json.loads(result.stdout)


def trial(binary, mode, seconds, base, number):
    directory = pathlib.Path(tempfile.mkdtemp(prefix=f"trial-{number}-", dir=base))
    database = directory / "history.db"
    call(binary, database, mode, "sql", "SELECT 1 AS ready")
    children = []
    outputs = []
    try:
        for role, count in (("writer", 16), ("reader", 4)):
            for worker in range(count):
                output = open(directory / f"{role}-{worker}.log", "w")
                outputs.append(output)
                process = subprocess.Popen(
                    [binary, "stress-worker", role, str(worker), str(seconds),
                     str(directory), "--db", str(database), "--mode", mode],
                    stdout=output, stderr=subprocess.STDOUT,
                )
                children.append(process)
        deadline = time.monotonic() + 120
        while len(list(directory.glob("*.ready"))) < 20:
            if any(child.poll() is not None for child in children):
                raise RuntimeError("worker exited before barrier")
            if time.monotonic() > deadline:
                raise RuntimeError("worker startup exceeded 120 seconds")
            time.sleep(0.05)
        started = time.monotonic()
        (directory / "start").write_text("start")
        for child in children:
            if child.wait(timeout=seconds + 120) != 0:
                raise RuntimeError(f"worker {child.pid} failed")
        elapsed = time.monotonic() - started
        expected = {}
        conflicts = retries = reader_operations = 0
        latencies = []
        for ledger in directory.glob("*.jsonl"):
            entries = [json.loads(line) for line in ledger.read_text().splitlines()]
            if not entries or not entries[-1].get("finished"):
                raise RuntimeError(f"incomplete ledger {ledger}")
            if ledger.name.startswith("reader"):
                reader_operations += entries[-1]["operations"]
                continue
            for entry in entries[:-1]:
                identity = entry["id"]
                if identity in expected:
                    raise RuntimeError(f"duplicate acknowledgement {identity}")
                expected[identity] = str(entry["sequence"])
                conflicts += entry["write"]["conflicts"]
                retries += entry["write"]["retries"]
                latencies.append(entry["write"]["commit_us"])
        actual = call(
            binary, database, mode, "sql",
            "SELECT r.id,t.name,m.value FROM runs r "
            "JOIN tests t ON t.run=r.id JOIN metrics m ON m.run=r.id "
            "WHERE m.key='sequence' ORDER BY r.id",
        )
        found = {}
        for result in actual:
            identity = result["id"]
            if identity in found or result["name"] != identity:
                raise RuntimeError(f"duplicate or partial transaction {identity}")
            found[identity] = result["value"]
        if found != expected:
            raise RuntimeError("acknowledged rows differ from stored rows")
        for table in ("runs", "tests", "metrics"):
            counts = call(binary, database, mode, "sql", f"SELECT COUNT(*) AS n FROM {table}")
            if counts[0]["n"] != len(expected):
                raise RuntimeError(f"half applied transaction in {table}")
        integrity = call(binary, database, mode, "sql", "PRAGMA integrity_check")
        if integrity != [{"integrity_check": "ok"}]:
            raise RuntimeError(f"integrity failure: {integrity}")
        latencies.sort()
        if not latencies or reader_operations == 0:
            raise RuntimeError("stress did not exercise readers and writers")
        report = {
            "trial": number, "mode": mode, "seconds": elapsed,
            "committed": len(expected), "transactions_per_second": len(expected) / elapsed,
            "conflicts": conflicts, "retries": retries,
            "p99_commit_ms": latencies[(len(latencies) * 99 - 1) // 100] / 1000,
            "reader_operations": reader_operations, "integrity_check": "ok",
            "exactly_once": True, "atomic": True, "directory": str(directory),
        }
        (directory / "result.json").write_text(json.dumps(report, indent=2) + "\n")
        return report
    except Exception as error:
        report = {"trial": number, "mode": mode, "error": str(error), "directory": str(directory)}
        (directory / "result.json").write_text(json.dumps(report, indent=2) + "\n")
        return report
    finally:
        for child in children:
            if child.poll() is None:
                child.kill()
                child.wait()
        for output in outputs:
            output.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--mode", default="multiprocess", choices=["multiprocess"])
    parser.add_argument("--seconds", type=int, default=60)
    parser.add_argument("--trials", type=int, default=5)
    parser.add_argument("--directory", required=True)
    options = parser.parse_args()
    pathlib.Path(options.directory).mkdir(parents=True, exist_ok=True)
    reports = []
    for number in range(1, options.trials + 1):
        report = trial(options.binary, options.mode, options.seconds, options.directory, number)
        reports.append(report)
        print(json.dumps(report), flush=True)
    pathlib.Path(options.directory, "results.json").write_text(json.dumps(reports, indent=2) + "\n")
    return int(any("error" in report for report in reports))


if __name__ == "__main__":
    raise SystemExit(main())
