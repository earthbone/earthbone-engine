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

`Metric` also exposes stable atomic counters and gauges for PRF-24. Detection
checks and actor pairs accumulate after a successful observation batch. The
Godot audio host publishes active voice count and cumulative decode nanoseconds.
The snapshot always includes pathing, physics, audio, animation, frame and
input metric names, with zero values for measurements whose runtime path does
not publish a value yet. These values are transient and are not saved.

PRF-25 query probes support raycasts, shape casts and overlap calls. Each
completed wrapped query increments its kind and elapsed nanoseconds under the
active `SystemScope`, with unscoped work charged to `other`. The counters are
thread-local and reset for each PlayerNode advance. Current host instrumentation
covers the three direct raycast adapters. The duration is the wrapped query
call, not surrounding query setup or result handling.
`prf_25_physics_queries_count_time_and_charge_the_requesting_system` checks
the three categories and caller attribution.

The bounded history and query-time statistics were checked against local
`engine-refs/godotwind/src/core/world/performance_profiler.gd`. Its dynamic
GDScript model tables do not fit the Rust scheduler's fixed system slots.
The implementation retains the repository's `Instant` timing convention.
It does not store profiling data in saves.

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

## Tracy export, PRF-23

The optional `tracy` Cargo feature uses `tracy-client` 0.19.0. The dependency
is optional and the feature is off by default. With the feature enabled, the
first recorded span starts the Tracy client. Every `Span`, including dynamic
Papyrus native names, opens a Tracy zone and closes it when the Rust scope is
dropped. The adapter uses Tracy's allocated source locations because runtime
native names are not static. WorkFrame drops plot the ten deterministic work
counters, and each completed `FrameHistory` sample emits a continuous frame
mark. Other tracing features keep the Chrome event list and JSON export.

The Tracy dependency enables on-demand capture and limits discovery to the
local host. The editor and installed game are not needed to compile the
feature. `cargo tree -p soe-trace` has no Tracy dependency. With `--features
tracy`, the tree adds `tracy-client` 0.19.0, `tracy-client-sys` 0.30.0 and
its `cc`, `find-msvc-tools` and `shlex` build dependencies.
`prf_23_tracy_adapter_exports_scopes_counters_and_frame_marks` checks static
and dynamic zone lifetimes, all ten counter plots and frame marks when that
feature is enabled.

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
operations log their duration, budget and largest exclusive cause phase,
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

## PRF-37 main-thread total

`FrameSample::engine_ns` sums the exclusive system slots on the recording
thread. `FrameHistory::engine_statistics` computes rolling p50, p99 and
worst from those per-frame sums, rather than summing system percentiles.
The PRF-12 summary appends this total as `main_thread_engine`.
Scopes on simulation and navigation threads never enter the main thread's
clock. Unscoped rendering waits remain in the wall-frame denominator and
do not enter the engine total. The total describes instrumented engine
callbacks, rather than GPU execution or every Godot/GDScript callback.
