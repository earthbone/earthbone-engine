#!/usr/bin/env python3
"""Assign breaker and judge groups to open backlog rows and set the model columns.

A breaker group is up to BREAKER_CAP weight of rows that read the same code and
share a dependency level, so their writers can run in parallel and one breaker
reads the subsystem once. A judge group splits a breaker group to JUDGE_CAP
weight. Weights follow the measured single-row peaks of 2026-09-27 (breaker 55k,
judge 127k on a very_high row) against the 275k context limit.
"""
import collections, csv, re, sys

WEIGHT = {"low": 1, "medium": 1, "high": 1, "very_high": 2}
JUDGE_WEIGHT = WEIGHT
BREAKER_CAP, JUDGE_CAP = 8, 4
MODEL, EFFORT = "gpt-6-luna", "high"
CHECK_FIELDS = ["breaker_group", "judge_group"]


def topic(r):
    m = re.findall(r"crates/([a-z_-]+)/src/([\w/]+?)(?:/mod)?\.rs", r["read_first"])
    if m:
        c, p = m[0]
        parts = p.split("/")
        return c + "/" + "/".join(parts[:2]) if parts[0] in ("world", "natives", "records", "papyrus") else c + "/" + parts[0]
    d = re.findall(r"docs/([\w-]+)\.md", r["read_first"])
    return "doc:" + d[0] if d else "crate:" + r["crates"].split(",")[0].strip()


def slug(t):
    t = t.replace("crate:", "").replace("doc:", "doc-")
    parts = t.split("/")
    if parts[0] == "game" and len(parts) > 1:
        return parts[-1]
    return "-".join(parts)


def main(path):
    with open(path, newline="") as f:
        reader = csv.DictReader(f)
        fields = list(reader.fieldnames or [])
        rows = list(reader)
    if "breaker_group" not in fields:
        i = fields.index("recommended_effort") + 1
        fields = fields[:i] + CHECK_FIELDS + fields[i:]
    live = {r["id"]: r for r in rows if r["status"] != "done"}

    level = {}

    def depth(rid, seen=()):
        if rid in level:
            return level[rid]
        ds = [d for d in live[rid]["depends_on"].split(",") if d in live and d not in seen]
        level[rid] = 1 + max((depth(d, seen + (rid,)) for d in ds), default=-1)
        return level[rid]

    for rid in live:
        depth(rid)

    prefix = lambda r: r["id"].split("-")[0]
    raw = {rid: (prefix(r), topic(r)) for rid, r in live.items()}
    size = collections.Counter(raw.values())
    key = {rid: (p, t if size[(p, t)] > 1 else "crate:" + live[rid]["crates"].split(",")[0].strip()) for rid, (p, t) in raw.items()}

    buckets = collections.defaultdict(list)
    for rid, k in key.items():
        buckets[(k, level[rid])].append(rid)
    # A row alone in its topic and level joins other lone rows of the same level,
    # first by area and crate, then by area, then by crate across areas, then by
    # level alone, so few checkers run for a single row.
    crate = lambda rid: "crate:" + live[rid]["crates"].split(",")[0].strip()
    for same_area, by_crate in ((True, True), (True, False), (False, True), (False, False)):
        lone = [b for b, ids in buckets.items() if len(ids) == 1]
        for b in lone:
            rid = buckets.pop(b)[0]
            k = (b[0][0] if same_area else "X", crate(rid) if by_crate else "mixed")
            buckets[(k, b[1])].append(rid)
    num = lambda rid: (rid.split("-")[0], int(re.sub(r"\D", "", rid.split("-", 1)[1]) or 0))
    counter = collections.Counter()
    for (k, lv) in sorted(buckets, key=lambda b: (b[0], b[1])):
        ids = sorted(buckets[(k, lv)], key=num)
        groups, cur, w = [], [], 0
        for rid in ids:
            wt = WEIGHT[live[rid]["difficulty"]]
            if cur and w + wt > BREAKER_CAP:
                groups.append(cur)
                cur, w = [], 0
            cur.append(rid)
            w += wt
        if cur:
            groups.append(cur)
        for g in groups:
            counter[k] += 1
            bname = "B-%s-%s-%d" % (k[0], slug(k[1]), counter[k])
            jgroups, cur, w = [], [], 0
            for rid in g:
                wt = JUDGE_WEIGHT[live[rid]["difficulty"]]
                if cur and w + wt > JUDGE_CAP:
                    jgroups.append(cur)
                    cur, w = [], 0
                cur.append(rid)
                w += wt
            jgroups.append(cur)
            for j, jg in enumerate(jgroups):
                jname = "J" + bname[1:] + ("abcd"[j] if len(jgroups) > 1 else "")
                for rid in jg:
                    live[rid]["breaker_group"] = bname
                    live[rid]["judge_group"] = jname
    for r in rows:
        r.setdefault("breaker_group", "")
        r.setdefault("judge_group", "")
        if r["status"] != "done":
            r["recommended_model"], r["recommended_effort"] = MODEL, EFFORT
    if "--write" in sys.argv:
        with open(path, "w", newline="") as f:
            w = csv.DictWriter(f, fieldnames=fields, quoting=csv.QUOTE_ALL, lineterminator="\n")
            w.writeheader()
            w.writerows(rows)
    bg = collections.Counter(r["breaker_group"] for r in live.values())
    jg = collections.Counter(r["judge_group"] for r in live.values())
    print("open rows", len(live), "breaker groups", len(bg), "judge groups", len(jg))
    print("breaker sizes", sorted(collections.Counter(bg.values()).items()), "judge sizes", sorted(collections.Counter(jg.values()).items()))
    print("levels", sorted(collections.Counter(level.values()).items()))
    per_area = collections.Counter(b.split("-")[1] for b in bg)
    print("breaker groups per area", sorted(per_area.items()))


if __name__ == "__main__":
    main(sys.argv[1])
