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

## Structured logging

`Span` collects opt-in scope timings, Chrome trace events and per-name
statistics. `logging` provides the shared tracing subscriber used by the host.
It follows [tracing's event and structured-field API](https://docs.rs/tracing)
and [tracing-subscriber's EnvFilter and reload layers](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html).
Both libraries use MIT licenses. Filters disable regular-expression matching.

`SOE_LOG` supplies the initial directives, with `info` as the default.
Targets include `game`, `game::ai`, `papyrus`, `esm`, `bsa`, `nif`, `navmesh`
and `host::<module>`. Events carry a `system` field and contextual form,
actor, plugin or script fields where the caller has that identity. Entered
tracing spans contribute their fields to contained events. The trace crate
reexports the five level macros. `release_max_level_info` removes debug and
trace events, including their argument evaluation, from release builds.

`LogControl::set_filter` reloads directives. Invalid input preserves the
previous filter. `flush` emits completed one-second buckets. Identical
level, target and field sets collapse into one line with `count`, while
changed actors or other fields keep separate lines. `flush_all` also emits
the partial bucket, for explicit captures and shutdown. Host callbacks run
after the state lock is released and still receive lines after a file error.

Files append to `engine.log`, rotating by size at 4 MiB with five numbered
backups. Each event is one escaped text line. The pending set holds at most
1024 identities, the output queue and history each hold at most 8192 lines.
Under sustained distinct-message traffic beyond those bounds, the queues
discard their oldest retained data. `window` selects messages by inclusive
logging-clock timestamps before grouping them, including unflushed messages.
`export_window` writes that selection for a hitch recorder. This checkout
has no PRF-18 recorder, so the API is tested independently of automatic hitch
capture. Logging state is transient and adds no save section.

The Godot routing contract comes from the local
`engine-refs/godot-docs/classes/class_@globalscope.rst` and
`tutorials/scripting/logging.rst`. Godot's file logger rotates whole sessions.
The engine writer adds bounded size rotation and retained log windows without
requiring a second runtime or logging thread.

`cargo tree -p soe-trace` reports tracing, tracing-core, pin-project-lite and
once_cell, plus tracing-subscriber's matchers, regex-automata, regex-syntax,
sharded-slab, lazy_static, thread_local and cfg-if. Typed errors use the
workspace's existing thiserror. Tempfile is a test dependency. No async
runtime or Godot dependency enters this crate.

The `tool_06_` tests cover reloads, invalid filters, five levels, span fields,
release argument elision, per-second counts, actor separation, shutdown,
reentrant sinks, rotation, escaped lines, bounded retention and file errors.
The hitch-window test includes unflushed messages and excludes both outside
boundaries. Host routing and F3 controls are tested in Godot separately.
