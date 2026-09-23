#!/usr/bin/env python3
"""Pick ready BACKLOG.csv rows and prepare worktrees for the soe-backlog workflow.

`ready` lists open rows whose dependencies are done, cheapest first. `plan` picks a batch
with at most one row per area and one host row, so parallel rows rarely touch the same
files. `prepare` checks the chosen rows, creates one worktree and branch per row under
target/, and prints the JSON that the soe-backlog workflow takes as its args. Each row
builds in its own target directory. A shared one is unsafe across worktrees, because cargo
fingerprints workspace crates by relative path and links a library built from another
worktree's source. The slim flag turns off incremental state and full debug info, which
kept those directories at 6 to 18 GiB. The CSV is not edited here. The lead records a row
as done only after checking the finished worktree.
"""

import argparse
import collections
import csv
import json
import shutil
import subprocess
import sys
from pathlib import Path

ENGINE = Path(__file__).resolve().parent.parent
BACKLOG = ENGINE / "BACKLOG.csv"
TYPES = ("native", "gameplay", "format", "host", "shim", "tooling")
DIFFICULTY = ("low", "medium", "high", "very_high")
# Cheaper profiles first when difficulty ties. Host rows build a release extension and run Godot.
TYPE_COST = {"tooling": 0, "format": 1, "native": 2, "gameplay": 3, "shim": 4, "host": 5}
MAX_ROWS = 4
# Builds, coverage runs and Godot exports all land on this disk. Stop before it fills.
MIN_FREE_GIB = 20
USER_ONLY = ("original-runtime", "original runtime", "physical speaker", "windowed")


def task_type(row):
    """Map a row to the workflow profile that fits its area and crates."""
    area = row["area"]
    crates = set(row["crates"].split(","))
    if area == "SCR":
        return "native"
    if area == "SHM":
        return "shim"
    if area in ("TST", "TOOL", "PRF"):
        return "tooling"
    if "godot-host" in crates:
        return "host"
    if "game" in crates:
        return "gameplay"
    return "format"


def needs_user(row):
    """Rows whose check needs the original game, a speaker or a person watching a window."""
    text = (row["title"] + " " + row["done_check"]).lower()
    return any(marker in text for marker in USER_ONLY)


def load(path):
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def deps(row):
    return [d for d in row["depends_on"].split(",") if d and d != "-"]


def unmet(row, status):
    return [d for d in deps(row) if status.get(d) != "done"]


def ready_rows(rows):
    """Open rows with finished dependencies, cheapest first, then by how many rows they unlock."""
    status = {r["id"]: r["status"] for r in rows}
    unlocks = collections.Counter(d for r in rows if r["status"] != "done" for d in deps(r))
    ready = [r for r in rows if r["status"] == "open" and not unmet(r, status)]
    return sorted(
        ready,
        key=lambda r: (DIFFICULTY.index(r["difficulty"]), -unlocks[r["id"]], TYPE_COST[task_type(r)]),
    )


def plan_batch(rows, count, skip=()):
    """Take ready rows in order, at most one per area and one host row."""
    chosen, areas, hosts = [], set(), 0
    for row in ready_rows(rows):
        if len(chosen) == count:
            break
        if row["id"] in skip or needs_user(row) or row["area"] in areas:
            continue
        if task_type(row) == "host":
            if hosts:
                continue
            hosts += 1
        areas.add(row["area"])
        chosen.append(row)
    return chosen


def git(*args):
    return subprocess.run(["git", "-C", str(ENGINE), *args], capture_output=True, text=True)


def branch_for(row_id):
    """Return a branch name that is free, suffixing one when an earlier attempt was merged."""
    base = "task-" + row_id.lower()
    name, n = base, 1
    while git("rev-parse", "--verify", "--quiet", "refs/heads/" + name).returncode == 0:
        if git("merge-base", "--is-ancestor", name, "master").returncode != 0:
            sys.exit(f"branch {name} has unmerged commits, merge or resume it first")
        n += 1
        name = f"{base}-r{n}"
    return name


