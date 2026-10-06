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

Allocation samples read `soe-trace::MemorySnapshot` from the host counting
allocator, enabled by the benchmark's `profile-memory` dependency feature.
They report successful allocation and reallocation request counts, and the
sum of requested sizes, between sample boundaries. This uses the same Rust
allocation accounting as the host, with one process allocator. Reallocation
bytes describe the new request's size. Godot's own heap is outside the sample.
