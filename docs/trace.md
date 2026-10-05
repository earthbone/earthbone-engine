# soe-trace: timing spans and deterministic work

## Entity attribution, PRF-13

PRF-22 summary counters include explicit entity, plugin and phase metadata.
Existing summary names and numeric metrics retain their meaning. Active
captures also retain exclusive system totals through `Profile::systems`.
Game tick and host presentation domains are separate because their intervals
overlap. System totals are excluded from plugin rankings and do not duplicate
entity time in plugin shares. Their call count is the number of frames,
including idle frames. Capture-off frames add no system rows.
The counter format is consumed by `soe-bench profile-diff`, documented in
[performance.md](performance.md).

`Profile` controls optional entity scopes independently of the always-on frame
clock and the existing `Trace` switch. `EntityScope` constructs metadata only
when capture is enabled. Each scope records calls, inclusive and self time,
and the work counts charged inside its innermost entity. Nested scopes subtract
child duration from parent self time. Entity and plugin rankings use self time.
The session totals retain every completed scope. The event list retains at
most `MAX_EVENTS` scopes, dropping the oldest at capacity. The rolling overlay
selects completions in the preceding wall-clock second from that list.

Entities carry a kind, stable identity, owning plugin and phase. Script
identities include the declaring function. Plugin counters sum self time and
innermost operations, avoiding nested time duplication. Latent scheduler
intervals report real seconds separately from running time. Input latency
records wall time until the host completes the token at presentation.
Render-instance submission wrappers measure elapsed time and charge the
existing work counter.

`export_chrome_json` writes completed scopes with entity and plugin arguments.
`export_summary` writes session entity and plugin counters as Chrome counter
events. Both files use the Chrome trace envelope that Perfetto imports. Export
errors return the filesystem error. All attribution is transient and uses no
save fields. The only added dependency is `serde_json` for export tests.
The design uses the repository's Chrome writer and local Godot profiler manual
for inclusive versus self time. godotwind's dynamic model timings informed the
rolling view, but do not provide Rust actor or script identities.

`Span` records optional Chrome trace events in the existing global event list.
`SystemScope` also provides always-on exclusive elapsed timings for fourteen fixed
slots: Papyrus VM, AI planning, AI movement, combat, magic, quests and story
manager, dialogue, physics sync, cell streaming, audio, save, input handling,
host render submission and other work.

The counters are thread-local and use no event strings, allocations or mutexes.
Entering another system charges the elapsed interval to the previous system.
Leaving restores the previous system, so a native operation inside a VM run
is charged once. Re-entering the same system requires no clock read.
`FrameSnapshot` differences cumulative counters without resetting them. The
game can measure its tick inside the host's longer presentation frame.
Scopes outside a tick, such as input callbacks and saves, reach the next host
frame. Worker-thread timings do not add to the main thread's frame share.

`FrameHistory` records frames in a fixed 240-frame ring and keeps session
totals and worst values. Queries compute nearest-rank p50, p99 and worst
over the ring, including zero samples for idle systems. The last frame exposes
milliseconds and a percentage of its own wall time. An empty frame returns
zero percentages. The summary includes every system's session total, mean
and worst alongside its rolling p50 and p99. Recording does not sort or allocate.
`with_timing_suspended` provides a thread-local baseline for overhead
calibration and restores recording after the closure, including on unwind.

The bounded history and query-time statistics were checked against local
`engine-refs/godotwind/src/core/world/performance_profiler.gd`. Its dynamic
GDScript model tables do not fit the Rust scheduler's fixed system slots.
The implementation retains the repository's `Instant` timing convention.
It adds no dependencies and does not store profiling data in saves.

The `prf_12_` unit tests cover complete system reports, exclusive nesting,
overlapping snapshots, zero frame shares and rolling eviction. The game
stress check compares recording against a suspended baseline with 1,000
actors and a declared ratio below 1.01. It requires an optimized build.

Timing spans and Chrome trace export are also documented in
[performance.md](performance.md). `WorkFrame::begin` activates a work window
on its entering thread and restores the previous window on drop or unwind.
A nested frame reports work since its entry and contributes once to its
parent. Unrelated threads and concurrent tests have independent windows.
`WorkRecorder::current` and `enter` propagate a capture to worker tasks.
Workers must join before the owning frame is sampled. Totals use relaxed
atomic increments, without clock reads, event allocation or a global mutex.
Outside a work window, operation instrumentation only checks thread-local
capture state. The timing trace's enable switch does not affect work counts.

`WorkCounts` holds ten counters in `Operation::ALL` order:

| Operation | Charged work |
| --- | --- |
| navmesh_locate | Point location or containment attempts |
| navmesh_triangles | Triangle containment tests |
| path_nodes | Non-stale A* queue pops and reachability expansions |
| papyrus_instructions | Executed PEX opcodes |
| native_calls | Invoked native implementations and scheduler builtins |
| conditions | Evaluated CTDA predicates, including errors |
| raycasts | Physics rays, including misses |
| shape_casts | Host sweeps or headless capsule contact and overlap queries |
| record_lookups | Record retrieval attempts, including missing forms |
| render_instances | RenderingServer instance creation calls |

`WorkCounts::check` compares every counter against an upper-bound snapshot.
It returns the first exceeded operation, measured count and limit. Zero
limits reject any unexpected work. Capture tests cover nested scopes,
workers, unrelated threads, reset, limits and unwinding. Scenario bounds
follow the repository's PRF-14 contract and local
`godotwind/docs/systems/benchmarking.md` guidance to fix workloads and keep
setup and diagnostics outside measured intervals. Work counts are transient
and are not part of world or VM saves. No dependency was added.
