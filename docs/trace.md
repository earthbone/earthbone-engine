# soe-trace: timing spans and deterministic work

## Hitch history, PRF-18

The main thread keeps a 65,536-event ring of named spans, exclusive system
spans, frame spans and all ten work counters, retaining the last five seconds.
Recording runs with optional tracing off. System events reuse PRF-12's clock reads.
Frame completion samples the active work recorder, including joined workers'
counts. Worker timings remain on their calling thread's ring. Static `Span`
names and payloads are retained. Dynamic spans use their category as a name
when optional tracing is off, without calling the name-building closure.
With optional tracing on, their full names are retained too. System and
operation names use fixed slots. After initial allocation, normal recording
with optional tracing off neither allocates nor locks.

`FrameHistory::record` detects frames strictly above 33.4 ms, twice the
16.7 ms budget. `Trace::set_hitch_threshold` accepts a positive duration.
A hitch pins the preceding five seconds until one second of subsequent
wall time has passed. Further hitches do not extend that tail. The next
completed frame hands the capture to a background writer. At least ten
seconds pass between a dump and the next eligible trigger. A process-wide
limiter also separates dumps from concurrent recording threads. A full ring
overwrites its oldest event and reports the count in the trace and summary.

The writer emits Chrome complete spans and counter events to a JSON file
and a matching one-line text summary. Times are microseconds relative to the
window's beginning, and spans are clipped to the window. The summary lists
three span names ordered by longest individual duration. System spans are
exclusive while ordinary named spans include nested work. Filenames contain
process, thread and trigger identity. Write failures are reported.
`Trace::wait_for_hitch_exports` joins submitted writes and returns failures.
Thread shutdown also joins them. A run that stops before the one-second tail
finishes has no completed capture to export.

The Linux headless default is
`$XDG_DATA_HOME/godot/app_userdata/Skyrim open engine host/hitches`, using
`$HOME/.local/share` when XDG_DATA_HOME is absent. `SOE_HITCH_DIR` overrides
this path. The host uses `Trace::set_hitch_directory` with Godot's resolved
`user://hitches`, including custom project user directories.

Sources read were local godotwind's
`src/core/world/performance_profiler.gd`, spdlog's
`details/backtracer-inl.h` in SKSE64Plugins, and Godot's ProjectSettings
class documentation. Their dynamic GDScript arrays and mutex-protected log
messages do not fit the fixed main-thread system slots. The Rust ring reuses
the repository's clock and Chrome exporter design without adding a dependency
or saved state. The `prf_18_` tests cover retention, capacity, threshold,
tail, cooldown, export and I/O failure, plus a forced synthetic hitch with
optional tracing off. The optimized overhead checks use a limit below 1.01.

## System timings and work counts

`Span` records optional Chrome trace events in the existing global event list.
`SystemScope` also provides always-on exclusive elapsed timings for fourteen fixed
slots: Papyrus VM, AI planning, AI movement, combat, magic, quests and story
manager, dialogue, physics sync, cell streaming, audio, save, input handling,
host render submission and other work.

The counters are thread-local and use no event strings or mutexes. The hitch
ring allocates once when first initialized on a recording thread.
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
and worst alongside its rolling p50 and p99. Ordinary recording does not sort
or allocate. A completed hitch tail allocates a capture for the writer.
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
