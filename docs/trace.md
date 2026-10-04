# soe-trace: timings and structured logging

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
