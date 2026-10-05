# Performance

The harness is `crates/bench` (binary `soe-bench`) plus `crates/trace`
(scoped spans) and `godot/bench.gd` (frame time). Every number below comes
from one machine and one commit, so a later run on the same machine can be
diffed line by line. Vanilla numbers are a separate problem, see the end.

Capacity sweeps, enforced frame budgets and sustained workload checks are
documented in [performance-limits.md](performance-limits.md). Historical
measurements below remain tied to their recorded revisions.

## Per-system frames, PRF-12

`soe-trace` records fixed-slot exclusive system time with optional Chrome
tracing off. `Game::frame_history` provides headless tick frames and rolling
240-frame statistics. PlayerNode provides presentation frames through
`SOE/<system>_ms` custom monitors in Godot's Debugger Monitors tab, alongside
Godot's built-in process, physics and rendering monitors. F3 in the walkthrough
shows milliseconds and percentages of the wall frame. On exit it prints and
writes `user://soe-systems.txt` with per-system session totals, means, worst
times and rolling p50 and p99. Idle systems report zero.

The tick denominator is tick wall time. The host denominator is wall time
between completed PlayerNode process callbacks, including rendering waits.
Main-thread scope durations include waits inside their calls and do not
measure GPU execution. Details and source paths are in
[trace.md](trace.md), [game.md](game.md) and [godot-host.md](godot-host.md).
Percentiles are computed when queried, outside the simulation tick.

Run the declared overhead gate, which alternates recorded and suspended
eight-tick batches in one warmed 1,000-actor travel/sandbox world, then
asserts the median paired on/off ratio is below 1.01. A second world replays
the same tick count with timing suspended to verify identical actor positions:

```text
rtk cargo test -p soe-game --release --test it prf_12_accumulator_under_one_percent -- --ignored --nocapture
```

The Rust `prf_12_` checks cover complete slots, exclusive nesting, idle
frames, percentile eviction and overlapping game/host snapshots. After the
release extension build and import, run the live monitor and overlay check:

```text
rtk proxy "$SOE_GODOT_BIN" --path godot --resolution 640x360 --script prf_12_monitors_test.gd
```

## Background music and ambience, AUD-12, 2026-10-05

The lead's release installed-data check on master plus R-PRF-7 and R-AUD-2
reported 6,783 ms of main-thread audio in one PRF-12 walkthrough frame when
music first started. That path decoded the complete native xWMA track and
built a WAV inside the frame.

The AUD-12 acceptance limit was fixed at 2 ms before its first run. The check
extends the actual walkthrough, keeps its automatic Bannered Mare music and
ambience start, then changes between two winning installed MUST paths and
returns to the first. Godot 4.4.1 ran windowed at 640 by 360 with software
Vulkan, llvmpipe LLVM 22.1.8. The extension was built in release mode and
imported before the first run. No compiler ran during the final measurement.

| Revision of the audio path | Worst main-thread audio ms | Result against 2 ms |
| --- | ---: | --- |
| Lead's synchronous decode baseline | 6783 | fail |
| Original checkout a40740b8, 20-second AUD-11 walk | 7466.842232 | fail |
| Worker decode, main-thread PCM disposal, first run | 3.860314 | fail |
| Worker decode, main-thread PCM disposal, diagnostic run | 4.186676 | fail |
| Worker decode and PCM disposal | 0.115918 | pass |

Generator startup diagnostics stayed below 0.5 ms in the second failed run.
Long tracks exceeding the cache cap still released their PCM on the main
thread when an outgoing fade finished. Finished, replaced and discarded
buffers now return to the decode worker for disposal. The final run removed
the observed spike without changing the acceptance threshold.

The final session summary covers 306 presentation frames. Audio mean was
0.013251 ms, rolling p50 was 0.009467 ms and rolling p99 was 0.107633 ms.
The acceptance script observed 268 frames after gameplay became ready, four
music starts, one ambience start, 12,582,912 cached PCM bytes and a master
mixer peak of 0.136122. The two transition tracks were
`music/dlc01/explore/soulcairn/palette/mus_soulcairn_guitarviol_a_01.xwm` and
`mus_soulcairn_guitarviol_a_02.xwm` in the same directory. Returning to the
first added no decoder call. The process exited zero after 66.23 seconds,
including Rust-side record discovery and walkthrough startup.

Run `aud_12_installed_walkthrough_music_starts_and_changes_under_two_ms`
from `soe-godot-host`'s `aud_12_background_audio` test target with
`SKYRIM_DATA` and `SOE_GODOT_BIN` set. The script enforces the session maximum,
checks actual mixer signal and exits after the changes. The worker cache
limit is 64 MiB of allocated PCM samples. Active tracks, pending completions,
queued disposal and the current decode are outside that cache budget.
These frame measurements describe this run and are not a portable scheduler
guarantee or a measurement of speaker output.

The existing installed AUD-11 walking test fails its voice-count assertion
on both original checkout `a40740b8` and AUD-12, reporting nine footsteps,
one music start, one ambience start and zero voices in both runs. The
original checkout measured 7,466.842232 ms worst audio, while the AUD-12 walk
measured 13.548742 ms worst audio with synchronous effects retained.
That walking maximum has no finer phase diagnosis.
This separate walking check does not pass the AUD-12 music-transition limit.
The dedicated start/change check above passes that limit. Idle NPC voice
presentation is an inherited AUD-11 gap, and AUD-12 does not add it.
The ordinary audio crate suite passed 81 tests, and the selected existing
host music/ambience checks plus the new playback fixture passed 14 tests.

## Machine

- AMD Ryzen 9 7900X, 12 cores, 24 threads, up to 5.7 GHz
- 61 GB RAM, the game data on the page cache after the first run
- NVIDIA GeForce RTX 5070 Ti, Vulkan 1.4, Godot 4.4.1 Forward+
- Fedora Atomic, release build, the installed Skyrim SE under Steam

## PRF-11 navigation baseline, 2026-10-03

Before changing navigation, `prf_11_installed_bannered_mare_and_whiterun_path_timings`
loaded the five installed masters and queried the first and last triangle
centroids of the largest mesh in each graph. These are single release queries,
with a first query followed by a warm query, rather than frame measurements.

| Graph | Meshes | Triangles | Route nodes | First | Warm |
|---|---:|---:|---:|---:|---:|
| WhiterunBanneredMare | 1 | 318 | 30 | 38.102 us | 28.423 us |
| WhiterunWorld | 44 | 4,705 | 55 | 102.924 us | 68.509 us |

