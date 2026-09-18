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
| `data_source` | no | a real `Data` folder through `esm`, `bsa`, `nif` and `dds` to loaded cells and exterior blocks |
| `batch` | no | a cell's surfaces merged per material set, with the side table back to each reference |
| `godot_layer` | yes | the `CellStreamer` node, `RenderingServer` instances, `array_mesh_from`, `heightmap_image_from` |

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

`stream_cell_id` walks the cell index, which keeps a placed object whose winning
record carries the deleted flag out of the drawn list and files it under
`deleted_references` instead (`docs/esm.md`, "Dirty plugins"). A deleted
reference still resolves through the load order and the game starts it disabled,
so the host never draws one and never has to test for it.

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
and the BSLightingShaderProperty flags verbatim. `data_source` resolves those paths and
`godot_layer` maps the result onto `StandardMaterial3D` ("Materials" below). Validation fails
loudly because Godot's own response to bad arrays is a console line and a missing mesh.

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

`data_source::mesh_data_from` is the integration with the `nif` crate: it owns a copy of
`nif::Mesh`'s vertex data with the reference's placement baked in and adapts it into
`mesh::MeshData`, rather than `nif::Mesh` implementing `MeshSource` directly, since the axis
and unit conversion in `build_surface` still needs to run on top. The `esm` crate's LAND view
feeds `decode_vhgt`, not yet wired to `data_source`. The Jolt binding from bhk shapes will need
collision data that `MeshSource` does not carry and should be a second trait, not a widening
of this one.

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

## Real cell (2026-09-17)

`godot-host` now depends on `esm`, `bsa` and `nif` (`crates/godot-host/Cargo.toml`), closing
the gap the previous section called out. `src/data_source.rs` is the new module, engine-free
like the rest of the crate below the Godot layer. `DataSource::open` loads the five base
masters through a `LoadOrder` and builds a `bsa::Vfs` from their associated archives plus the
data folder as a loose overlay, matching the shipped engine's override order. `resolve_cell`
finds a cell's winning `CELL` record by editor id, preferring an interior cell when an
interior and an exterior one share a name, and `first_populated_interior_cell` picks any
interior cell with more than a given reference count when a named one is missing, so the
gated test still has real content to load on a different install. `stream_cell` walks the
winning `REFR` records placed in the cell, and for each one whose base resolves to a `STAT`
with a model, reads the NIF through the `Vfs`, parses it, and runs every shape through
`mesh::build_surface` with the reference's own placement transform composed with the shape's
transform inside the NIF. A reference whose mesh fails to read or parse is counted and
skipped rather than aborting the cell.

The reference's rotation floats become a matrix the same way `NiMatrix3::SetEulerAnglesXYZ`
does in CommonLibSSE, since that is what the engine itself uses to turn a REFR's `DATA`
rotation into world orientation. `Transform::then` composes the two transforms so the shape's
own NIF-internal transform applies first and the reference's placement applies second, which
is the order `mesh_data_from` needs before `build_surface` converts axes and units.

`CellStreamerNode` exposes this as two `#[func]` methods. `load_data(path)` opens a
`DataSource` over a `Data` folder and returns whether it opened, storing the source on the
node for `stream_cell` to use, logging the reason to Godot's error log on failure.
`stream_cell(editor_id)` resolves the cell, builds a `MeshInstance3D` with `array_mesh_from`
for every surface returned, adds each as a child of the node, and returns how many were built
(-1 if `load_data` was never called, 0 if the cell resolved but nothing built).

`crates/godot-host/tests/real_cell.rs` is the gated Rust test, run without a Godot binary:

```
$ SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-godot-host --release --test real_cell -- --nocapture
cell: 0:01605E
references: 690
statics with a model: 318
meshes parsed: 318
meshes failed: 0
triangles: 124924
wall time: 22.89s
test a_real_cell_streams_statics_from_the_installed_game ... ok
```

`0:01605E` is `WhiterunBanneredMare`, found directly, the fallback to
`first_populated_interior_cell` was not needed on this install. Every static with a model
parsed, none failed, and the cell produced 318 mesh loads for 124,924 triangles in 22.89
wall clock seconds including archive indexing and load order setup, release build, single
thread, development machine.

