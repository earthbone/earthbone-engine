# godot-host: streaming, mesh and terrain for the Godot 4 host

Crate `soe-godot-host` (`crates/godot-host`). A GDExtension in Rust on godot-rust gdext
0.5.5 (crates.io 2026-08-09, the same version as `engine-refs/gdext`), API floor Godot 4.4
because Jolt became the built-in physics there. The crate builds and tests with cargo alone.
With `--no-default-features` the `godot` dependency is gone and what remains is the logic.

## Layout

| Module | Depends on Godot | Job |
|---|---|---|
| `units` | no | Skyrim units and axes to Godot metres and Y up |
| `streaming` | no | which cells to load and unload around a focus, with budgets and hysteresis |
| `mesh` | no | a decoded NIF shape to `ArrayMesh` surface arrays, with validation |
| `terrain` | no | LAND heights to Terrain3D regions |
| `godot_layer` | yes | the `CellStreamer` node, `array_mesh_from`, `heightmap_image_from` |

The Godot layer decides nothing. That is what keeps the tests engine-free, and it is the
seam a VulkanSceneGraph or other host would reuse: everything above the layer is plain data.

`godot/` holds a project stub and `soe.gdextension` pointing at `target/`, so `cargo build`
plus opening `godot/` in a 4.4 or later editor loads the extension. No Godot binary is on the
development machine, so that step has not been run.

## Units and axes

One Skyrim unit is 1.428875 cm, a cell is 4096 units (58.5 m), LAND heights are in eighths
of a unit. Skyrim is Z up with Y north, Godot is Y up with north at -Z. The map
`(x, y, z) -> (x, z, -y)` has determinant +1, so handedness and winding survive it. Godot
draws clockwise faces as front faces. Whether Skyrim's shapes come out counter-clockwise
after the map is not settled until a real NIF renders, so `MeshOptions::flip_winding` is a
switch, default on, and the generated normals follow it.

## Streaming