Bannered Mare endpoints were `[-250.43564, 477.0016, 70.85016]` and
`[580.056, -261.84796, 67.956154]`, cost 1914.888. WhiterunWorld endpoints
were `[20497.994, -7813.914, -3635.1064]` and
`[21203.918, -4165.7085, -2998.261]`, cost 2723.3533.
`find_path_between` resolves each endpoint once. Its A* expansion loop does
not call `locate`. The package driver can issue repeated path requests.

## TST-16 game integration test binary, 2026-10-01

The change recorded 440 `soe-game` integration test executables before
the change and one `it` executable after it. On the same machine, warm
`cargo test -p soe-game --no-run` took 341.869 seconds before and 131.178
seconds after. These measurements come from the TST-16 run at commit
`0dfc2cae`, whose temporary build records were removed after the comparison.
The before run had warmed dependencies before timing the test build. The
after run built the consolidated integration test target.

The committed `tst_16_test_names_match_the_pre_consolidation_inventory` test
compares all 2,289 before and after names after removing the `unit::` prefix
and TST-16 guard tests. The retained pre-change installed run
stopped with a failure, and targeted retries recorded two more failed test
cases. The consolidated installed capture passed, so the available records do
not establish matching installed results test for test.

## Parsed plugin cache, 2026-09-29

`crates/esm/tests/fmt_29_cache.rs::fmt_29_installed_skyrim_master_cache_hit_matches_and_times_second_load`
loaded the installed `Skyrim.esm` through `read_plugin_cached` twice with an
empty cache directory before the first call. In the test profile, the first
load parsed and wrote the entry in 15.226 seconds. The second load read and
decoded the cached plugin in 10.743 seconds. Both returned plugins equaled a
fresh parse. The same test checked cache equality for Update.esm, Dawnguard.esm,
HearthFires.esm and Dragonborn.esm. This is one run, including the cache write
on the first load. It is not a release benchmark.

`soe-bench open-data` opens the installed data folder with its archives and
reports the parsed-plugin cache cold and warm separately. On 2026-09-29, a
release run with 24 threads and three samples measured a 4.713 s cold median
(4.596 s minimum) and a 3.173 s warm median (3.091 s minimum). Each cold sample
started with an empty cache directory. The warm samples reused the entries
from the cold run. For comparison, the 2026-09-17 open-data result below was
9.18 s median before the parsed-plugin cache integration.

## How to run

```text
SKYRIM_DATA="<Data>" cargo run -p soe-bench --release -- [--runs N] [--out DIR] <subcommand>
```

Subcommands: `load-order`, `open-data`, `archives`, `cell <editor id>`, `cell-index <editor
id>`, `exterior <editor id>`, `scripts`, `navmesh`, `save`, `overhead <editor
id>`, `trace <subcommand
...>` (writes `soe-bench-trace-<name>.json` for Perfetto or `chrome://tracing`
and a `.txt` summary next to it), `all`. Each measurement prints as
`name value unit`, timings as the median and the minimum of `--runs`
(default 3). The Godot frame benchmark:

```text
SKYRIM_DATA="<Data>" godot --path godot --script bench.gd
```

The first run of a fresh checkout needs `godot --headless --path godot --import`
once, see `docs/godot-host.md`.

## PRF-08 first-use rendering

The 2026-09-23 exterior log recorded a 69.089 ms worst frame on run one,
but did not record its frame number or pipeline counters. The code path left
first-use pipeline compilation outside streaming: `add_instance`,
`add_multimesh` and `lod_instance` uploaded geometry without a surface material,
then assigned an instance material override. The benchmark created its camera
on its first process frame and waited ten frames without a completion check.
Godot identifies instance overrides as unavailable to mesh-stage precompilation.
Material combinations and view-dependent rendering were first exercised after
the load call. Texture decode completes inside `stream_exterior_each` and
`add_cell` uploads textures before returning. The benchmark does not request
collision or assign a streaming focus, so neither collision construction nor
cell streaming runs on its first exterior process frame.

PRF-08 binds bulk materials to mesh surfaces before instancing and preserves
them across placement rebakes. Benchmark and walkthrough loading draw their
configured camera and lights behind a cover, synchronize submitted GPU work,
and wait for three drawn frames and 500 ms with unchanged pipeline counters.
An unsettled load fails at 30 seconds. The covered frames warm first-use
pipelines and uploads before the cover is removed. The helper reports loading
time, its worst frame and compilation counts independently of measurement.
It uses stock Godot 4.4 APIs. The default disk shader cache stays enabled.

This names and removes the deferred first-use path found in the source.
It does not establish that compilation caused the historical 69.089 ms frame,
because that log lacks a frame trace. The windowed rerun must verify the fix.
The helper observes quiet counters, not pending background compiler jobs.
Specialization can resume later. No headless timing is rendering evidence.

Build and import the extension as described in [godot-host.md](godot-host.md),
then run these three windowed exterior radius 1 starts with one command:

```text
rtk proxy env SKYRIM_DATA="/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data" SOE_BENCH_CELL=WhiterunOrigin SOE_BENCH_RADIUS=1 SOE_BENCH_BATCH=1 SOE_BENCH_OCCLUDERS=1 SOE_BENCH_SECONDS=5 SOE_BENCH_P99_MS=16.7 SOE_BENCH_WORST_MS=33 SOE_BENCH_SOAK=0 SOE_BENCH_CAPTURE= XDG_DATA_HOME="$PWD/target/prf-08-user" /var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/tools/godot/Godot_v4.7.2-stable_linux.x86_64 --headless --path godot --script bench.gd -- --cold-starts 3
```

The launcher is headless, while each child uses a window and the project's
renderer with the same executable. Each child opens data and builds the scene
in a fresh process, measures at least 600 frames and five seconds, prints
`godot_run N worst X ms frame F` and a `SOE_SAMPLE` row, then exits before
the next launch. Frame indices start at one for measurement. Loading frame
indices are separate. Capture and reporting are outside measured intervals.
The launcher runs every requested child even after a breach. Exit 2 preserves
any frame-budget failure, exit 1 preserves any invalid run, and exit 0 requires
all children to pass. A soak cannot be combined with cold-start mode.

Cold means fresh process caches, as in performance-limits.md. Disk shader,
driver and operating-system file caches are retained. This command does not
delete the user's caches. `pipelines_before_render`, `pipelines_after_load`
and `pipelines_during_measurement` distinguish load-time compilation from
later compilation. Compare these with `load_render_worst_ms` and the measured
worst frame. Thresholds remain 16.7 ms p99 and 33 ms worst.
The executable acceptance check is
`prf_08_three_windowed_cold_whiterun_runs_under_33_ms`, gated by
`SKYRIM_DATA`, `SOE_GODOT` and `SOE_PRF_08_WINDOWED=1`.
Windowed acceptance is left to the lead in this sandbox.

