#!/usr/bin/env python3
"""Pick ready BACKLOG.csv rows by task type and prepare worktrees for the soe-backlog workflow.

`ready` lists open rows whose dependencies are done, grouped by the workflow profile that
fits them. `prepare` checks the chosen rows, creates one worktree and branch per row under
target/, and writes the JSON that the soe-backlog workflow takes as its args. The CSV is not
edited here. The lead records a row as done only after checking the finished worktree.
"""

import argparse
import csv
import json
import subprocess
import sys
from pathlib import Path

ENGINE = Path(__file__).resolve().parent.parent
BACKLOG = ENGINE / "BACKLOG.csv"
TYPES = ("native", "gameplay", "format", "host", "shim", "tooling")
# Code forks run at most three at a time (engine CLAUDE.md).
MAX_ROWS = 3


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


def needs_runtime(row):
    """Rows whose check needs observations from the original game, which only the user can run."""
    return "original-runtime" in row["done_check"] or "original runtime" in row["done_check"]


def load(path):
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def unmet(row, status):
    deps = [d for d in row["depends_on"].split(",") if d and d != "-"]
    return [d for d in deps if status.get(d) != "done"]


def ready_rows(rows):
    status = {r["id"]: r["status"] for r in rows}
    return [r for r in rows if r["status"] == "open" and not unmet(r, status)]


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


def cmd_ready(args):
    rows = load(args.csv)
    shown = 0
    for row in ready_rows(rows):
        kind = task_type(row)
        if args.type and kind != args.type:
            continue
        if needs_runtime(row) and not args.include_runtime:
            continue
        writer = "opus" if row["difficulty"] == "very_high" else "sonnet"
        print(f"{row['id']:8} {kind:8} {row['difficulty']:9} {writer:6} {row['title']}")
        shown += 1
        if args.limit and shown >= args.limit:
            break


def cmd_prepare(args):
    if args.type not in TYPES:
        sys.exit(f"unknown type {args.type}, expected one of {', '.join(TYPES)}")
    if len(args.ids) > MAX_ROWS:
        sys.exit(f"{len(args.ids)} rows requested, at most {MAX_ROWS} code forks run at once")
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
        if needs_runtime(row):
            sys.exit(f"{row_id} needs original-runtime observations from the user")
        if task_type(row) != args.type:
            print(f"warning: {row_id} maps to {task_type(row)}, running it as {args.type}", file=sys.stderr)
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
                "title": row["title"],
                "difficulty": row["difficulty"],
                "effort": row["recommended_effort"],
                "crates": row["crates"],
                "read_first": row["read_first"],
                "done_check": row["done_check"],
                "depends_on": row["depends_on"],
                "branch": branch,
                "worktree": str(worktree),
            }
        )
    payload = {"type": args.type, "engine": str(ENGINE), "rows": out_rows}
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
    ready.add_argument("--include-runtime", action="store_true")
    ready.set_defaults(func=cmd_ready)
    prepare = sub.add_parser("prepare", help="create worktrees and print workflow args")
    prepare.add_argument("--type", required=True)
    prepare.add_argument("--out")
    prepare.add_argument("--allow-unmet", action="store_true")
    prepare.add_argument("--dry-run", action="store_true")
    prepare.add_argument("ids", nargs="+")
    prepare.set_defaults(func=cmd_prepare)
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
