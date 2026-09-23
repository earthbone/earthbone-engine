# Performance limits

This harness asks where the current implementation exceeds a stated budget.
It measures synthetic subsystem capacity and real exterior rendering. It does
not claim the capacity of a complete Skyrim gameplay loop.

## Workloads

| Scenario | Load means | Work measured each tick or frame |
|---|---|---|
| CPU `scripts` | Script objects receiving one call per tick | Enqueue calls, advance clocks, execute a 100,000-instruction VM slice. Each call does 32 additions and increments a completion counter. |
| CPU `actors` | Active actors | `Game::tick` over record-backed travel packages and a NAVM corridor. Every actor must move. No animation or collision. |
| CPU `navigation` | Path requests per tick | Connected queries across at least 128 triangles of a fixed 256-triangle graph. Failed queries fail the run. |
| CPU `streaming` | Statics per loaded cell | Walk a bounded route, unload stale cells, build and retain 64-triangle surfaces for new cells. The actual streaming ring stays below 50 cells. |
| CPU `mixed` | The same load in each of the four workloads | Script arrivals, actor ticks, path requests and streaming in one process. Uses Game's tick scheduling, which differs from the standalone VM slice. |
| Godot `exterior` | Exterior radius in cells | The real host's `stream_exterior` around WhiterunOrigin. Reports actual cells, triangles and materials since content can run out before radius does. Requires the game. |
| Godot `geometry` | Visible server instances | A shared 512-triangle plane per instance, arranged in a visible grid. Isolates the host renderer, without NIF decoding. |
| Godot `textures` | Unique visible textures | One mipmapped 512 by 512 RGBA8 texture per server instance, one quad each. Isolates allocation and texture residency. |

Synthetic actor records use the existing package, NPC and NAVM builders.
Actors travel toward a distant target so the workload keeps moving during a
long run. This simple path does not represent dense crowd avoidance or combat.
Synthetic renderer workloads use RenderingServer RIDs, with zero per-instance
scene nodes. They do not exercise the engine's material cache or mesh batching.
The real exterior benchmark exercises those paths separately.

## Measurement contract

Defaults are fixed before the first run: p99 at most 16.7 ms, worst at most
33 ms, no outstanding script calls after a measured tick, process RSS below
2,048 MiB and reported Godot video memory below 4,096 MiB. A soak permits at
most 64 MiB growth above its first measured window. All are CLI options.
These are workload budgets, not claims about every machine.

- `check` repeats one load and fails on a timing, memory or backlog breach.
- `sweep` doubles load up to an explicit maximum. All repeats must agree.
  After the first confirmed breach, it bisects the last passing and first
  failing loads, with repeated fresh processes at every point. This is a
  local budget boundary, not proof of monotonic behavior at every load.
- Mixed pass/fail repeats produce `unstable`. Crashes, invalid output and
  timeouts produce `error`, not a fabricated performance limit.
- Reaching the configured maximum without a breach reports `limit_not_found`.
  Adjacent cost-per-load ratios identify possible nonlinear growth for review.
  Radius is not proportional to geometry, so interpret exterior ratios using
  the reported actual scene counts.
- CPU ticks use 1/60 second of virtual time. Soaks pace them to at most 60 per
  wall second. Sweeps run without pacing. Setup and warm-up are separate.
- CPU samples exclude backlog observation and final correctness checks.
  Their observer cost is reported separately as `telemetry_ms`. Script call
  creation and streaming allocation/release are inside the timed tick.
- CPU windows default to 120 samples. Nearest-rank p99 uses actual samples.
  With fewer than 1,000 samples, p99.9 is effectively a maximum, not strong
  evidence about a rare population tail.
- Godot measures wall time between process calls, GPU and render CPU time,
  over at least 600 frames and five seconds at a fixed 1600 by 900 render
  resolution. Window dimensions are recorded separately. Headless runs fail.
  Script errors and zero rendered work fail even after an exit code of zero.
  The separate headless metric test checks arithmetic, not rendering speed.
- Each sweep repeat launches a fresh process. This clears process caches,
  not the operating system's file cache. A cold texture cache is not cold IO.
- RSS is observed through Linux `/proc`, including setup, with a 50 ms parent
  poll and CPU window snapshots. Short peaks can escape the parent poll.
  Godot's allocator and video-memory counters are also reported. They are
  not total process RSS or measurements of driver eviction.
- No allocation counter is added. RSS growth can come from caches and retained
  capacity as well as leaks. A breached growth budget is evidence to investigate.

Soaks retain the same process and engine state. CPU streaming revisits a
bounded route. Godot clears and rebuilds the same scene after each five-second
window, including the rebuild hitch in the next window. Report generation
is timed separately and excluded from the following frame interval. The real exterior
soak keeps its material/texture caches warm. Measurements are stored one row
per window, keeping sample arrays bounded for runs up to 24 hours.

## Running

