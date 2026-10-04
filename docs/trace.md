# soe-trace: timing spans and deterministic work

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

## Load causes, PRF-29

`LoadScope` records loading independently of `Trace::enable`. Its bounded
65,536-event history retains phase, cause, asset, owning object, parent, root,
thread, elapsed time and exclusive time. The summary reports evicted events.
Nested scopes subtract their elapsed time from their parent on the same thread.
`LoadToken` carries parent and object identity to workers, whose durations do
not subtract from the submitting thread. `queue_wait` measures batch submission
to chunk entry. It includes scheduling delay before a chunk starts.

The cause vocabulary is `io_wait`, `parse`, `decode`, `cpu_build`, `gpu_upload`,
`shader_compile`, `pipeline_compile` and `worker_queue`. Aggregate operations
carry explicit budgets. On completion, root operations and over-budget nested
operations print their duration, budget and largest exclusive cause phase,
including its asset and owning object. Unclassified work reports `unattributed`.
Worker phase durations overlap the caller's wall interval and are not frame
shares or critical-path measurements.

`LoadTrace::summary_text` lists all retained phases and budget reports. Both
`Trace::summary_text` and `Trace::export_chrome_json` include the load history.
Archive index I/O uses `accumulated` to retain summed read and seek times
without a separate event for every call. These sums subtract from the
active parse scope and export as Chrome counters at completion, rather than
pretending disjoint calls form one continuous interval.

The design uses the repository's existing `Instant` timing and the PRF-08
measurement boundaries in `docs/performance.md`. Local
`godot-docs/tutorials/performance/pipeline_compilations.rst` and
`godotwind/src/core/diagnostics/pipeline_compile_monitor.gd` provide the
compilation counter contract. The recorder does not infer compiler duration
or asset ownership from those counters. It adds no dependency or saved state.