`godot/headless_test.gd` now branches on whether `SKYRIM_DATA` is set. Set, it calls
`load_data` and `stream_cell("WhiterunBanneredMare")` on `CellStreamerNode` and exits on that
result alone, no frame wait needed since the call is synchronous. Unset, it falls back to the
original synthetic run from the previous section unchanged. Run against the same install:

```
$ SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" godot --headless --path godot --script headless_test.gd
Initialize godot-rust (API v4.4.stable.official, runtime v4.4.1.stable.official, safeguards balanced)
SKYRIM_DATA=/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data
Godot version: 4.4.1-stable (official)
Godot headless load: cell = WhiterunBanneredMare, mesh nodes built = 763
RESULT: PASS
```

763 mesh nodes for 318 parsed statics is expected, since one NIF can carry more than one
shape and `stream_cell` adds one `MeshInstance3D` per surface, while the Rust test's
`meshes_parsed` counts references, not surfaces. Exit code 0. This closes the "Godot headless
load" done check from `ENGINE-PLAN.md`: one real cell, streamed from the installed game,
through the full `esm`, `bsa`, `nif`, `mesh`, `godot_layer` pipeline.

Not done in this round: winding and UV orientation were unverified against what the meshes
actually look like on screen, since the headless run has no display to check against, only
node and triangle counts, and material mapping onto `StandardMaterial3D` was still deferred so
`stream_cell`'s meshes carried no texture even though `MaterialDesc` was populated from the
NIF. Both are closed below. Per-cell triangle and byte budgets, and interior cell streaming by
proximity rather than by editor id, are still unstarted, `stream_cell` loads exactly the one
cell it is asked for.

## Materials (2026-09-17)

