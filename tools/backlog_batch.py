#!/usr/bin/env python3
"""Pick ready BACKLOG.csv rows and prepare worktrees for the soe-backlog workflow.

`ready` lists open rows whose dependencies are done, cheapest first. `plan` picks a batch:
with `--area` every ready row of that area, otherwise at most one row per area and one host
row. `prepare` checks the chosen rows, creates one worktree and branch per row under target/,
and prints the JSON that the soe-backlog workflow takes as its args. The workflow runs the
rows in lanes, each lane with one reusable build directory that it cleans of workspace
packages between rows, because cargo fingerprints workspace crates by relative path and
would otherwise link a library built from another worktree's source. The CSV is not edited
here. The lead records a row as done only after checking the finished worktree.
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
MAX_ROWS = 100
LANES = 9
CARGO_JOBS = 6
# Each lane's build directory takes about 4 GiB, plus room for the lead's master gate.
LANE_GIB = 4
MIN_FREE_GIB = 15
# The repository has no git identity, so commits and merges set it in the environment.
IDENTITY = ("GIT_AUTHOR_NAME=pengolodh GIT_AUTHOR_EMAIL=lysander.deloore@gmail.com "
            "GIT_COMMITTER_NAME=pengolodh GIT_COMMITTER_EMAIL=lysander.deloore@gmail.com")
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


def plan_batch(rows, count, skip=(), area=None):
    """Take ready rows in order. With an area, every ready row of it, otherwise one per area and one host row."""
    chosen, areas, hosts = [], set(), 0
    for row in ready_rows(rows):
        if len(chosen) == count:
            break
        if row["id"] in skip or needs_user(row):
            continue
        if area:
            if row["area"] == area:
                chosen.append(row)
            continue
        if row["area"] in areas:
            continue
        if task_type(row) == "host":
            if hosts:
                continue
            hosts += 1
        areas.add(row["area"])
        chosen.append(row)
    return chosen


def plan_area(rows, area, skip=(), count=MAX_ROWS):
    """Every row of one or more areas (comma separated) that can finish in one run: ready rows
    first, then rows whose unmet dependencies are all earlier in the list. The order is
    topological, which the workflow's lanes rely on."""
    areas = set(area.split(","))
    status = {r["id"]: r["status"] for r in rows}
    chosen = [r for r in ready_rows(rows) if r["area"] in areas and r["id"] not in skip and not needs_user(r)]
    ids = {r["id"] for r in chosen}
    waiting = [
        r for r in rows
        if r["area"] in areas and r["status"] in ("open", "blocked") and r["id"] not in ids
        and r["id"] not in skip and not needs_user(r)
    ]
    grew = True
    while grew and len(chosen) < count:
        grew = False
        for r in list(waiting):
            if all(d in ids for d in unmet(r, status)):
                chosen.append(r)
                ids.add(r["id"])
                waiting.remove(r)
                grew = True
    return chosen[:count]


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


def workspace_crates():
    """Package names of every crate under crates/, for cleaning a lane between rows."""
    names = []
    for manifest in sorted((ENGINE / "crates").glob("*/Cargo.toml")):
        for line in manifest.read_text().splitlines():
            if line.startswith("name = "):
                names.append(line.split('"')[1])
                break
    return names


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
    rows = load(args.csv)
    if args.area:
        chosen = plan_area(rows, args.area, set(args.skip), args.count)
    else:
        chosen = plan_batch(rows, args.count, set(args.skip))
    for row in chosen:
        print(row["id"])


def cmd_prepare(args):
    if len(args.ids) > MAX_ROWS:
        sys.exit(f"{len(args.ids)} rows requested, at most {MAX_ROWS} run at once")
    lanes = min(args.lanes, len(args.ids))
    need = MIN_FREE_GIB + LANE_GIB * lanes
    if not args.dry_run and free_gib() < need:
        sys.exit(f"{free_gib():.0f} GiB free, {lanes} lanes need {need} GiB, remove merged worktrees first")
    rows = load(args.csv)
    by_id = {r["id"]: r for r in rows}
    status = {r["id"]: r["status"] for r in rows}
    chosen = []
    for row_id in args.ids:
        row = by_id.get(row_id)
        if row is None:
            sys.exit(f"{row_id} is not in {args.csv}")
        earlier = {r["id"] for r in chosen}
        missing = [d for d in unmet(row, status) if d not in earlier]
        if row["status"] not in ("open", "blocked") or (row["status"] == "blocked" and missing):
            sys.exit(f"{row_id} is {row['status']}, only open rows or rows waiting on earlier rows are prepared")
        if missing and not args.allow_unmet:
            sys.exit(f"{row_id} depends on unfinished {', '.join(missing)} that is not earlier in this run")
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
            }
        )
    payload = {
        "engine": str(ENGINE),
        "concurrency": args.lanes,
        "jobs": args.jobs,
        "sccache": shutil.which("sccache") is not None,
        "workspace_crates": workspace_crates(),
        "merge": args.merge,
        "identity": IDENTITY,
        "rows": out_rows,
    }
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
    plan = sub.add_parser("plan", help="pick the next batch, a whole area or one row per area")
    plan.add_argument("--count", type=int, default=MAX_ROWS)
    plan.add_argument("--skip", nargs="*", default=[], help="row ids to leave out, such as partial rows")
    plan.add_argument("--area", help="every row of one or more areas that can finish in one run, such as AI or PRG,CMB")
    plan.set_defaults(func=cmd_plan)
    prepare = sub.add_parser("prepare", help="create worktrees and print workflow args")
    prepare.add_argument("--type", choices=TYPES, help="override the profile for every row")
    prepare.add_argument("--out")
    prepare.add_argument("--allow-unmet", action="store_true")
    prepare.add_argument("--dry-run", action="store_true")
    prepare.add_argument("--lanes", type=int, default=LANES, help="rows the workflow runs at once")
    prepare.add_argument("--jobs", type=int, default=CARGO_JOBS, help="cargo jobs per lane")
    prepare.add_argument("--no-merge", dest="merge", action="store_false",
                         help="leave finished rows in their worktrees instead of merging them")
    prepare.add_argument("ids", nargs="+")
    prepare.set_defaults(func=cmd_prepare)
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
