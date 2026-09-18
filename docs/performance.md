# Performance

The harness is `crates/bench` (binary `soe-bench`) plus `crates/trace`
(scoped spans) and `godot/bench.gd` (frame time). Every number below comes
from one machine and one commit, so a later run on the same machine can be
diffed line by line. Vanilla numbers are a separate problem, see the end.

## Machine

- AMD Ryzen 9 7900X, 12 cores, 24 threads, up to 5.7 GHz
- 61 GB RAM, the game data on the page cache after the first run
- NVIDIA GeForce RTX 5070 Ti, Vulkan 1.4, Godot 4.4.1 Forward+
- Fedora Atomic, release build, the installed Skyrim SE under Steam

## How to run

```text
SKYRIM_DATA="<Data>" cargo run -p soe-bench --release -- [--runs N] [--out DIR] <subcommand>
```

Subcommands: `load-order`, `archives`, `cell <editor id>`, `exterior <editor
id>`, `scripts`, `navmesh`, `save`, `overhead <editor id>`, `trace <subcommand
...>` (writes `soe-bench-trace-<name>.json` for Perfetto or `chrome://tracing`
and a `.txt` summary next to it), `all`. Each measurement prints as
`name value unit`, timings as the median and the minimum of `--runs`
(default 3). The Godot frame benchmark:

```text
SKYRIM_DATA="<Data>" godot --headless --path godot --script bench.gd
SKYRIM_DATA="<Data>" godot --path godot --script bench.gd
```

The first run of a fresh checkout needs `godot --headless --path godot --import`
once, see `docs/godot-host.md`.

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