Build once, then run benchmarks sequentially on an otherwise idle machine:

```text
rtk proxy cargo build -p soe-bench --profile thin --locked
rtk proxy python3 tools/performance.py sweep --binary target/thin/soe-bench --scenario scripts --start 32 --max-load 4096 --out performance-results/scripts
rtk proxy python3 tools/performance.py check --binary target/thin/soe-bench --scenario actors --start 1000 --out performance-results/actors
rtk proxy python3 tools/performance.py sweep --binary target/thin/soe-bench --scenario mixed --start 1 --max-load 1024 --out performance-results/mixed
rtk proxy python3 tools/performance.py soak --binary target/thin/soe-bench --scenario mixed --start 8 --duration 600 --out performance-results/soak
```

`--duration 86400` requests 24 wall hours. This is supported by the runner,
not a statement that a 24-hour run has passed. CPU workloads need no game data.
Use `--samples`, `--warmup`, `--seed`, `--vm-budget`, `--repeats` and `--refine`
to specify a reproduction. Defaults are printed in `--help` and saved in every
report. Do not change thresholds after seeing results to make a run pass.

Windowed host runs, with an installed Godot binary:

```text
rtk proxy python3 tools/performance.py sweep --backend godot --godot /path/to/godot --scenario geometry --max-load 32768 --out performance-results/geometry
rtk proxy python3 tools/performance.py sweep --backend godot --godot /path/to/godot --scenario textures --max-load 4096 --out performance-results/textures
rtk proxy env SKYRIM_DATA=/path/to/Data python3 tools/performance.py sweep --backend godot --godot /path/to/godot --scenario exterior --max-load 8 --out performance-results/exterior
```

The real exterior path needs the built GDExtension and an imported Godot
project, as described in [godot-host.md](godot-host.md). Synthetic renderer
workloads use stock Godot and do not need the extension. An isolated writable
`XDG_DATA_HOME` is needed when the default Godot data directory is read-only.

The original `godot/bench.gd` also enforces budgets when invoked directly.
`SOE_BENCH_P99_MS` and `SOE_BENCH_WORST_MS` override its 16.7/33 ms defaults.
Invalid values fail. `SOE_BENCH_SECONDS` sets measurement duration, and
`SOE_BENCH_SOAK=1` enables repeated clear/rebuild windows. The Python runner
clears inherited `SOE_BENCH_*` variables so unrelated settings cannot alter
its prescribed workload.

## Reports and automation

Every run gets a new output directory. Existing directories are never
reused. `report.json` is updated after each child exits and includes arguments,
thresholds, seed, commit, dirty-state flag, source/binary hashes, machine
information, commands,
per-window measurements and a boundary or explicit unresolved result. Each
child's full output is retained separately. Exit codes are:

- 0: all requested checks passed, or no boundary was found below the sweep cap.
- 2: a performance/resource budget failed, including a successfully measured
  sweep boundary. Finding a limit does not turn the failed workload green.
- 1: invalid measurement, crash, timeout or inconsistent repeated outcomes.

An interrupted runner leaves an `incomplete` report. Its child runs in a
separate process group and is killed and reaped on interruption. Child commands
reproduce measurements. The saved runner command and arguments also reproduce
the gate, with a fresh `--out` directory.

`tools/lint.sh` runs the runner's deterministic contract tests and Rust workload
checks. `.github/workflows/performance.yml` separately runs optimized mixed
and thousand-actor checks without proprietary assets. Its shared-runner limits
are p99 100 ms, worst 500 ms and RSS 512 MiB. They catch large regressions and
are not 60 Hz gameplay acceptance. Artifacts upload even when a check fails.
GPU sweeps and long soaks remain explicit machine-local runs.

## Sources and implementation choice

Read locally before implementation:

- `engine-refs/godotwind/docs/systems/benchmarking.md` and
  `src/tools/bench_ladder_runner.gd`: workload ladders, warm-up, actual workload
  counters, watchdogs and the distinction between cold setup and warm rendering.
- `engine-refs/openmw/scripts/HOWTO-benchmark.md`: retain separate engine,
  rendering, GPU and resource observations, and account for measurement cost.
- `engine-refs/godot-docs/classes/class_performance.rst` and
  `class_renderingserver.rst`: memory monitor scope and viewport render timings.
- This repository's `TEST_PLAN.md`, `docs/performance.md`, and public engine
  fixtures in `crates/game/tests/packages.rs` and the host streaming test.

The reference runners depend on their own scene and streaming APIs. This
implementation reuses this engine's public APIs and existing builders instead
of copying those runners. Python's standard library supplies process control
and reports. No new Rust or Python dependencies are introduced.

## Remaining boundaries

No integrated combat, dialogue, animation, physics and rendering stress is
claimed. The mixed test combines currently available CPU systems. No large
mod load order sweep, driver-level VRAM eviction tracing or allocation-count
CI gate is supplied. The real exterior benchmark uses the current host's
supported record types and rendering features. Missing host features remain
missing from its measured cost. Recorded runs below distinguish what was
executed from what the harness merely supports.

