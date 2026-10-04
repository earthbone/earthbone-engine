# soe-trace: timing spans and deterministic work

## Cell costs and allocation counters, PRF-20

`LoadRecorder` follows one cell through worker preparation, collision and host
upload. Its scoped capture restores the previous cell on return or unwind.
Joined workers explicitly enter the caller's recorder. `LoadReport` includes
archive I/O by winning archive path or loose source, NIF parse, texture
preparation, texture upload, collision build and NAVM preparation, including
zero slots. Durations are cumulative CPU work and waits, rather than wall-time
shares. Texture preparation includes DDS parsing and compressed mip padding.
Archive reads include decompression and lock waits. Collision construction
excludes separately measured archive reads and NIF parsing.

Asset costs sum measured operations for each canonical path. The slowest asset
uses that cumulative cost, with lexical tie breaking. `diagnostic` names it
alongside stage and archive totals. Texture inventories contain one payload
size per canonical path per cell, including generated terrain and cube faces.
These totals describe referenced upload bytes, rather than driver allocations.

`MemorySnapshot` holds process-wide Rust allocation counters for every system.
The host counting allocator retains the allocating system in a prefix and
charges deallocation and reallocation to that owner even after scope or thread
changes. `FrameSample` reports live requested bytes and successful allocation
or reallocation calls between snapshots. Godot's heap and GPU allocations are
outside these counters. Instrumentation is enabled in host debug builds and
optimized builds with `profile-memory`.

The timing table was checked against local
`godotwind/src/core/world/performance_profiler.gd`. Its aggregate Godot memory
monitors do not provide Rust allocation ownership, so the host uses the Rust
standard library allocator and layout contract. No dependency or saved state
was added. The PRF-18 dump consumer is absent from this checkout. The diagnostic
API supplies streaming attribution for a consumer to include.

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
