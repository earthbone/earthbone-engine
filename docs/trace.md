# soe-trace

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