`soe-dds` is now a `godot-host` dependency. `data_source.rs` resolves each surface's
`MaterialDesc` into decoded textures. A NIF material's `diffuse` and `normal` fields already
carry the full `textures\...` path (checked against the `nif` crate's own fixtures), so no
prefix is added, only `soe_bsa`'s normal casing and separator handling, which `Vfs::read`
already does internally. The read goes through the same `Vfs` the meshes use, so a loose file
in the `Data` folder overrides an archive's copy of the same texture. `load_texture` reads the
bytes and decodes the top mip with `soe_dds::Dds::parse` and `.decode(0, 0)` to an RGBA8
`Image`. `MaterialCache`, built once per `stream_cell_id` call and keyed on the lowercased
path, decodes each distinct texture once per cell and hands out `Rc<Image>` clones to every
surface that names it, so a texture used by forty statics costs one decode. The cache also
counts distinct texture sets (by lowercased diffuse and normal path pair), textures decoded,
missing paths, and decode failures, folded into `CellStats`.

`godot_layer.rs` turns a decoded `Image` into a Godot `ImageTexture` with
`Image::create_from_data` (`FORMAT_RGBA8`) and `ImageTexture::create_from_image`.
`CellStreamerNode::stream_cell` keys a `StandardMaterial3D` cache on the surface's diffuse and
normal path pair, so surfaces sharing a texture set share one material instead of building a
duplicate per mesh. The diffuse image becomes `TextureParam::ALBEDO`, and when a normal map is
present it becomes `TextureParam::NORMAL` with `Feature::NORMAL_MAPPING` turned on. Skyrim's
BC7 or BC5 normal maps carry a specular mask in the alpha channel, not opacity, and Godot's
`normal_texture` only ever reads RGB for the surface normal, so that alpha channel is inert
here rather than fed into anything, until a specular term is added. Alpha mode comes from
`MaterialDesc::alpha`: `Opaque` leaves `Transparency::DISABLED`, `Cutout(threshold)` sets
`Transparency::ALPHA_SCISSOR` with that threshold, `Blend` sets `Transparency::ALPHA`.
`double_sided` sets `CullMode::DISABLED`, otherwise Godot's default back-face cull applies.

`crates/godot-host/tests/real_cell.rs` reports the new stats alongside the existing ones:

```
$ SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-godot-host --release --test real_cell -- --nocapture
cell: 0:01605E
references: 690
statics with a model: 318
meshes parsed: 318
meshes failed: 0
triangles: 124924
wall time: 9.02s
distinct texture sets: 46
textures decoded: 88
textures missing: 0
texture decode failures: 0
texture decode wall time: 0.56s
test a_real_cell_streams_statics_from_the_installed_game ... ok
```

46 distinct diffuse and normal path pairs across the cell's 318 statics decoded to 88
textures (most sets carry both a diffuse and a normal map), none missing from the `Vfs`, none
failed to decode, 0.56 of the 9.02 second wall time. The wall time itself dropped from the
22.89 seconds the previous section recorded to 9.02, both release builds on the same machine,
which is archive indexing and load order setup noise between runs rather than anything this
change accounts for.

`godot/capture.gd` (windowed, `--resolution 1600x900`, `SKYRIM_DATA` set) overwrote
`docs/images/bannered-mare-1.png` through `-3.png`. The three views show wood grain on beams,
floor and furniture, plaster and stone wall textures, a woven basket motif, a banner-carved
pillar, and a hanging feather fetish rendering with alpha-scissor cutout, gaps between the
feathers rather than an opaque quad. This confirms winding and UV orientation are correct: a
wrong winding would have culled these surfaces from the camera's side instead of showing their
textures. A few surfaces still render flat white, references whose base form or model this
round's static-only path does not cover, out of scope here.

## Batching, RenderingServer instances, compressed textures, exteriors (2026-09-18)

Branch `batching`. Four changes to the node-building side, one to the material path, and the
exterior benchmark that `../research/host-engine-options.md` set as the gate for staying on
Godot. Every number below is from `godot/bench.gd` on the machine in `docs/performance.md`,
windowed, X11 display driver, vsync off, 1600 by 900, with the median and percentiles over at
least five seconds and 600 frames.

### What the 56 ms frame was

`docs/performance.md` (2026-09-17) reported the Bannered Mare at 56 ms a frame and named the
763 unbatched mesh nodes as the cause. That number came from `Performance.TIME_PROCESS`, which
this run showed reading a constant 80 to 113 ms across every frame of a scene the wall clock
puts at half a millisecond. `bench.gd` now times the wall clock between two `_process` calls
and reads the viewport's own measured GPU and CPU render time, and the unbatched Bannered Mare
is 0.48 ms a frame at about 2,000 frames a second. The nodes were never the frame's cost. They
were still worth removing, since the batched scene halves the frame again, and the render CPU
row shows what each change buys.

### Batching: one mesh per material set

`src/batch.rs` merges a cell's surfaces by `BatchKey`: lowercased diffuse and normal path,
alpha mode, cull mode, and which vertex attributes are present, since a merged vertex buffer
has one layout. Surfaces that agree are appended into one `SurfaceArrays` with their indices
offset, bounds merged, first-seen order kept. Skinned surfaces never merge. Every source
surface keeps its place as a `BatchMember` (first vertex and count, first triangle and count,
source index), so `Batch::member_of_triangle` maps a triangle index back to the surface, and
`LoadedSurface` now carries the `GlobalFormId` of the reference that placed it (or of the
`LAND` record for terrain), which is what a raycast hit will resolve to a form through
`CellStreamerNode::reference_at`. Batches are per cell, not per block, so Godot still culls per
cell in an exterior.

Merging into one `ArrayMesh` was chosen over a `MultiMesh` per repeated NIF. `mesh_data_from`
has already baked each reference's placement into the vertices, so a `MultiMesh` would need
that undone and re-applied per instance, and two placements of one NIF share its material and
so land in the same batch anyway. A `MultiMesh` pays off where one heavy NIF is placed hundreds
of times, the exterior rock and tree case, and is the next step once a trace shows it.

The Bannered Mare goes from 763 surfaces to 48 batches (46 texture sets, the rest split by
alpha mode), the largest batch holding 171 surfaces. The 3 by 3 Whiterun block goes from 802
surfaces to 195 batches.

### RenderingServer instances instead of nodes

The review that came in with this branch asked for no node per reference at all. Streamed
geometry now bypasses the scene tree: `add_instance` builds the `ArrayMesh` resource, calls
`RenderingServer::instance_create2(mesh, scenario)` in the node's viewport scenario (the main
loop's root viewport when the node is not yet inside the tree, which is where a `SceneTree`
script's `_initialize` runs), sets the transform to identity and the material override, and
keeps the RIDs and the mesh handle on the node. `exit_tree` and `clear` free them. There is no
`Node3D` per cell, per reference or per batch, no per-node process or transform notification,
and `scene_bounds` reports the merged bounds so a camera can frame the scene without walking
children. `bench.gd` and `capture.gd` use it. The unbatched mode (one instance per surface)
stays as the benchmark's baseline, on the `batch_surfaces` export.

The `StandardMaterial3D` cache moved from a per-call local to the node, keyed by texture set,
so a texture uploaded for one cell serves every later cell that names it. `last_stats`
reports the materials a call created, not the cache size.

### Compressed textures as they are

The 2026-09-18 trace on `master` had `host.material` at 97 percent of a cell's stream time:
every texture decoded from BC1, BC3, BC5 or BC7 to RGBA8 on the calling thread after the
workers joined, once per `stream_cell_id` call. Three changes:

- `TextureCache` is owned by `DataSource` and shared by every stream call and every worker
  thread (a mutex around the map, atomics for the counters), so a texture is read once for the
  source's lifetime and a second stream of any cell pays nothing for it.
- The read runs on the worker threads, inside `load_static_surfaces`, right after the NIF's
  surfaces are built.
- A block-compressed DDS is not decoded at all. `load_texture` copies layer 0's mip chain out
  of the file as `TextureData::Compressed` and `godot_layer::image_texture_from` hands it to
  `Image::create_from_data` with `mipmaps = true` as `FORMAT_DXT1`, `DXT3`, `DXT5`, `RGTC_R`,
  `RGTC_RG` or `BPTC_RGBA`. `godot/texture_check.gd` proves headless that 4 by 4 and 8 by 8
  DXT1, DXT5, RGTC_RG and BPTC_RGBA images with and without chains round-trip through
  `create_from_data` and `ImageTexture::create_from_image` with the format unchanged. The
  vanilla files stop their chains at 2 by 2 or 4 by 4 and Godot refuses a chain that does not
  reach 1 by 1 (`Expected Image data size ... got 16 bytes less`), so `load_texture` pads the
  tail with copies of the last stored block. Uncompressed DDS, signed BC4 and BC5 and volumes
  still decode their top mip to RGBA8 and get a generated mip chain, since without mips every
  minified sample of a 2048 square texture misses the texture cache.

Texture memory for the Bannered Mare: 592 MB as decoded RGBA8 without mips, 783 MB with
generated mips, 175 MB as the files' own BC blocks with their chains. The Godot-side
`stream_cell` (data path plus upload) went from 1.11 to 1.19 s with the decode to 0.50 to
0.52 s without it. The Rust-side stream numbers are in `docs/performance.md`.

### Exteriors: block, terrain, parent land

`DataSource::resolve_exterior_block(editor_id, radius)` walks the winning `CELL` records
once, finds the named cell's grid position and worldspace, and returns every cell of that
worldspace within `radius` grid steps, sorted south to north then west to east (radius 2 is
the game's `uGridsToLoad=5`). `stream_exterior` streams each cell through `stream_cell_id` and
appends the cell's terrain as one more `LoadedSurface`: `land_for_cells` walks the `LAND`
winners once for the whole block, and `terrain_surface` turns the 33 by 33 `VHGT` heights into
32 by 32 quads in Skyrim units, `VNML` normals when present, wound counter-clockwise from above
so `build_surface`'s flip lands them like a NIF's faces. `CellStreamerNode::stream_exterior`
is the GDScript entry point and `last_stats` carries the block's totals, the worldspace's
editor id and the cell list.

`WhiterunOrigin` is grid (4, -2) of `WhiterunWorld`, whose `WNAM` parent is Tamriel with the
`PNAM` "use land data" bit set (CommonLib's `kUseLandData`). Only three of the nine cells
around the origin carry their own `LAND`, the city draws Tamriel's under the other six, so
`resolve_exterior_block` also collects the parent's cells at the block's grid positions and
`stream_exterior` takes the parent's `LAND` where the child has none. Nine of nine cells then
have terrain. Whether the shipped engine draws the parent's land under a child cell that has
its own is not settled here, the override wins.

What the exterior still lacks, none of it needed for the block benchmark: the `LAND` quadrant
layers (`LTEX` base plus alpha layers) need a splat material, so terrain draws in the
untextured white material. Distant geometry is Bethesda's precomputed LOD (the `BTO` and `BTR`
files under `meshes/terrain`, to be consumed, never generated), not loaded, so nothing beyond
the requested ring exists. No water plane. `WhiterunWorld` lists grid (0, 0) twice among its
winners, both streamed. Interior streaming by proximity is still unstarted.

### Numbers

Frame time is the wall clock between two `_process` calls, presentation included, at 1600 by
900. GPU and CPU are `RenderingServer.viewport_get_measured_render_time_gpu` and `_cpu` for the
root viewport. Static count means references whose base is a `STAT` with a model.

| Scene | Cells | Refs | Triangles | Instances | Frame p50 | p99 | p99.9 | Worst | GPU p50 | CPU p50 | Texture MB |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Bannered Mare, unbatched | 1 | 690 | 124,924 | 763 | 0.480 ms | 0.950 | 1.492 | 5.651 | 0.081 ms | 0.177 ms | 175 |
| Bannered Mare, batched | 1 | 690 | 124,924 | 48 | 0.235 ms | 0.397 | 0.903 | 2.282 | 0.062 ms | 0.039 ms | 175 |
| Whiterun 3 by 3, unbatched | 9 | 957 | 303,619 | 802 | 0.758 ms | 1.934 | 2.749 | 3.747 | 0.093 ms | 0.280 ms | 313 |
| Whiterun 3 by 3, batched | 9 | 957 | 303,619 | 195 | 0.288 ms | 0.519 | 1.045 | 1.698 | 0.087 ms | 0.066 ms | 313 |
| Whiterun 5 by 5, unbatched | 25 | 1,767 | 624,408 | 1,499 | 0.889 ms | 1.782 | 3.013 | 7.567 | 0.156 ms | 0.454 ms | 347 |
| Whiterun 5 by 5, batched | 25 | 1,767 | 624,408 | 390 | 0.359 ms | 0.686 | 1.319 | 3.191 | 0.142 ms | 0.105 ms | 347 |
| WhiterunWorld 9 by 9, batched | 59 | 2,465 | 825,587 | 563 | 0.440 ms | 0.935 | 1.576 | 2.246 | 0.171 ms | 0.147 ms | 409 |

Draw calls equal instances in every row. The 3 by 3 block is cells (3,-3) to (5,-1), the 5 by
5 block (2,-4) to (6,0), the 9 by 9 radius covers every cell `WhiterunWorld` has (59 of a
possible 81, 57 with terrain). The camera sits above and behind the scene's bounds looking at
its centre, so every instance is in the frustum and the GPU row is the whole scene. Streaming
into Godot took 3.7 s for the 3 by 3 block, 9.5 s for the 5 by 5 and 30 s for the 59 cells,
about 0.4 s a cell, almost all of it the reference scan that the `cell-index` branch on
`master` has since replaced with a lookup, which this branch does not carry.

Captures: `docs/images/bannered-mare-batched.png` (the interior from above, batched, textures
intact), `docs/images/whiterun-exterior-3x3.png`, `docs/images/whiterun-exterior-5x5.png` (the
city on its rock, Dragonsreach at the top, white terrain around it), and
`docs/images/whiterun-world-9x9.png`.

### Godot 4.4.1 against 4.7.2

A Godot 4.7.2 binary sits next to the 4.4.1 one under `../tools/godot/`. The extension,
built against gdext's `api-4-4`, loads on it unchanged (`Initialize godot-rust (API
v4.4.stable.official, runtime v4.7.2.stable.official)`), and the 3 by 3 block was run on both,
plus 4.7.2 on both rendering thread models (`--render-thread safe` and `separate`).

| Binary, thread model | Batched | Instances | Frame p50 | p99 | p99.9 | Worst | GPU p50 | CPU p50 |
|---|---|---|---|---|---|---|---|---|
| 4.4.1, single-safe | no | 802 | 0.758 ms | 1.934 | 2.749 | 3.747 | 0.093 ms | 0.280 ms |
| 4.7.2, single-safe | no | 802 | 0.517 ms | 1.193 | 1.675 | 22.399 | 0.091 ms | 0.196 ms |
| 4.4.1, single-safe | yes | 195 | 0.288 ms | 0.519 | 1.045 | 1.698 | 0.087 ms | 0.066 ms |
| 4.7.2, single-safe | yes | 195 | 0.276 ms | 0.498 | 1.071 | 1.677 | 0.084 ms | 0.063 ms |
| 4.7.2, separate render thread | yes | 195 | 0.281 ms | 0.492 | 1.110 | 1.849 | 0.084 ms | 0.064 ms |

Batched, the two binaries are within run-to-run noise of each other, and the separate render
thread changes nothing at a third of a millisecond a frame, where there is no render work to
overlap. Unbatched, 4.7.2 has the lower median (0.517 against 0.758 ms with 802 draw calls)
and the one real hitch of the whole session, a 22.4 ms frame among 8,562, still under the
33 ms bar. One run each, so the median gap is a lead, not a finding. 4.7.2 prints one `This function (finalize) can only be called from the
render thread` error at exit in the separate model, after the last frame, which does not touch
the numbers. Nothing here argues for or against either binary. The differences the rendering
brief lists (occlusion in 4.5, MultiMesh visibility-range fade and the command queue lock in
4.6, the environment buffer in 4.7) need a scene with occluders, a MultiMesh path and a moving
camera to show, none of which this benchmark has yet.

### Against the pass bar

`ENGINE-PLAN.md` on `master` fixed the bar before these numbers existed: for the 3 by 3 block
with every static drawn, p99 at or under 16.7 ms and the worst hitch under 33 ms, and a cell
streamed in under 250 ms on the workers.

| Criterion | Bar | Measured (4.4.1, batched, 3 by 3, 9 cells, 957 references, 303,619 triangles) | |
|---|---|---|---|
| Frame p99 | at or under 16.7 ms | 0.519 ms | pass |
| Worst hitch | under 33 ms | 1.698 ms over 16,444 frames in 5 s | pass |
| Cell stream on workers | under 250 ms | 3,655 ms for 9 cells, 406 ms per cell, of which 356 ms is the reference scan this branch still carries | fail on this branch |

The unbatched 3 by 3 (802 instances) also passes the two frame criteria at 1.934 ms and
3.747 ms. The stream criterion fails here because of the linear reference scan, which the
`cell-index` branch on `master` replaced with a 0.5 ms lookup after this worktree branched.
With that index the per-cell cost on this branch is the NIF parse, mesh build and texture copy
on the workers (`host.build_all`, 17 ms mean for the Bannered Mare's 318 statics, 67 ms worst)
plus the Godot upload, so the merged tree should pass it. That is a claim to measure on
`master` after the merge, not a result.

On the host question: the whole city worldspace, 825k triangles, draws in 0.17 ms of GPU time
and 0.15 ms of render CPU at 1600 by 900 on this machine, with the frame wall clock at 0.44 ms
and the worst hitch over five seconds at 2.2 ms. Godot's renderer is not the limit at this
scale. What the numbers do not cover: the Tamriel plains around the city with their LOD,
grass, trees and water, and a moving focus with cells loading and unloading under it. Those
are the next benchmark, on the cell index and the BTO and BTR readers.

## Skinned characters (2026-09-18, branch `skinning`)

The animation stack was finished and invisible: `crates/havok` reads skeletons,
clips and behavior graphs, and since `behavior-2` it walks the vanilla
`mt_behavior.hkx` and plays the standing idle at full weight. Nothing reached a
screen. This connects the two.

Three pieces, none of which touch Godot except the last:

- `soe_havok::pose::to_model_space_into` composes the interpreter's local pose
  with its parent chain into a caller's buffer, so a frame allocates nothing.
  The allocating walk it mirrors, `Skeleton::to_model_space`, already existed
  and is not duplicated.
- `crate::skinned` matches a shape's own bone list onto the skeleton by name,
  combines the model pose with the shape's inverse bind, and converts the
  result into Godot's axes and metres. The conversion is conjugation by the
  axis map, which is a homomorphism, so transforms compose first and convert
  once per bone.
- `crate::godot_actor::SkinnedActorNode` uploads that. One `RenderingServer`
  skeleton **per shape**, not per character: a shape's vertex bone indices
  address its own bone list, so two shapes naming different bones would read
  each other's transforms.

### Measured on the installed game

`malebody_1.nif` with the vanilla human skeleton and `mt_behavior.hkx`:

| | |
|---|---|
| Skinned shapes, vertices, triangles | 2, 1,802, 2,948 |
| Skeleton bones | 99 |
| Bones the body names, matched | 29, all 29, none by case |
| Graph nodes, clips loaded, missing | 2,223, 787, 1 |
| Pose plus skin, per frame | 8.2 us |
| Walk frames where the skin moved | 180 of 180 |

Headless, the graph starts in `MT_Idle.HKX` and the walk event moves it to
`MT_WalkForwardSlow`, so the interpreter, the clips and the upload all run.
`godot/skinned_test.gd` asserts that transition, `godot/skinned_capture.gd`
saves `docs/images/skinned-{idle,walk}.png`.

### Two defects found, both closed

**Bone indices.** A `NiSkinPartition` splits a shape into body parts, and a
packed vertex's bone indices address the **partition's** short bone list rather
than the skin instance's. The `nif` crate's legacy `NiTriShape` path maps them
through `Partition::bones`, the `BSTriShape` path Special Edition uses copied
them raw. `actor::partition_bone_indices` corrects it here by reading the
partition blocks directly. The reader itself was fixed on master afterwards,
measured over 400 actor meshes where 564 of 846 partitions carry a reordering
bone list, so the two now agree and this copy could be dropped in favour of
`mesh.bone_indices`.

### The bind pose, and why it hid at the pelvis

The character rendered with a correct torso, hips and legs and with the arms
flattened into blades. The error was zero at the pelvis and grew through every
bone below it, which was read at the time as two bind poses disagreeing.

It was one transposed rotation. `nif.xml` declares `Matrix33` as nine floats in
OpenGL column major order, `m11 m21 m31 m12 ...`, and `crates/nif` reads it
that way, so `Mat33::rows` holds the matrix as written and `Affine::from_nif`
applies it as `M * v`. That is right for a scene graph node and the streamed
cells prove it every time they draw. It is wrong for a bind transform, and
`Affine::from_nif_skin` transposes those.

`nifly`, the C++ library under PyNifly, which imports and exports Skyrim
characters with working skinning, does the same thing by a different route: it
reads the nine floats straight into memory, so its `Matrix3` is the transpose
of ours, and its `ApplyTransform` computes what we would write as
`transpose(M) * v`. Transposing here reproduces `nifly` rather than departing
from it. Its `Skin.hpp` also names the two fields plainly, `boneTransform` is
`transformSkinToBone` and `skinTransform` is `transformGlobalToSkin`, which
settles the composition: bone world times skin to bone carries a vertex from
skin space to model space, and the global offset is not part of it. GPL-3.0,
read and credited, and nothing derived from it may reach the MIT shim crates.

Why it hid: the pelvis bind rotation is the identity, and an identity matrix is
its own transpose, so that one bone cancelled correctly while every descendant
inherited the error. That also explains the earlier attempt that composed
`Skin::transform` without the transpose. It made the pelvis exact and the
number smaller while the capture got worse, because the pelvis was the only
bone the missing transpose had ever spared.

The composition is now `pose * transpose(inverseBind)`, with the shape offset
left out. Three facts pin it in `tests/bind_pose.rs`, gated on the installed
game:

| Check | Result |
|---|---|
| Bones agreeing on one transform at the reference pose | within 0.000002 |
| What they agree on, against the shape offset inverted | matches |
| Skinned height range, a body with no head, hands or feet | 0.162 m to 1.645 m |
| An upper arm's stored bind against the pose rotation | equal to 0.000001, against its transpose 0.82 |

The last row is the cause stated as a measurement: the rotation in the bind
slot is the bone's own pose rotation, so reading it straight applies the
rotation twice instead of cancelling it.

`tests/real_skinning.rs` asserts the first of those on every frame path, and
the captures in `docs/images/skinned-{idle,walk}.png` are a body standing and
walking. Materials are still unwired, so they are unlit grey.
