# soe-bench: performance harness

Commands, scenarios and timing budgets are documented in
[performance.md](performance.md) and [performance-limits.md](performance-limits.md).

`soe-bench profile-diff before.json after.json` reads two PRF-13 Chrome
counter summaries without requiring installed data. It groups entity phases,
script functions and plugin totals, and compares actors, scripts, quests,
plugins and systems. Each timing or count metric lists its ten largest
absolute changes, with before, after, signed delta and percentage relative to
the baseline. Equal changes use stable identity order. A zero baseline is
reported as new, and a removal as minus 100 percent. Game and host system
domains retain distinct identities. Older summaries without explicit entity
metadata use the existing entity.phase name. System totals absent in older
files are zero. Scope traces, malformed metrics and counter overflows fail.

The parser uses the existing serde_json dependency, moved from test-only to
runtime use, and the existing PRF-13 envelope. Self-time comparisons follow
local `godot-docs/tutorials/scripting/debug/the_profiler.rst`. godotwind's
`performance_profiler.gd` supplies timing rankings but has no Rust entity
summary reader. `prf_22_` tests record 600 PLY-01 steps, replay twice with the
same state hashes, inject a delay into a real VM script and check that it
heads the script self-time diff. Other checks cover every entity kind,
operation counts, zero baselines, removals and invalid inputs.

`soe-bench frame-budget` runs a fixed PRF-16 workload with 1,000 travel and
sandbox actors, a record-backed NAVM, ten warm-up ticks and 3,600 measured
ticks paced over 60 seconds. It verifies movement, navigation and package
selection for every actor. It emits one `SOE_BUDGET` JSON line containing
full-run CPU frame and PRF-12 per-system p50, p99 and worst milliseconds,
including idle slots. The suite runner applies the budgets in performance.md.
Setup, pacing and reporting are outside the tick timing window. The command
rejects arguments that could change the fixed workload.

TST-18's non-ignored thousand-actor checks enforce p99 16.7 ms and worst
33 ms, with process RSS below 2,048 MiB, over both 120 cold optimized ticks
and the runner's 120 ticks after one warm-up tick. The cold child is stopped
and reaped after 120 seconds. The normal Cargo test builds its optimized
benchmark child without replacing build flags. The
unoptimized cold check bounds navigation triangle examinations to 4,000 per
tick, including the first package tick. The record-backed
Travel and Sandbox fixture has 60,002 NAVM triangles across four overlapping
floors. Graph construction and all actor package selections happen before
timing. The stress report includes
`navmesh_triangles` and `package_evaluations`. Every actor must select its
expected procedure, with finite positions and advancing Travel actors.
The PRF-07 runner test checks numeric timing and workload values in its JSON
report. The NAVM writer and local xEdit `wbNVNM` definition supply the fixture
layout and index widths.

The `tst_18_walkthrough` integration target runs the actual walkthrough
startup headless for Bannered Mare and the Whiterun exterior, with a
20-wall-second gameplay settling phase followed by active NPC packages for
at least 60 measured wall seconds each. It is gated on `SKYRIM_DATA` and
uses `SOE_GODOT_BIN`. Its child deadline, pipe draining, exit checks and
measurement assertions fail incomplete or over-budget runs. The complete
contract and commands are in [performance.md](performance.md).

The stress runner captures a `soe_trace::WorkFrame` around each measured
step and prints the maximum count per operation for each sample window as
`work_peak_<operation> <count> count`. Setup, warm-up and reports are excluded.
Nested Game ticks enter the same window. Fixed `prf_14_` tests run the existing
PRF-07 world with 1,000 travel and sandbox actors, ten warm-up ticks and
120 measured ticks, plus the mixed
fixture with 16 actors, scripts and paths. They check every operation against
fixed limits without elapsed-time assertions and verify the fixture's actor
movement and script completion. The scenarios follow local
`godotwind/docs/systems/benchmarking.md` guidance on fixed workload and
separate setup.