def free_gib():
    return shutil.disk_usage(ENGINE).free / 2**30


def cmd_ready(args):
    shown = 0
    for row in ready_rows(load(args.csv)):
        kind = task_type(row)
        if args.type and kind != args.type:
            continue
        if needs_user(row) and not args.include_user:
            continue
        writer = "opus" if row["difficulty"] == "very_high" else "sonnet"
        print(f"{row['id']:8} {kind:8} {row['difficulty']:9} {writer:6} {row['title']}")
        shown += 1
        if args.limit and shown >= args.limit:
            break


def cmd_plan(args):
    for row in plan_batch(load(args.csv), args.count, set(args.skip)):
        print(row["id"])


def cmd_prepare(args):
    if len(args.ids) > MAX_ROWS:
        sys.exit(f"{len(args.ids)} rows requested, at most {MAX_ROWS} run at once")
    if not args.dry_run and free_gib() < MIN_FREE_GIB:
        sys.exit(f"{free_gib():.0f} GiB free, below {MIN_FREE_GIB} GiB, remove merged worktrees first")
    rows = load(args.csv)
    by_id = {r["id"]: r for r in rows}
    status = {r["id"]: r["status"] for r in rows}
    chosen = []
    for row_id in args.ids:
        row = by_id.get(row_id)
        if row is None:
            sys.exit(f"{row_id} is not in {args.csv}")
        if row["status"] != "open":
            sys.exit(f"{row_id} is {row['status']}, only open rows are prepared")
        missing = unmet(row, status)
        if missing and not args.allow_unmet:
            sys.exit(f"{row_id} depends on unfinished {', '.join(missing)}")
        if needs_user(row):
            sys.exit(f"{row_id} needs the user: original runtime, a speaker or a watched window")
        chosen.append(row)
    out_rows = []
    for row in chosen:
        branch = branch_for(row["id"])
        worktree = ENGINE / "target" / ("soe-" + branch)
        if not args.dry_run:
            added = git("worktree", "add", str(worktree), "-b", branch, "master")
            if added.returncode != 0:
                sys.exit(added.stderr.strip())
        out_rows.append(
            {
                "id": row["id"],
                "type": args.type or task_type(row),
                "title": row["title"],
                "difficulty": row["difficulty"],
                "effort": row["recommended_effort"],
                "crates": row["crates"],
                "read_first": row["read_first"],
                "done_check": row["done_check"],
                "depends_on": row["depends_on"],
                "branch": branch,
                "worktree": str(worktree),
                "target_dir": str(worktree / "target"),
            }
        )
    payload = {"engine": str(ENGINE), "slim": True, "rows": out_rows}
    text = json.dumps(payload, indent=2)
    if args.out:
        Path(args.out).write_text(text + "\n")
    print(text)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--csv", default=str(BACKLOG))
    sub = parser.add_subparsers(dest="command", required=True)
    ready = sub.add_parser("ready", help="list open rows whose dependencies are done")
    ready.add_argument("--type", choices=TYPES)
    ready.add_argument("--limit", type=int, default=0)
    ready.add_argument("--include-user", action="store_true")
    ready.set_defaults(func=cmd_ready)
    plan = sub.add_parser("plan", help="pick the next batch, one row per area")
    plan.add_argument("--count", type=int, default=MAX_ROWS)
    plan.add_argument("--skip", nargs="*", default=[], help="row ids to leave out, such as partial rows")
    plan.set_defaults(func=cmd_plan)
    prepare = sub.add_parser("prepare", help="create worktrees and print workflow args")
    prepare.add_argument("--type", choices=TYPES, help="override the profile for every row")
    prepare.add_argument("--out")
    prepare.add_argument("--allow-unmet", action="store_true")
    prepare.add_argument("--dry-run", action="store_true")
    prepare.add_argument("ids", nargs="+")
    prepare.set_defaults(func=cmd_prepare)
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