## Recorded validation, 2026-09-19

Worktree `/tmp/soe-performance-limits`, branch `performance-limits`, based on
`977ab4b`. Optimized Cargo `thin` builds on the Ryzen 9 7900X and RTX 5070 Ti
machine described in `performance.md`. Godot rendering runs used 4.4.1,
Forward+, windowed, vsync disabled. Seed 1, CPU warm-up 10 ticks and 120
samples per window. These are development-machine observations, not portable
capacity guarantees. Other desktop activity was not controlled.

| Run | Result |
|---|---|
| Scripts, 32 to 4,096 arrivals per tick, 3 repeats and 4 refinements | Confirmed passing load 2,816, first failing load 2,944. Failure was outstanding work under the 100,000-instruction slice. Exit 2. |
| Mixed, 1 to 1,024 units, 3 repeats | Passed at 256, failed at 384. Repeats disagreed at 320, so refinement stopped as unstable. Exit 1. |
| 1,000 moving actors, 3 repeats | p99 0.567 to 0.583 ms. All passed the 16.7/33 ms budgets. Exit 0. |
| Geometry, 1,024 to 65,536 instances, 2 repeats | All passed. Last run rendered 33,554,432 triangles with p99 7.12 ms. Limit not found below the configured cap. Exit 0. |
| Textures, starting at 64, 2 repeats | One pass, one worst-frame breach at 44.93 ms. Unstable at the first rung, so no capacity claim. Exit 1. |
| Scripts, 256 arrivals per tick, 60-second soak | Backlog stayed zero, but RSS rose from 14.32 to 257.75 MiB. Exceeded the 64 MiB growth budget. Exit 2. |
| Renderer rebuild soak, four instances | Three measured windows, about 15 seconds for a 12-second minimum. No retained instance growth and the frame budgets passed. Exit 0. |
| Forced frame-budget failure, four instances | An intentionally impossible p99 budget of 0.000001 ms returned exit 2. This is a negative gate test, not a performance result. |
| Real exterior, radius 1, one run, explicit RSS budget 8,192 MiB | Stopped during startup at a sampled 8,232 MiB. No frame samples, so no exterior frame-time claim. Exit 2. |

The script soak's p99 grew from 1.43 ms in its first window to 6.77 ms in
its last. `Vm::spawn` appends threads, and the current scheduler retains
completed thread entries. This source behavior is consistent with the measured
time and memory growth. The harness does not change thread-result lifetime
semantics. A retention/reclamation policy needs separate engine work.

Raw logs, exact arguments and JSON reports are retained under the worktree's
ignored `performance-results/` directory. The empty-scene prototype run is
excluded. Godot's metric tests passed on 4.4.1 and 4.7.2. No 24-hour soak,
large mod-loadout survey or complete gameplay stress was run.

## Recorded validation, 2026-09-23

Master `79a3d44`, with cell-by-cell exterior streaming (PRF-02) and the cutout
shader literal fix. Release `soe-godot-host` build on the Ryzen 9 7900X and
RTX 5070 Ti machine, Godot 4.7.2, Forward+, windowed. A one-second sampler ran
alongside: no `rustc` process overlapped any run, peak load was 8.1 on 24
threads, and the only other GPU client was a browser. Command:
`python3 tools/performance.py check --backend godot --scenario exterior
--start 1 --repeats 3 --max-rss-mib 8192`, with `SKYRIM_DATA` set to the
installed game.

The block around `WhiterunOrigin` streamed 9 cells, 231 instances and 321,892
triangles, and reached frame sampling in every run.

| Run | p50 | p99 | p99.9 | Worst | Peak RSS | Result |
|---|---|---|---|---|---|---|
| 1 | 0.432 ms | 1.646 ms | 3.693 ms | 69.089 ms | 3,536 MiB | Breach, worst above 33 ms |
| 2 | 0.376 ms | 1.256 ms | 2.420 ms | 5.186 ms | 3,549 MiB | Pass |
| 3 | 0.393 ms | 1.389 ms | 2.447 ms | 4.507 ms | 3,550 MiB | Pass |

Every percentile through p99.9 sits far below the 16.7 ms bar, and peak RSS
fell from the 8,232 MiB startup failure of 2026-09-19 to about 3.5 GiB. Only
the first run had a frame above 33 ms, which suggests a cold shader or
pipeline cache, not yet confirmed. PRF-08 tracks it.

An earlier run the same day shared the GPU with a game and showed two frames
near 100 ms. It is discarded, and this table replaces it.

That earlier attempt also surfaced a shader compilation failure: a cutout
threshold of 255 printed as the int literal `1`, which Godot rejects against
the float alpha. Every material with that threshold rendered without its
shader until the fix.
