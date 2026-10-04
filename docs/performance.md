# Performance

The harness is `crates/bench` (binary `soe-bench`) plus `crates/trace`
(scoped spans) and `godot/bench.gd` (frame time). Every number below comes
from one machine and one commit, so a later run on the same machine can be
diffed line by line. Vanilla numbers are a separate problem, see the end.

Capacity sweeps, enforced frame budgets and sustained workload checks are
documented in [performance-limits.md](performance-limits.md). Historical
measurements below remain tied to their recorded revisions.

## Machine

- AMD Ryzen 9 7900X, 12 cores, 24 threads, up to 5.7 GHz
- 61 GB RAM, the game data on the page cache after the first run
- NVIDIA GeForce RTX 5070 Ti, Vulkan 1.4, Godot 4.4.1 Forward+
- Fedora Atomic, release build, the installed Skyrim SE under Steam

## Startup quest candidate index, PRF-33, 2026-10-04

`crates/game/examples/startup_timing.rs` adapts the lead's 2026-10-03 host
harness from the external `crash-1004-1945/wt2.patch` run log. It times opening
the five masters and script archives, constructing Game, reading and
registering PEX, binding,
each start-game-enabled quest and script settling. It opens the script VFS
directly so this game-crate example needs no host dependency or rendering
assets. Its opening time is not a `DataSource::open` measurement.

The comparison ran sequentially in this worktree with the release profile,
fat LTO and `--trace`, before and after the candidate index change. Both runs
loaded 14,026 scripts, bound 18,225 attachments and attempted 423 startup
quests, of which 421 started. The two required-alias failures stayed failures.
The lead's earlier play-session overlay had 14,302 scripts and recorded
12.8 to 14.0 seconds starting quests. The table below is this comparison's
own before and after pair, with times in milliseconds.

| Measurement | Before | After |
|---|---:|---:|
| Startup quest loop, total | 15,370.476 | 4.423 |
| Per quest, median | 36.218251 | 0.001993 |
| Per quest, nearest-rank p99 | 39.872904 | 0.017263 |
| Per quest, maximum | 42.742893 | 0.373156 |
| Candidate construction, call count | 423 | 1 |
| Candidate construction, total | 15,360.197 | 0.366 |
| FreeformRiften07 | 36.435211 | 0.002184 |
| PlayerVampireQuest | 37.134945 | 0.000732 |
| CreatureDialogueHorse | 36.283464 | 0.000962 |
| DialogueSolitude | 36.351603 | 0.006001 |

Phase spans inside `World::start_quest` identified candidate construction as
the repeated cost. The old path collected all winning ACHR and REFR records,
sorted and deduplicated them on every attempt, including quests without a
searched alias. The replacement skips construction for those quests and
derives one bounded index from `LoadOrder::winners`, whose identities are
already unique and sorted. It caches either the complete candidate set or
the fact that there are more than 4,096 candidates. A partial set is never
evaluated. Conditions, live eligibility and alias reuse still run at each
start. The cache adds no saved field.

All World hash fields matched before and after the change, both immediately
after startup and after `run_until_idle(64)`. This includes alias assignments,
stages, quest flags and queued events. The quest hash before script settling
was `12244036582383919460`, and after settling it was
`16996129545838252704`, equal in the two processes. Script settling took
280.275 ms before and 94.298 ms after. That phase was not changed, and these
single-process samples do not establish a script-execution speedup.

`prf_33_installed_startup_quests_finish_within_release_budget` loads scripts
and binds before timing. Its timed loop includes quest state construction
and the first candidate lookup. It excludes data opening, script loading,
binding and script settling. The 1.5 second release limit was fixed before
the first test run. With tracing off, the release case started the same 421
quests in 3.604097 ms, with a 1.383 microsecond median and a 301.119 microsecond
maximum. Debug builds check the startup result without enforcing a release
timing limit. These measurements cover this machine and installed cohort,
not frame time or complete quest execution.

The final release rerun passed all seven PRF-33 tests and measured 3.817829 ms
for the same 423 attempts and 421 starts. Existing quest regressions and 73
additional alias cases passed. `cargo fmt --all` and
`cargo clippy -p soe-game --all-targets -- -D warnings` exited zero.

The alias fields and startup-stage flags follow local
`TES5Edit/Core/wbDefinitionsTES5.pas` and `uesp-modfileformat/QUST.wikitext`.
`ck-wiki/Quest_Script.wikitext` supplies the quest state query contracts.
PRF-33 reuses the existing quest interpreter and load-order winner iterator
rather than adding another record or alias model.

```text
SKYRIM_DATA="<Data>" cargo run --release -p soe-game --example startup_timing -- --trace
SKYRIM_DATA="<Data>" cargo test --release -p soe-game --lib --test it prf_33_ -- --nocapture
```

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

## Detection neighborhoods, PRF-31

The threshold fixed before the run is 2 ms for the whole of
`PlayerNode::advance_detection`, including body synchronization, sight rays,
light lookup and `World::evaluate_detection`. The installed-data check uses
Godot 4.4.1 with the release extension and the five-master load order. The
comparison opens Bannered Mare and uses its unlocked door into WhiterunWorld,
then samples the same exterior update with 32 resident actors. The original
path produced 266 observations and took 3,203 microseconds. The original
process exited zero. Timing excludes startup and door resource replacement.

The implementation keeps light-reference identity in a derived World index,
bins current light influence for CPU queries, and builds observer/target pairs
only within the GMST detection range. Sound source neighborhoods remain
separate from actor neighborhoods. Light is shared per target in a frame,
actor senses and combat awareness are shared within an evaluation batch.
Existing awareness beyond range advances loss and recovery timers without
creating distant observer/target pairs. These indexes add no saved state or
reference scene nodes.

The first spatial-path windowed comparison retained 32 actors and produced
248 observations in 1,410 microseconds, compared with 266 observations in
3,203 microseconds before the change. A later windowed sample of the final
body-list reuse path measured 37,981 microseconds with the same actor and
observation counts. These single samples show substantial timing variation,
they do not establish the 2 ms acceptance budget.

The corrected headless Godot 4.4.1 exterior sample took 717 microseconds with
26 actors and 218 observations. It settles deferred unload and changes grid
focus before sampling, so it does not share the earlier 32-actor cohort.
Its walking check ran two 30-second wall-time phases, excluding gate resource
replacement. It completed 3,623 detection
updates and visited WhiterunWorld and Tamriel cells with resident NPCs. Maximum
horizontal displacement was 468.237 units inside Whiterun and 1,503.824 units
in Tamriel. Whiterun sampled 9 actors and 8 observations near the gate.
Tamriel sampled 446 actors and 530 nearby observations, with reported samples
of 3,591, 3,862 and 3,743 microseconds. The maximum was 6,657 microseconds and
1,813 updates exceeded 2,000 microseconds. The process exited one. PRF-31's
whole-update budget is not met. Sparse pair selection and light lookup are
implemented, but the complete update remains over budget in this exterior.

The walking harness holds NPC placements fixed and omits AI planning and VM
ticks. It steps the host capsule, updates shared player placement, streams
exterior residency and calls the whole gameplay detection updater. A zero-time
gameplay frame after each load-door transition settles its deferred unload
before walking. The earlier windowed attempt omitted that settlement and
rejected streaming deltas, so its walking samples are not detection timings.

Validation passed four new game unit tests, 23 existing game detection
integration tests, 13 host detection tests with installed data, including
indexed/full-scan light equality in Bannered Mare, Whiterun and Tamriel, and
four existing host adversarial tests. Formatting and both changed packages'
all-target Clippy checks passed. The workspace gate was left to the reviewer.