Local validation on 2026-10-02: the deterministic loading and reporting checks
pass on stock Godot 4.4.1 and 4.7.2. The installed-data headless material check
passes on 4.4.1 with nine cells, 287 batched instances and 805 unbatched
instances, all with surface materials before drawing. Clearing releases both
sets. These are correctness results, with no measured rendering throughput.

References: local Godot pipeline, RenderingServer and ProjectSettings docs,
checked against the [Godot 4.4 pipeline guide](https://docs.godotengine.org/en/4.4/tutorials/performance/pipeline_compilations.html)
and [shader cache setting](https://docs.godotengine.org/en/4.4/classes/class_projectsettings.html#class-projectsettings-property-rendering-shader-compiler-shader-cache-enabled).
The `soe-perf` skill's old frame-monitor description is superseded by this
repository's wall-clock measurement contract.

## PRF-15 core operation baselines

`tools/perf.sh` runs the Divan target `crates/bench/benches/prf_15.rs` in
release mode with 30 samples and a 100 ms minimum per benchmark. Short
operations run in fixed batches, and the stored values divide each batch
median by its operation count. It measures form lookup, winning-override
resolution, navmesh locate and path search, a Papyrus call and native call,
NIF parsing, BC7 DDS decode and a BSA file read. The fixtures are generated
in memory. If
`SKYRIM_DATA` is set, the target also parses an installed NIF and reads the
corresponding file from a mesh archive under the data directory.

The current medians live in `docs/perf-baselines.json`, in nanoseconds, tagged
with host, CPU model, architecture and data mode. Run `tools/perf.sh` to
compare the nine synthetic operation medians on the same machine. When
`SKYRIM_DATA` is set, its two installed asset medians are stored and compared
as well. It exits nonzero when an operation is more than 15 percent slower or a
measurement is missing, or when the baseline belongs to another CPU identity.
After reviewing a measured change, run `tools/perf.sh refresh` to replace the
medians for the current CPU. A refresh writes the current host and CPU
identity into the baseline. Divan 0.1.21 is licensed MIT or Apache-2.0.
`cargo tree -p soe-bench -i divan` shows its dependency
tree. `cargo tree -p divan` includes cfg-if, clap, condtype, divan-macros,
libc and regex-lite. Clap also brings terminal_size and rustix. The benchmark
API and its median table follow the
[Divan 0.1.21 documentation](https://docs.rs/divan/0.1.21/divan/).

## Run of 2026-09-17, branch `perf`

### Top level

| Measurement | Value |
|---|---|
| Load order, five masters, 1 thread | 2.15 s median, 1.67 s min, 545k forms/s |
| Load order, five masters, 24 threads | 1.42 s median, 1.13 s min, 824k forms/s, 1.51x |
| Forms in the table | 1,168,671 |
| Archive index, 23 archives, 172,914 files | 11.1 s median, 8.5 s min, 15.6k files/s |
| Archive read, 1,000 random files, 133 MB | 23 ms, 5.7 GB/s from the page cache |
| Open data (masters plus archives), 1 thread | 9.18 s median |
| Open data, 24 threads for the masters | 6.62 s median, 5.34 s min |
| Stream WhiterunBanneredMare, 1 thread | 0.442 s median, 282k triangles/s |
| Stream WhiterunBanneredMare, 24 threads | 0.400 s median, 312k triangles/s, 1.10x |
| Bannered Mare content | 690 references, 318 statics, 124,924 triangles, 763 surfaces |
| Stream the 3 by 3 exterior block around WhiterunOrigin, 1 thread | 3.28 s, 87k triangles/s |
| Same, 24 threads | 3.30 s, 0.99x |
| Exterior block content | 9 cells, 957 references, 242 meshes, 285,187 triangles |
| Scripts from `Skyrim - Misc.bsa` | 14,026 loaded, 0 failed, 1.83 s including the load order |
| Bind | 28,143 instances, 71,318 properties, 0.137 s |
| Start | 423 quests started, 0.044 s |
| VM hot loop, one thread by contract | 12.7M instructions/s (800k instructions in 63 ms) |
| Save state after bind | 27 ms, 12.97 MB |
| Load state | 72 ms |
| Navmesh graph, Tamriel | 13,011 meshes, 2,271,709 triangles, built in 0.149 s |
| Navmesh local queries, both ends in one mesh | 80.6k queries/s, 1,000 of 1,000 found |
| Navmesh province queries, random pairs across Tamriel | 3.7 queries/s, 40 of 100 found |

Godot, Bannered Mare through the real data path, 300 frames after 10 warm-up frames:

| Measurement | Headless | Windowed, vsync off |
|---|---|---|
| load_data | 6.55 s | 5.79 s |
| stream_cell | 0.455 s | 0.399 s |
| Mesh nodes built | 763 | 763 |
| Objects drawn per frame | 0 | 763 |
| Primitives drawn per frame | 0 | 124,924 |
| Process step, median | 0.03 ms | 56.1 ms |
| Physics step, median | 0.02 ms | 0.22 ms |
| Render setup CPU, median | 0 | 4.0 ms |

Headless has no renderer, so its process time is the scene tree alone. The
windowed frame is 56 ms with 763 separate `MeshInstance3D` nodes and one
material each. That is the first rendering number the project has and it is
poor for a single interior: the surfaces are unbatched, so the next step on
the host is merging static surfaces per material into one mesh or a
`MultiMesh`, and measuring again.

### Where the time goes

From `soe-bench trace cell WhiterunBanneredMare` (16,617 events over the
3 plus 3 open and stream runs):

| Span | Count | Total ms | Mean | Note |
|---|---|---|---|---|
| host.open | 6 | 45,960 | 7.66 s | opening the data folder |
| host.index_archives | 6 | 35,264 | 5.88 s | 77 percent of open |
| bsa.archive_open | 102 | 34,980 | 343 ms | 17 archives per open, the two texture archives take over a second each |
| esm.plugin_parse | 30 | 11,288 | 376 ms | Skyrim.esm 1.1 to 1.3 s, the others under 150 ms |
| host.parse_masters | 6 | 9,548 | 1.59 s | the threaded parse, bounded by Skyrim.esm |
| esm.load_order_add | 30 | 1,147 | 38 ms | form table insert |
| host.stream_cell | 6 | 2,221 | 370 ms | |
| host.scan_references | 6 | 2,164 | 361 ms | 97 percent of stream: a linear pass over every REFR in the load order |
| host.build_all | 6 | 56 | 9.4 ms | NIF parse plus mesh build, all 318 statics |
| host.static | 1,908 | 125 | 66 us | per static, across the workers |
| nif.parse | 1,908 | 59 | 31 us | 47 MB of NIF bytes |
| host.build_surface | 4,578 | 24 | 5 us | |
| bsa.vfs_read | 1,908 | 29 | 15 us | |

Two conclusions follow. First, streaming a cell is not the mesh work at
all: 361 of 370 ms is `scan_references` walking the whole reference table
to find the 690 references in one cell. An index from cell to its references,
built once at open, turns that into a lookup, after which the threaded mesh
path (9 ms for the whole cell) is what remains. Second, opening the data
folder is the archive index, and inside it the texture archives. The bench
does not need textures, and a lazy archive index (open on first read) would
cut open from 6.6 s to about 1.5 s.

From `soe-bench trace scripts` (105,313 events):

| Span | Count | Total ms | Note |
|---|---|---|---|
| esm.plugin_parse | 20 | 7,357 | four load orders for the four runs |
| esm.load_order_add | 20 | 705 | |
| vm.run | 256 | 148 | 2.4M instructions over start plus the hot loop |
| vm.slice | 37,961 | 140 | 3.7 us per 64-instruction slice |
| game.bind | 1 | 93 | |
| game.bind_form | 23,443 | 89 | 3.8 us per form with scripts |
| bsa.vfs_read | 42,078 | 21 | 0.5 us per script read |
| game.pump | 1 | 0.5 | 611 queued events and calls after start |
| native calls | 1,183 | 0.06 | every one named `<class>.None` |

The native rows are a finding, not a measurement: during `start` the VM
reaches `call_native` 1,183 times with the function name `None` and walks
the whole class chain each time (script names appear as classes), which
means a method-call operand decodes to `None` somewhere in the start path,
most likely `CallParent` or a call on a `None` object. No real native by
name shows up in the trace at all, so the 423 quest starts ran their
`OnInit` bodies without any native reaching the game crate. That is a VM
bug the trace exposed, filed in `docs/papyrus.md` under known gaps and left
for a VM change, since this branch adds spans only.

### Threads

| Phase | 1 thread | 24 threads | Speedup | Verdict |
|---|---|---|---|---|
| Parse the five masters | 2.15 s | 1.42 s | 1.51x | pays, bounded by Skyrim.esm at 1.1 s on one thread |
| Bannered Mare, parse and build | 0.442 s | 0.400 s | 1.10x | the mesh work is 9 ms, the scan is single threaded |
| Exterior 3 by 3 block | 3.28 s | 3.30 s | 0.99x | same reason, nine scans of the reference table |
| VM | one thread | one thread | | by contract, scripts see one scheduler |

The threaded paths are `std::thread::scope` over chunked work lists, no
runtime. Threads only pay where the trace shows the time, and for cell
streaming it shows it in the scan, so the index comes first.

### Tracing overhead

Bannered Mare streamed with the switch off and on, 3 runs each, 24 threads:

| | Off | On | Ratio |
|---|---|---|---|
| Stream median | 0.374 s | 0.368 s | 0.98x |
| Events per run | 0 | 2,751 | below the run-to-run noise |

A span costs two `Instant::now` calls and one push into the global list
under its mutex. At 2,751 events per cell the difference is inside the
noise between runs. A first design buffered per thread and flushed on
thread exit, which measured 1.10x and lost the tail of a scoped worker's
events when the join returned before the thread-local destructor ran, so
the global list stayed. With the switch off a span is one relaxed atomic
load.

## Run of 2026-09-18, branch `batching`

Same machine, release build without the `lto = "fat"` and `codegen-units = 1`
profile `master` gained the same day, so every before and after pair here
was measured inside this worktree. The branch changes the Godot layer
(batching, `RenderingServer` instances, compressed texture upload), the
material path (a texture cache on `DataSource`, reads on the workers, no
BCn decode) and adds exterior streaming with terrain. The design is in
`docs/godot-host.md`, the section of the same date.

### A correction to the 2026-09-17 frame numbers

The 56 ms "process step" for the Bannered Mare above was
`Performance.TIME_PROCESS`, which this run showed reading a constant 80 to
113 ms across every frame of a scene that the wall clock draws in half a
millisecond. It is not a per-frame number. `godot/bench.gd` now records the
wall clock between two `_process` calls and the viewport's measured GPU and
CPU render time, over at least five seconds and 600 frames, as p50, p99,
p99.9 and worst. The 763 unbatched nodes were never the frame's cost.
"Where the time goes" above stands, its Godot row does not.

### Frame time, windowed, 1600 by 900, vsync off, X11 display driver

| Scene | Instances | Frame p50 | p99 | p99.9 | Worst | GPU p50 | Render CPU p50 | Frames/s |
|---|---|---|---|---|---|---|---|---|
| Bannered Mare, one instance per surface | 763 | 0.480 ms | 0.950 | 1.492 | 5.651 | 0.081 ms | 0.177 ms | 1,996 |
| Bannered Mare, batched per material | 48 | 0.235 ms | 0.397 | 0.903 | 2.282 | 0.062 ms | 0.039 ms | 4,004 |
| Whiterun 3 by 3 (9 cells, 303,619 triangles), per surface | 802 | 0.758 ms | 1.934 | 2.749 | 3.747 | 0.093 ms | 0.280 ms | 1,098 |
| Whiterun 3 by 3, batched | 195 | 0.288 ms | 0.519 | 1.045 | 1.698 | 0.087 ms | 0.066 ms | 3,284 |
| Whiterun 5 by 5 (25 cells, 624,408 triangles), per surface | 1,499 | 0.889 ms | 1.782 | 3.013 | 7.567 | 0.156 ms | 0.454 ms | 1,058 |
| Whiterun 5 by 5, batched | 390 | 0.359 ms | 0.686 | 1.319 | 3.191 | 0.142 ms | 0.105 ms | 2,713 |
| WhiterunWorld, every cell (59, 825,587 triangles), batched | 563 | 0.440 ms | 0.935 | 1.576 | 2.246 | 0.171 ms | 0.147 ms | 2,090 |

Draw calls equal instances. The frame wall clock is presentation bound at
these sizes (Xwayland), the GPU and render CPU rows are the scene. Batching
buys a 4x on render CPU and about 2x on the frame. The GPU row grows with
triangles, 0.06 ms per 125k to 0.17 ms per 825k, and would need about five
million triangles in view to reach one millisecond on this GPU. The
`Performance.TIME_PROCESS` row is gone.

Godot 4.7.2 on the same 3 by 3 block, batched: frame p50 0.276 ms, p99
0.498, p99.9 1.071, worst 1.677, GPU 0.084 ms, render CPU 0.063 ms on the
single-safe model, and 0.281, 0.492, 1.110, 1.849, 0.084, 0.064 on the
separate render thread. Unbatched on 4.7.2: 0.517, 1.193, 1.675, 22.399,
0.091, 0.196. Batched, the binaries are within noise of each other,
unbatched 4.7.2 has the lower median and the session's one 22 ms hitch, see
`docs/godot-host.md`.

Against the bar `ENGINE-PLAN.md` set for the 3 by 3 block (p99 at or under
16.7 ms, worst hitch under 33 ms, a cell streamed in under 250 ms on
workers): the frame criteria pass at 0.519 ms and 1.698 ms batched (1.934
and 3.747 unbatched), the stream criterion fails on this branch at 406 ms
per cell, 356 of them the reference scan that `master`'s cell index
replaces. Measure again after the merge.

### Stream time, Rust side, `soe-bench cell WhiterunBanneredMare`

| | 2026-09-17 (`perf`) | 2026-09-18 (`batching`) |
|---|---|---|
| Stream, 1 thread, median | 0.442 s | 0.402 s |
| Stream, 24 threads, median | 0.400 s | 0.391 s |
| `host.material` total over 6 streams | 1,954 ms (97 percent, `master` trace of 2026-09-18) | 46 ms (2 percent) |
| `host.texture_decode`, cold reads | decoded every stream on the calling thread | 88 reads, 44.7 ms summed over the workers, once per source |
| `host.scan_references` mean | 361 ms | 356 ms (the `cell-index` branch replaces it, not carried here) |

The 1,954 ms row is the lead's trace of `master` after `cell-index` landed,
where the material path was all that remained. Here a block-compressed
texture is never decoded, its BC blocks and mip chain are copied out of the
file and uploaded as they are, and the copy happens on the worker thread
that parsed the NIF, into a cache the `DataSource` owns. So the first stream
of a cell pays 45 ms of reads spread over the workers and a second stream of
any cell that shares its textures pays nothing. On this branch the stream
is the reference scan again. With the index from `master` on top of this
branch, the Bannered Mare should stream in about 20 ms warm.

Godot side, `stream_cell` in `bench.gd` (data path plus upload): 1.11 to
1.19 s with the RGBA8 decode and a generated mip chain, 0.50 to 0.52 s with
the compressed upload. Texture memory 592 MB (RGBA8, no mips), 783 MB (RGBA8
with generated mips), 175 MB (the files' own blocks and chains).

### Spans added

`host.batch` (payload: surfaces in), `host.add_cell`, `host.upload_mesh`
(payload: vertices), `host.upload_texture` (payload: pixels),
`host.texture_decode` (a cache miss, read plus copy or decode),
`host.scan_cells`, `host.scan_land`, `host.terrain_surface`,
`host.stream_exterior`. `host.material` now wraps the cache lookup on the
worker.

## Vanilla reference points

We do not have vanilla numbers for any row above. Nothing here says the
engine is faster or slower than Skyrim SE. What we can measure of vanilla,
and how:

- Loading screen times: a Proton launch of Skyrim SE with
  `[Papyrus] bEnableLogging=1 bEnableTrace=1` in `Skyrim.ini` writes
  timestamped `Papyrus.0.log` lines. The first `OnInit` timestamp minus the
  process start gives cold start to script start. Coc into
  WhiterunBanneredMare from the main menu (`coc WhiterunBanneredMare` in the
  console) and read the `OnCellLoad` or `OnLoad` timestamps for the cell load
  time.
- Frame time: `ENB` or `MangoHud` under Proton for a frame time histogram in
  the Bannered Mare and on the Whiterun plains, on this GPU, at the same
  resolution the Godot window uses.
- Script throughput: no vanilla counter exists. A script with a counting loop
  and `Debug.Trace` timestamps at the start and end gives instructions per
  second within the log's second granularity.
- Save size and time: the vanilla `.ess` for the same point, from the Saves
  folder, with its file size and the loading screen timing.

Until a session captures those, the tables above are internal baselines for
regression checks, not a comparison with the shipped game.

## Run of 2026-09-18, branch `cell-index`

The reference scan is gone. `DataSource::open` builds `soe_esm::CellIndex`
once (the `esm.cell_index_build` span) and `stream_cell_id` walks the ids the
index files under the cell instead of every `REFR` in the load order. Same
machine, release build, `--runs 3`. New subcommand: `cell-index <editor id>`.

| Measurement | Before (2026-09-17) | After | Note |
|---|---|---|---|
| Cell index build | | 0.309 s median, 0.273 s min | 872,220 placed objects, 2.8M objects/s, once per open |
| One `references` lookup | | 9.9 ns | 1M lookups of the Bannered Mare |
| `host.scan_references`, Bannered Mare | 361 ms per stream | 0.5 ms per stream | 697 ids walked instead of the whole table |
| Stream WhiterunBanneredMare, 1 thread | 0.442 s | 0.320 s median, 0.306 s min | 391k triangles/s |
| Stream WhiterunBanneredMare, 24 threads | 0.400 s | 0.301 s median | 416k triangles/s |
| Stream the 3 by 3 block around WhiterunOrigin, 1 thread | 3.28 s | 4.06 s | 957 references, 285,187 triangles, see below |
| `host.scan_references`, exterior block | nine scans | 2.0 ms over 18 streams | |

The stream did not drop to the 9 ms of mesh work the earlier trace promised,
and the exterior block got slower. Both have the same cause: the 2026-09-17
numbers came from branch `perf` before the materials branch merged, and
texture decode now fills the time the scan used to. From
`trace cell WhiterunBanneredMare` (21,729 events over 3 plus 3 runs):

| Span | Count | Total ms | Mean | Note |
|---|---|---|---|---|
| host.stream_cell | 6 | 2,005 | 334 ms | |
| host.material | 4,578 | 1,954 | 427 us | 97 percent of stream: `Vfs::read` plus `Dds::decode`, 88 textures per stream |
| esm.cell_index_build | 6 | 1,676 | 279 ms | once per open, not per stream |
| bsa.vfs_read | 2,436 | 319 | 131 us | texture reads dominate, mesh reads were 15 us each |
| host.build_all | 6 | 46 | 7.7 ms | the mesh work, unchanged |
| host.scan_references | 6 | 3.0 | 0.5 ms | was 361 ms |

And from `trace exterior WhiterunOrigin` (one run, 18 streams):

| Span | Count | Total ms | Note |
|---|---|---|---|
| host.stream_cell | 18 | 9,219 | |
| host.material | 1,586 | 9,166 | 99 percent, exterior textures are larger than interior ones |
| bsa.vfs_read | 1,144 | 1,091 | |
| host.build_all | 18 | 50 | |
| host.scan_references | 18 | 2.0 | |

So the next hot spot is texture decode, not the index and not the mesh path.
`MaterialCache` lives for one `stream_cell_id` call, so every stream decodes
its textures again, and the decode runs on the calling thread after the mesh
workers join. A texture cache on `DataSource` that outlives the call, and
decoding on the worker threads, are the two obvious moves, in that order.
The thread speedup rows stay near 1.0x for the same reason: the timed work
is now single-threaded texture decode.

The exterior block's 957 references and 285,187 triangles are the cells'
own temporary children only. WhiterunWorld's persistent cell holds another
495 placed objects (doors, markers, the city's persistent statics) that the
index keeps apart under `world_persistent_cell`, an exterior loader has to
add those and filter them by position.

## After the round 3 batch (2026-09-18, master 0874286 and later)

The cell index and the batching branch merged the same day, measured together on master with
`soe-bench --runs 3`, release profile with fat LTO and line tables, 24 threads.

| Measurement | Before the batch | After |
|---|---|---|
| Bannered Mare stream, 24 threads, median | 0.400 s | 0.004 s |
| Bannered Mare stream, 1 thread, min | 0.442 s | 0.012 s |
| Whiterun 3 by 3 block, 24 threads, median | 4.06 s | 0.013 s |
| Whiterun 3 by 3 block, 1 thread, median | 3.28 s | 0.022 s |

The exterior block keeps its 957 references, 242 meshes and 285,187 triangles. Against the bar
fixed before the numbers arrived (p99 at or under 16.7 ms, worst hitch under 33 ms, a cell under
250 ms on workers): frame p99 0.519 ms and worst 1.698 ms on the batched 3 by 3 block, both pass,
and the cell stream passes once the index and the compressed texture path are on the same tree.
The 56 ms frame figure recorded earlier was `Performance.TIME_PROCESS`, which reads a constant 80
to 113 ms on a scene the wall clock draws in 0.48 ms, so nodes were never the frame cost. The
host question is settled for now: Godot at this scale is not the bottleneck.

## Explicit timing gates (2026-09-19)

Each timed phase now reports median, minimum, nearest-rank p99, maximum and sample
count. The p99 of fewer than 100 samples is the maximum, not evidence about a
population tail. Most phases use `--runs`, but the province navigation cohort
still runs once. Its samples describe the complete 100-query cohort, not
individual query latencies.

Repeat `--max-seconds metric=value` before the subcommand to set explicit upper
bounds, for example:

```text
SKYRIM_DATA="<Data>" soe-bench --runs 100 --max-seconds cell_stream_WhiterunBanneredMare_1_thread_p99=0.05 cell WhiterunBanneredMare
```

Use the exact printed metric name. Invalid, duplicate and unmeasured names fail
the command, as do exceeded limits. Choose machine-specific limits before the
first measured run. No measured number in this report automatically becomes a
threshold.

Navigation also reports lazy point-index construction separately from repeated
1,000-point lookup cohorts. The synthetic CPU stress harness counts allocation
and reallocation requests per timed tick in an isolated child process. It does
not alter the installed-game timings below and does not measure live heap size.

The cell benchmark reports its cold texture-cache stream separately, then
compares warm-cache one-worker and multi-worker streams. Each timing sample
drops the previous result before starting its timer, so startup measurements
do not retain two complete archive sets. A unit test checks the drop order.

### Audit measurements, 2026-09-19

Baseline `75cfb55` and the audit branch used the same installed five-master load
order and Cargo `thin` profile. Before and after processes ran sequentially.
Other project work was active on the machine, and startup timings varied.
These are observations, not portable thresholds.

| Workload | Baseline | Audit branch |
|---|---:|---:|
| 100 fixed province path queries | 27.934 s | 6.135 s |
| Paths found in that cohort | 40 | 40 |
| 1,000 local path queries, median of 3 | 0.013 s | 0.010 s |
| Local paths found | 1,000 | 1,000 |
| 24-worker startup, separate process pair 1 | 9.168 s | 3.776 s |
| 24-worker startup, separate process pair 2 | 9.352 s | 6.991 s |
| 24-worker startup, separate process pair 3 | 5.769 s | 3.449 s |

The province cohort is 4.55 times faster in this comparison, with the same
reachability count. It calls `find_path` directly, so the gain is from component
rejection and stale-queue handling, not the point index. The new point-query
cohort found 1,000 of 1,000 points in 0.008 s after a separately measured
0.016 s lazy index build. There is no matching old-harness point-query timing.

The startup samples have medians of 9.168 s before and 3.776 s after, but the
one-worker control samples also varied widely. All paired threaded samples
improved, which supports retaining bounded parallel archive opening. A repeat
on an idle machine is needed for a stable regression threshold.

Every cell run retained 690 references, 318 parsed meshes, zero failed meshes,
124,924 triangles and 763 surfaces. Streaming numbers from the old harness are
not compared because its first one-worker stream had a cold texture cache and
its multi-worker stream had a warm cache.

Persistent worker pools and lazy archive payload storage remain unimplemented.
They need separate measurements of worker startup, first-use latency and peak
memory. The failed repeated startup run that retained two full archive sets is
not included in the table. The allocation-count gate covers synthetic CPU
scenarios only, as documented in [performance-limits.md](performance-limits.md).

## Persistent data-source workers, PRF-05

The worker pool change was measured on the same machine and with the release
`soe-bench cell WhiterunBanneredMare` command, five warm stream samples at one
and 24 workers. The baseline ran before the pool change, then the command ran
again after it. Both runs reported 690 references, 318 statics with models,
393 meshes parsed and no failed meshes.

| Measurement | Scoped workers before | Persistent workers after |
|---|---:|---:|
| Warm stream, 1 worker | 20 ms | 22 ms |
| Warm stream, 24 workers | 6 ms | 5 ms |

The focused startup test measured 21 repeated batches of 512 integer items.
Its scoped-worker median was 100 microseconds per batch. Creating the four
thread pool and running its first batch took 344 microseconds, while later
batches on that pool had a 30 microsecond median. This synthetic measurement
isolates worker creation. It is not a substitute for the real-cell stream
measurements above. The stream medians are close to the observed run-to-run
variation, so this sample does not show a reliable stream-time change. The
gated debug test also streamed Bannered Mare five times with 24 workers and
recorded a 50 ms median. That figure is kept separate from the release
benchmark.

## Cold interior texture preparation, 2026-09-20

HST-06 fixed a 50 ms cold texture preparation budget before its first timing
run. OpenMW's `apps/openmw/mwgui/loadingscreen.cpp` uses the same initial delay
before drawing a loading screen. The earlier Bannered Mare trace above reported
44.7 ms of cold texture work, which put the declared boundary just above that
known sample.

The installed-data test clears `DataSource`'s prepared texture entries before
each cell while retaining its load order and VFS. Cold therefore means a miss
in the engine texture cache. It does not mean an empty operating-system page
cache. The release test used one worker and one allowed CPU because a parallel
data-source open peaked above the available memory during concurrent project
work. `preparation` is the sum already recorded by
`CellStats::texture_decode_time`. It is cumulative preparation work and can
exceed elapsed wall time when cell work runs in parallel. This corpus fixed
the worker count at one. `wall` includes the rest of `stream_cell_id`, but
excludes Godot resource upload and collision building.

| Fixed interior | References | Textures | Preparation ms | Wall ms | Fade |
|---|---:|---:|---:|---:|---|
| MarkarthSilverBloodInn | 794 | 114 | 48.417 | 75.657 | no |
| WhiterunBanneredMare | 690 | 118 | 68.865 | 89.433 | yes |
| RiftenBeeandBarb | 619 | 111 | 69.926 | 86.826 | yes |
| FalkreathDeadMansDrink | 349 | 69 | 75.502 | 86.355 | yes |
| BleakFallsBarrow01 | 2,465 | 143 | 78.721 | 230.207 | yes |
| SolitudeWinkingSkeever | 975 | 146 | 90.991 | 115.431 | yes |
| MorthalMoorsideInn | 348 | 81 | 97.145 | 105.614 | yes |
| WinterholdTheFrozenHearth | 326 | 67 | 98.595 | 106.962 | yes |
| WindhelmCandlehearthHall | 700 | 134 | 101.570 | 123.862 | yes |
| DawnstarWindpeakInn | 353 | 83 | 106.797 | 115.210 | yes |
| WhiterunDragonsreach | 1,432 | 174 | 158.486 | 193.273 | yes |
| RiverwoodSleepingGiantInn | 481 | 97 | 241.598 | 254.647 | yes |

Nearest-rank preparation percentiles are 90.991 ms at p50 and 158.486 ms at
p90, with a 241.598 ms maximum. The test completed in 18.65 seconds after the
release build and asserted the fade decision for every sample. Its monitored
process peaked at 14,556,104 KiB RSS during data-source open, then exited zero.

The separate headless Godot gameplay acceptance measured 367.029 ms of cold
outbound preparation and activated the overlay. Warm return and input
crossings each measured 0 ms and left it inactive. The process exited zero
after 46.0 seconds, with a peak RSS of 15,273,864 KiB.

## Deterministic operation budgets, PRF-14

`soe_trace::WorkFrame` captures operation counts without using elapsed time.
Game ticks retain their last counts through `Game::last_work_counts`.
`PlayerNode.work_counts` includes its enclosing host advance. Explicit work
windows cover setup or queued work outside a tick. Nested ticks contribute
once to their enclosing window. Data-source workers enter the caller's
recorder before processing a chunk and join before the snapshot.

Every fixed test compares all ten counters, including zero limits. These
limits are fixed before running the scenarios:

| Scenario | Locate | Triangles | Path nodes | Instructions | Natives | Conditions | Rays | Shapes | Records | Instances |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| AI-25 orbit, 8,192 triangles, each of 60 ticks at 0.1 s | 2,000 | 8,000 | 256 | 0 | 0 | 0 | 0 | 0 | 2,048 | 0 |
| PRF-07, 1,000 actors, ten warm-up ticks then 120 ticks at 60 Hz | 8,000 | 16,000 | 4,000 | 0 | 0 | 0 | 0 | 0 | 200,000 | 0 |
| Mixed stress, 16 actors, scripts and paths, each of five ticks | 128 | 256 | 8,192 | 544 | 0 | 0 | 0 | 0 | 10,000 | 0 |
| Burst of 32 quest stages, fragments and OR conditions | 0 | 0 | 0 | 96 | 32 | 32 | 0 | 0 | 8,192 | 0 |
| One installed Bannered Mare NAVM point query | 1 | 256 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

The orbit fixture keeps AI-25's authored bounds, height, arc samples and
movement. Triangle candidates use the graph's bounds tree at triangle level.
A full scan of its 8,192 triangles for every orbit sample exceeds its
triangle limit by hundreds of times. The gate checks work rather than CPU
speed. A* remains synchronous, with no scheduling or path-budget change.

The stress CLI prints `work_peak_<operation> <count> count` after each
measurement window. These are per-counter maxima across measured ticks,
excluding setup, warm-up and report work. Scope instrumentation is independent
of the global timing trace switch. Counter definitions and worker scope
behavior are in [trace.md](trace.md). The fixed tests are in
`game/tests/ai_25_orbit.rs`, `game/tests/prf_14_work.rs` and
`bench/src/stress/mod.rs`. The installed NAVM case uses all five masters.
The Godot correctness test loads the Bannered Mare, checks actual RID creation
counts against streamed instances, and verifies a new empty window is zero.
It does not measure renderer throughput.

### PRF-11 after triangle indexing and sliced A*

The same installed graphs and endpoints measured 40.387 us first and 25.539 us
warm for Bannered Mare, and 93.216 us first and 60.665 us warm for WhiterunWorld.
The route node counts and costs remained 30 / 1914.888 and 55 / 2723.3533.
The warm before/after pairs are 28.423 / 25.539 us and 68.509 / 60.665 us.
These single-query timings do not establish a general speedup. The frame
scheduler bounds work independently: 64 nodes per request and 512 across
requests per frame. Pending requests retain their heap and predecessor state.

An intermediate Godot 4.4.1 headless run, with triangle indexing and sliced A*
but before the closed-door lookup index, completed 3,600 frames and failed
all 3,600 against the fixed 8 ms limit. Seven contact NPCs were present.
It reported 53,030 package evaluations and a worst AI tick of 154.985 ms.
A three-frame diagnostic then measured 1.235 ms of door refresh within a
43.023 ms first AI tick, with 64 instantiated actors. This evidence led to
indexing closed-door blocking by triangle, reusing the existing cell reference
index and narrowing combat candidates through the existing awareness table.

### PRF-11 full-phase diagnosis, 2026-10-03

The continuation measured the complete AI tick and nested phases on Godot
4.4.1, release build, five installed masters, headless Bannered Mare,
3,600 ticks at 60 Hz. The first diagnostic run reported seven contact actors,
24,208 evaluations, 295 ticks above 8 ms and a 13.199 ms maximum. The finer
second run reported 1,180 failures and a 13.709 ms maximum. These runs were
sequential and their wall-clock maxima varied.

At frame 600 of the finer run, total actor processing took 6.571 ms.
Request setup used 2.116 ms and route-result fallback used 3.496 ms.
The first tick spent 9.414 ms scanning sandbox markers. Resumed search work
was below one microsecond in these samples, so the node budget did not cover
these costs. The previous two-phase report exposed none of them.

Nearest lookup now carries the existing best radius into each mesh's triangle
tree. The synthetic overlapping-mesh regression failed before the fix after
testing all 80,002 triangles, and passes after testing two. Boolean containment
stops at the first match rather than visiting every overlapping interior.
With these query changes, frame 600 fallback time fell to 22.683 us. That
intermediate run still failed six early ticks, with a 14.683 ms maximum.

Marker filtering now reads only the live base or the winning NAME, XLCN and
cell fields, sharing the reference initializer's BOS base selection. It avoids
full placement decoding and inventory or actor-template initialization for
irrelevant references. This changes no saved fields and adds no dependencies.

The final marker subphase diagnostic found 5.742 ms in base filtering,
3.375 ms in cell-reference lookup and 1.267 ms in bed classification during
the first tick. Reference initialization accounted for 0.508 ms. That run
failed three ticks and reached 15.404 ms. The candidate scan therefore gained
its own retained cursor, with 64 reference examinations per request and 256
across the frame. Pending actors keep their step until candidates are ready.
The query still returns candidates in FormID order before choosing with RNG.

The first host run with sliced marker queries reported one failure at frame
1561, reaching 12.319 ms. Its failing frame did not record nested phases.
The script now prints phase details for failures. A repeat on the same
extension did not reproduce it. Its cause is unknown. That repeat exited zero
and measured a 5.885 ms maximum, with all 3,600 frames below 8 ms. The original
71 percent failure rate is gone in these runs, but this is a measured check
on this machine, rather than a portable wall-clock guarantee.

```text
PRF-11 actors=7 frames=3600 seconds=60 evaluations=24900 worst_ai_us=5885.331 failures=0
```

The final run's first tick spent 1.844 ms in cell-reference lookup and
1.874 ms in the entire new marker request phase. At frame 1800, request setup
used 3.097 ms, fallback used 28.409 us, marker request work used 138.662 us
and resumed marker work used 0.471 us. Package and actor state keep their
existing saved layout. No actor scene-node path was added.

The installed-data release Rust checks also passed both 60-second Bannered
Mare runs, with maxima of 1.560 ms and 2.696 ms. All 24 PRF-11 Rust tests passed.
The final path survey retained both baseline corridors and costs. Bannered
Mare measured 35.437 us first and 23.705 us warm, versus 38.102 us and
28.423 us before indexing. WhiterunWorld measured 89.209 us first and
57.709 us warm, versus 102.924 us and 68.509 us before indexing.
The 498 library tests across game, navmesh and godot-host passed. Package
clippy with all targets and warnings denied passed for all three crates.

### PRF-11 repeated-request diagnosis, pass 3

The lead's pass 2 release check reported 637 failing frames and a 24.893 ms
maximum. Additional actor-driver phases separate actor state, Calm and
evaluation eligibility from movement and its existing nested query timings.
The initial pass 3 diagnostic, before changing request reuse or door refresh,
reported 778 failures and a 39.097 ms maximum over 3,600 frames. It used Godot
4.4.1 headless with the installed five-master gameplay load order.

At frame 600, movement used 7.088 ms, including 6.254 ms in request setup.
At frame 2400, setup used 8.461 ms of 9.092 ms in movement. A failed route was
discarded on completion, so unchanged actors resolved the same endpoints
again on later ticks. At frame 304, a sandbox request also spent 4.361 ms
collecting and filtering cell references. Door refresh ranged from 1.257 to
2.060 ms at the periodic samples. The worst frame, 2289, spent 19.516 ms in
request setup, 5.260 ms in procedure-tree entry and 6.034 ms in door refresh.
These are inclusive wall-clock phases, including time spent descheduled.

Completed failures now remain in the transient planner until their inputs or
the NAVM graph revision change. Unchanged requests perform no new query work.
Door refresh reads live references each tick and reuses unchanged footprint
bindings. Neither cache changes saved actor state or adds a scene-node path.

With route reuse and unchanged-door filtering, periodic samples fell to 23.136
us of setup and 129.645 us of door refresh at frame 600. That run overlapped
the worktree's Rust test build. It still failed 184 frames and reached 118.969
ms, with 81.543 ms in package selection on its worst tick. Its first tick
also spent 7.142 ms collecting cell references, outside the marker scan budget.
Sandbox request setup now reads only index slice lengths, and each bounded
scan merges the sorted persistent, temporary and deleted slices. It no longer
copies, filters and sorts the whole cell before its first slice.

The installed Rust checks passed their three 60-second runs at 967.218,
698.921 and 749.527 us maximum, but the cold first-tick regression failed at
699.073 ms. Ordinary cell loading had left the world NAVM graph lazy.
`World::load_cell` now prepares the graph after reference prefetch and before
attachment, so this work occurs during loading rather than the first AI tick.

The navmesh test command reported 83 passes with `SKYRIM_DATA` unset. A separate
installed path survey passed on both graphs. Bannered Mare measured 34.095 us
first and 24.877 us warm, and WhiterunWorld 91.253 us first and 60.033 us warm.
Both baseline corridors and costs remain unchanged. The warm before/after
pairs are 28.423 / 24.877 us and 68.509 / 60.033 us.

After moving preparation into loading, all 173 game library tests passed.
The installed integration selection passed 83 tests, including the cold
first-tick check, all PRF-11 timed runs, door invalidation and cell transitions.
The three Bannered Mare 60-second maxima were 703.269, 699.803 and 724.780 us.
Package Clippy passed for game, navmesh and godot-host with all targets and
warnings denied. No saved fields, dependencies or actor scene nodes were added.

The final release extension passed the same Godot 4.4.1 headless check, with
no compiler running in this worktree during measurement. The process exited
zero. First-tick cell-reference work was 3.778 us. At frame 600, request setup
was 11.439 us and door refresh 59.322 us, compared with 6.254 ms and 1.899 ms
in the initial pass 3 diagnostic. The worst tick was frame 1800 at 1.756 ms,
including 1.582 ms of actor processing, 264.609 us of request setup and
153.110 us of door refresh. All 3,600 samples met the unchanged 8 ms limit.
These are wall-clock observations on this machine, not a portable scheduling
guarantee. Earlier failed diagnostic runs remain recorded above.

```text
PRF-11 actors=7 frames=3600 seconds=60 evaluations=24900 worst_ai_us=1756.340 failures=0
```