`CellStreamer::update(focus)` returns a `StreamPlan`: cells to load, nearest first, and cells
to unload, farthest first. The load square is `load_radius` cells each side of the focus cell
(2 gives the game's `uGridsToLoad=5`). Cells are released only beyond `unload_radius`, which
must be at least the load radius. The band between the two is hysteresis: a player pacing on
a cell boundary loads the new column once and never unloads the old one. Per-tick caps on
loads and unloads spread a fast traversal over frames. `settle` drains the plan for loading
screens.

The streamer assumes a requested load lands. A load that fails has to be reported back with
`mark_unloaded`, or the cell is never asked for again. That is a choice: the alternative,
the streamer tracking load state itself, puts engine state into the decision logic and was
what made every mod manager VFS in the notes hard to test.

Budget design for the Whiterun gate: the `StreamPlan` is the unit the host schedules. The
intended loop is one `update` per frame, loads handed to a worker pool that decodes NIF and
builds `SurfaceArrays` off the main thread, and only `array_mesh_from` plus scene insertion
on the main thread, capped by `max_loads_per_tick`. Triangle and byte budgets per tick are
not in the streamer yet because nothing measures the real per-cell cost until the `nif` and
`esm` crates land. `SurfaceArrays` carries bounds and a triangle count so that budget can be
added without changing the plan shape.

## Mesh

`mesh::MeshSource` is a trait shaped like the `nif` crate's neutral output: positions,
optional normals, tangents with a handedness sign, UVs, a triangle index list, and optional
four-bone skinning. `build_surface` validates (empty, non-triangle, out-of-range index,
attribute length, half-skinned), converts axes and units, optionally flips winding, generates
area-weighted normals when the source has none, flattens tangents to four floats and bones to
four per vertex, and computes bounds in metres. The material descriptor carries texture paths
and the BSLightingShaderProperty flags verbatim, and the mapping onto `StandardMaterial3D`
is deferred to where the feature set is known. Validation fails loudly because Godot's own
response to bad arrays is a console line and a missing mesh.

## Terrain

`terrain::decode_vhgt` turns a LAND VHGT block into 33 by 33 absolute heights in units, rows
from the south-west corner. `RegionBuilder` stitches cells into Terrain3D regions of
`cells_per_region` cells a side, one vertex per LAND sample, so vertex spacing is 128 units
(1.83 m) and 8 cells make Terrain3D's default 256-vertex region. Missing cells are filled at
a configurable height under sea level, never an error, since Tamriel's border cells have no
LAND. `RegionHeightmap::rf_bytes` is the `FORMAT_RF` payload and `godot_location` the
Terrain3D region location.

The one subtle point is the flip. Turning Skyrim's half-open cell interval `[a, b)` into
Godot's `(-b, -a]` lands one sample off the region grid, so a Godot region row `j` reads
LAND row `size - j` counted from the region's southern edge, which means rows 1 to 32 of each
cell with row 32 the shared northern seam. The seam test builds cells from one continuous
function and checks every vertex against it.

## Benchmark

`tests/whiterun_bench.rs` generates a 9 by 9 cell grid with forty 2,000-triangle statics and
a LAND grid per cell, walks the focus across it one cell per tick, and runs the full Rust
pipeline per loaded cell. On the development machine, debug build, single thread:

| | |
|---|---|
| Cells loaded | 120 |
| Triangles converted | 9.6 million |
| Cells per second | 45 |
| Triangles per second | 3.6 million |

That is the floor for the Rust side. Godot's upload and draw cost, and NIF decoding, sit on
top and need a Godot binary and the `nif` crate to measure. The test asserts only that the
rate stays above one cell per second, so it flags a regression without failing on a slow
machine.

## Untested

- Loading the extension in Godot, the `CellStreamer` node's signals, and both conversion
  functions at runtime. They compile against gdext and follow its documented API, nothing more.
- Winding and UV orientation on a real NIF.
- Terrain3D's acceptance of a 256 by 256 `FORMAT_RF` image at the computed location.
- Real per-cell cost. The benchmark's statics are synthetic.

## Round 2 hooks

`impl MeshSource for soe_nif::Shape` is the integration with the `nif` crate. The `esm`
crate's LAND view feeds `decode_vhgt`. The Jolt binding from bhk shapes will need collision
data that `MeshSource` does not carry and should be a second trait, not a widening of this one.

## Headless run (2026-09-14)

A Godot 4.4.1 Linux editor binary now lives at `../tools/godot/Godot_v4.4.1-stable_linux.x86_64`
(round 3, `godot-headless`). The crate builds as a cdylib with `cargo build -p soe-godot-host
--release`, no source changes were needed.

Loading the extension outside the editor took two fixes, both now committed:

- `godot/soe.gdextension`'s `linux.debug.x86_64` key pointed at
  `res://../target/debug/libsoe_godot_host.so`, which is never built here. The stock editor
  binary reports the `debug` feature tag even in headless script mode, so the loader picked
  that key and failed with `GDExtension dynamic library not found: 'res://soe.gdextension'`
  rather than falling back to the release key. Both `linux.debug.x86_64` and
  `linux.release.x86_64` now point at the same `res://../target/release/libsoe_godot_host.so`,
  since only the release build exists.
- Godot only reads a project's `.gdextension` files through a cache it writes on first import
  (`godot/.godot/extension_list.cfg`), which a bare `--script` run never generates. A project
  never opened before needs one editor pass first:
  `godot --headless --path godot --import`. That pass now prints
  `Initialize godot-rust (API v4.4.stable.official, runtime v4.4.1.stable.official, safeguards
  balanced)` with no GDExtension errors, confirming the load.

`godot/headless_test.gd` is a `SceneTree` script, run with
`godot --headless --path godot --script headless_test.gd`. What it actually exercises: this
crate has no `esm`, `bsa` or `nif` dependency yet (see "Round 2 hooks" above), so there is no
way for the extension to resolve a named cell such as `WhiterunBanneredMare` or read anything
under `SKYRIM_DATA`, and `array_mesh_from` is a plain Rust function, not a `#[func]` GDScript
can call. The script instead drives the one thing that is wired up, `CellStreamerNode`, with a
synthetic focus placed mid exterior cell `(0, 0)`, `load_radius = 1`, builds one placeholder
`MeshInstance3D` per `cell_load_requested` signal, and exits 0 once at least one mesh node
exists, 1 otherwise. Real content loading through `SKYRIM_DATA` and named interior cells is the
retry item once `godot-host` gains the `esm`, `bsa` and `nif` dependencies (round 4 territory,
not scoped to this branch).

Run and result (`SKYRIM_DATA` set to the vanilla install, 2026-09-14):

```
$ godot --headless --path godot --script headless_test.gd
Initialize godot-rust (API v4.4.stable.official, runtime v4.4.1.stable.official, safeguards balanced)
SKYRIM_DATA=/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data
Godot version: 4.4.1-stable (official)
Godot headless load: cells streamed = 9, mesh nodes built = 9
RESULT: PASS
```

Exit code 0. Nine cells is the full 3 by 3 ring `load_radius = 1` asks for around the focus
cell, so every cell the streamer wanted got a placeholder mesh. One benign warning appears on
the first frame, `ERROR: Condition "!is_inside_tree()" is true. Returning: Transform3D()` from
`get_global_transform`, because `CellStreamerNode::process` reads the focus node's global
position before the scene's first `_ready` pass finishes placing it in the tree. It does not
change the result (the focus position is fixed and correct from the second frame on) and needs
no fix for this smoke test, but a real game-side user of `CellStreamerNode` should add the
focus node to the tree before the streamer node, or gate the first `process` tick on
`is_inside_tree()`.
