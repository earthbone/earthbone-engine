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

## PRF-28 CPU capacity probes

`soe-bench capacity ai synthetic` doubles the existing Travel/Sandbox
workload from 16 actors through a configured maximum. `capacity ai whiterun`
uses the five installed masters and clones enabled WhiterunWorld placed
actor bases at their authored positions, retaining their packages. Neither
fixture loads Papyrus scripts, collision or rendered actors. Both tick Game
at 1/60 second and verify the process counts and actor updates. The installed
fixture uses the shared load order and its cached cell index.

`capacity animation synthetic` evaluates a three-node blend of two constant
99-track spline clips, composes the parent-first skeleton and computes 99
skin matrices per actor. `capacity animation whiterun` selects the first
Whiterun actor from the same fixture, loads its record-selected rig and
appearance from the installed mesh and animation archives, then runs one
interpreter and pose buffers per copy. Asset loading is outside measurement.
The probe prints missing clips, unresolved references and unsupported graph
work. It computes CPU skin matrices without GPU vertex skinning or uploads.

Each CPU rung warms ten ticks and measures 120 ticks. CLI options are
`--start`, `--max`, `--samples` and `--warmup`. The limits are p99 16.7 ms and
worst 33 ms. Every rung prints `CAPACITY_SAMPLE`, and `CAPACITY_RESULT`
records the highest passing rung and first failing rung. A passing configured
maximum reports `limit_not_found`. A completed curve returns zero even when it
finds a breach, an invalid workload returns nonzero. These are single-process
ladders, without repeated boundary refinement. Counts above the sampled
passing rung are not established capacity.

Only high-process AI and one worker execute in this checkout. `--level high`
and `--workers 1` are accepted for AI. Other levels and worker counts fail
before setup. The benchmark reports these capabilities explicitly.

`capacity settings` reads only Skyrim.esm and lists numeric GMST candidate
names containing actor, process, light or shadow, in sorted order, through
World's existing setting reader. It also reports whether the three published
actor-cap candidate names are stored. Numeric mod settings retain the
existing editor-ID override behavior, exercised by a synthetic patch test.
This survey does not apply actor or light limits to runtime scheduling.

The `prf_28_` tests cover ladder boundaries, an unreached maximum, invalid
counts, nearest-rank percentiles, unsupported process modes, mod setting
values, actor updates and the animation blend/skin pipeline. Installed cases
are gated on SKYRIM_DATA. Sources read include local UESP GMST, xEdit's
`wbDefinitionsTES5.pas`, Wrye Bash's `multitweak_settings.py`, CommonLibSSE-NG
`AIProcess.h` and `ProcessLists.h`, and godotwind's benchmarking manual.
The existing readers and host skinning implementation supply the algorithms.
No new external dependency was added. HKX and NIF are existing workspace
crates, NIF moved from a test dependency to a runtime dependency here.
