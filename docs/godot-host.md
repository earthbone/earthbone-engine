# godot-host: streaming, mesh and terrain for the Godot 4 host

## Live detection observations, CMB-32

PlayerNode samples each ordered pair of resident, living actors in the same
cell after the player's pose reaches Game. Their FormIDs come from
`World::contact_actors`. A Godot physics ray from eye to eye reports the first
body, including an intervening actor or static obstruction. The query uses
the existing RID-backed actor bodies and allocates no reference nodes.

`detection::light_level` reads the winning CELL/XCLL ambient RGB, LGTM
inheritance, and enabled authored or runtime placed LIGH/DATA radius, color,
negative and off-by-default flags with FNAM fade. It samples current reference
placement and enable state, including `PlaceAtMe` references, and bounds the
resulting record-derived light estimate to 0 to 150. The host sends that
value, sight, motion and queued detection sounds to
`World::evaluate_detection` each step.
A sound event uses its source position and owning actor FormID. Footsteps emit
a detection sound after successful playback. World keeps the stealth pool and
observer state across cell resource replacement.

The estimate does not yet select a live WTHR from climate or apply WTHR
ambient, sunlight and image-space sunlight scale. The current sound path
covers `CreateDetectionEvent` and played player footsteps. The focused tests
exercise record light, occlusion inputs, cross-cell identity and sound source
ownership. The `SKYRIM_DATA` case reads a populated vanilla interior. No
interactive Godot sight run or frame-cost measurement is recorded here.

## Cell ambience, AUD-09

The player owns one non-positional ambience stream through a Godot
`AudioStreamPlayer`. On a cell change it stops the prior stream and starts the
first eligible single-track path returned by `soe_game::ambience`, which reads
MUST conditions and palette leaves. This uses Godot's audio server path
documented by local `godot-docs/classes/class_audioserver.rst`. Track looping,
crossfades, multiple regional sound descriptors and MUSC priority arbitration
are not implemented.

### Crosshair and held references, HST-18

`PlayerNode` raycasts from the eye along the camera look direction on each
advance and stores the hit reference in the shared game world. The Game
Papyrus globals `GetCurrentCrosshairRef` and `GetPlayerGrabbedRef` return those
current host values. Holding the mapped Activate input converts a hit
reference's collision body to kinematic mode and moves it with the look ray.
Releasing Activate returns its body to rigid mode. The grabbed reference is
cleared when the cell is replaced. These values are transient and are not
saved.

The grab path currently uses references with static collision bodies loaded
for the active cell. Movable-reference body creation and synchronizing their
physics pose into the shared world remain HST-12 work.

### Character race and sex selection, UI-07

`PlayerNode::show_race_menu` opens the native Control menu for race and sex
selection. `Game.ShowRaceMenu` queues a transient host request that the player
consumes on its next gameplay advance. The list comes from winning, non-deleted
`RACE` records whose `DATA` playable flag is set, ordered by resolved FormID.
Confirmation stores the chosen race and sex through the shared game's player
actor-base state. Actor values initialize from the confirmed race when the
player actor is first built, and later confirmations leave initialized values
intact. The actor-base race and sex already belong to SOEW saves.

The race flag and head-part field definitions were read in local
`engine-refs/TES5Edit/Core/wbDefinitionsTES5.pas`, with the prose layouts in
`engine-refs/uesp-modfileformat/RACE.wikitext` and
`engine-refs/uesp-modfileformat/HDPT.wikitext`. The CK
`engine-refs/ck-wiki/Game_Script.wikitext` describes `ShowRaceMenu` as the
race and sex change screen. The current menu also supports the appearance
editing described for UI-14 below.

### Player head parts and morph values, UI-14

The race menu lists winning, live HDPT records marked playable and compatible
with the selected sex. Each entry uses its PNAM part type, and selection
updates the menu's appearance preview summary. Face, Nose and Jaw morph
controls adjust normalized values from -1 to 1 in 0.1 steps. Confirm writes
the draft to the player's shared actor state, which SOEW stores in the UI-14
actor section. Closing the menu discards the draft and leaves the saved actor
appearance unchanged. The preview is a menu summary of selected records and
morph values.

### Race menu asset cache, UI-17

`Game.PrecacheCharGen` asks the host to read model files for playable race
head-part choices and the TRI files tagged as chargen morphs on those parts.
The streamer retains each available path once and reports retained file count,
NIF count, TRI count, byte total and paths missing from the active data VFS.
`Game.PrecacheCharGenClear` drops the buffers and resets residency and byte
counts. The asset buffers are CPU-side file data. This does not upload preview
meshes or apply morphs to a rendered actor.

The CK `engine-refs/ck-wiki/Game_Script.wikitext` defines the paired cache and
clear semantics. xEdit's `wbRecord(HDPT)` and UESP's `HDPT.wikitext` define the
model field and the NAM0 chargen TRI path.

morph values. It is a menu summary, not rendered character appearance.

`godot/ui_07_race_menu_test.gd` opens the live menu, presses a different race
record and sex button, and confirms through the connected Control callbacks.
Run it with `SKYRIM_DATA` set and the extension loaded in
`target/godot-check`.

The [large world physics survey](physics.md#large-world-boundary-survey-2026-09-19)
records the signed cell offsets, tolerances and Jolt measurements for the
`large-world-physics` branch. Its Rust mesh conversion test and Godot server
script cover sampled positions through 256 cells per horizontal axis. Rendering,
origin shifting and a full mod-worldspace streaming test remain outside that
survey.

Crate `soe-godot-host` (`crates/godot-host`). A GDExtension in Rust on godot-rust gdext
0.5.5 (crates.io 2026-08-09, the same version as `engine-refs/gdext`), API floor Godot 4.4
because Jolt became the built-in physics there. The crate builds and tests with cargo alone.
With `--no-default-features` the `godot` dependency is gone and what remains is the logic.

### Cart and horse host tether

`CellStreamerNode::tether_bodies` creates a Godot pin joint between caller-
supplied cart and horse body RIDs and keys ownership by both resolved
reference IDs, each encoded as `(file << 32) | object`. Creating another link
for either endpoint first frees the old one. `untether_reference` frees links
for explicit untethering. The player
sync path also releases a link when either endpoint is missing or disabled,
and `clear` or node teardown frees every remaining joint. The caller supplies
the rigid bodies and local pivots. The host API does not infer cart bones or
build the wheel, pole, chassis or horse rig from NIF collision data.

## Layout

### Movable reference physics, HST-12

`DataSource::placed_cell_colliders` retains the NIF body motion and mass from
`soe-physics`. `add_placed_statics` installs fixed and zero-mass colliders as
static bodies. It groups the remaining colliders by owning FormID and installs
one `PhysicsServer3D` rigid body per reference. The body owns its shape RIDs.
Concave triangle meshes are skipped for rigid bodies, since Godot's rigid
solver requires convex collision. The body's local shapes include authored
scale and NIF offsets, while its server transform carries reference position
and rotation. This follows `nifxml/nif.xml`'s mass and motion fields and
`godot-docs/classes/class_physicsserver3d.rst`'s body modes and state API.

Each gameplay tick first applies game-side moves and disable state to the
bodies, then reads simulated transforms back into `World`. The same tick's
render pass reads `World`, patches merged mesh members and updates `MultiMesh`
instance transforms. The host writes only the existing position and rotation
fields of an enabled reference. It frees body and shape RIDs on disable or
despawn, creates them again on re-enable, and frees a cell's resources on
clear, replacement, exit or exterior release. `collision_stats` reports live
body, shape and movable counts. `movable_reference_poses` and
`rendered_reference_pose` expose the host state for integration checks.
`render_alignment_error` reads the uploaded mesh vertices or `MultiMesh`
transforms and reports their largest difference from the cached World pose.
`PlayerNode` exposes `gameplay_reference_pose`, `set_reference_enabled` and
`despawn_reference` for host controls and integration checks.

`godot/hst_12_movable_test.gd` uses the installed Bannered Mare cell, applies
an impulse to an authored movable reference, then checks World and rendering
pose agreement and the disable, despawn and unload cleanup paths.

`CellBodies::grab_movable_reference` changes one resident movable body to
kinematic mode. `move_grabbed_reference` updates its PhysicsServer3D transform
from a finite Skyrim-space position. `release_movable_reference` returns the
body to rigid simulation at that pose. Each `CellBodies` collection permits
one grabbed reference at a time. The game grab state and these host operations
are separate APIs, and PlayerNode does not yet wire a targeting or held-input
loop between them.

### Spawn Hazard prerequisites, MAG-61

The MAG-61 audit on 2026-09-20 found no host path for HAZD or PHZD.
`DataSource::stream_cell_id` accepts placed REFR records and skips PHZD.
The shared `data_source::models::model_for` selector accepts STAT, FURN,
DOOR and CONT, excluding HAZD even when a generic reference names that base.
Its mesh builder bakes reference placement into surface vertices. The
collision builder uses the same model selector and installs static bodies.

`PlayerNode::advance_game` updates the player, advances Game and synchronizes
the player's pose or cell resources. It does not install resources for new
runtime references or consume hazard expiry notifications. The existing
`World::place_at` therefore cannot by itself satisfy the Spawn Hazard check.
HST-04 owns reflection of runtime reference changes. MAG-61 depends on it
before adding hazard-specific collision/effect resources and their cleanup.

Local `CommonLibSSE-NG/include/RE/H/Hazard.h` represents a hazard as a placed
reference with owner, age, lifetime, target timer, radius and magnitude.
`include/RE/S/SpawnHazardEffect.h` retains the spawned reference handle.
Those fields belong in the shared game state, with host resources derived
from it. `uesp-modfileformat/PHZD.wikitext` distinguishes a placed hazard
from its HAZD base. The record survey and additional gameplay prerequisites
are in [game.md](game.md#spawn-hazard-prerequisite-audit-mag-61).

This audit adds no host runtime code or scene nodes and performs no Godot
run. Hazard contacts, rendering, resource cleanup and their costs remain
unmeasured. Static world collision is not evidence of hazard execution.

### Modules

| Module | Depends on Godot | Job |
|---|---|---|
| `units` | no | Skyrim units and axes to Godot metres and Y up |
| `streaming` | no | which cells to load and unload around a focus, with budgets and hysteresis |
| `mesh` | no | a decoded NIF shape to `ArrayMesh` surface arrays, with validation |
| `facegen` | no | apply named TRI morph weights to head-part meshes and compose ordered NPC tint masks into DDS files |
| `terrain` | no | LAND heights to Terrain3D regions |
| `data_source` | no | a real `Data` folder through `esm`, `bsa`, `nif` and `dds` to loaded cells and exterior blocks |
| `batch` | no | a cell's surfaces merged per material set, with the side table back to each reference |
| `camera` | no | first and third person placement in Skyrim units, the basis Godot wants |
| `input` | no | the vanilla `controlmap.txt`, key bindings, and a frame of input into movement intent |
| `godot_layer` | yes | the `CellStreamer` node, `RenderingServer` instances, `array_mesh_from`, `heightmap_image_from` |
| `godot_player` | yes | the `PlayerNode`: devices in, physics character and camera out |

The Godot layer decides nothing. That is what keeps the tests engine-free, and it is the
seam a VulkanSceneGraph or other host would reuse: everything above the layer is plain data.

`facegen::build_head_part_surface` takes one already resolved head-part NIF mesh, a parsed
`FRTRI003` file and named morph weights. It applies the deltas to a fresh mesh copy and
passes the changed geometry through the same axis conversion, bounds and surface validation
as streamed meshes. Normals are generated from the changed geometry. The caller resolves
the NPC's `PNAM` head-part links and maps `NAM9` values to morph names.

## FaceGen tint textures, EDT-23

`facegen::generate_face_tint` accepts decoded RGBA8 masks in the order supplied by the
caller. Each mask alpha is multiplied by its layer opacity, and its color is blended over
black. The result is encoded as one 512 by 512 BC1 sRGB DDS level. `face_tint_path` returns
`textures/actors/character/facegendata/facetint/<plugin>/<FORMID>.dds`, where `FORMID` is
the eight digit uppercase local form id used by Skyrim asset tools.

The path follows Mutagen's `Npc.GetResolvedAssetLinks` implementation. xEdit's TES5
definitions identify each race tint asset by layer index, texture path and mask type.
TiltedEvolution's `FaceGenSystem` passes each tint texture, color, alpha and type to the
engine tint renderer and allocates a 512 by 512 surface. Its `CreateTints` call is an engine
function, so the blend arithmetic here is a deterministic editor export rule covered by a
synthetic fixture. PyNifly was named in the backlog read-first list but is absent from the
local reference corpus, which was unavailable from this worktree during conflict resolution.

### Game native presentation signals, SCR-29

`PlayerNode` drains transient `GameUiRequest` values from the shared `World`
after each gameplay tick. It emits `race_menu_requested`,
`screen_blood_requested(value)` and `bink_requested(file_name, interruptible,
mute_audio, mute_music, letterbox)` with the values supplied by the Papyrus
native. A change to `World::hud_cart_mode` emits
`hud_cart_mode_changed(enabled)`. These signals are the host presentation
boundary. A connected UI or media presenter supplies the corresponding race
editor, overlay and video playback.

## EffectShader playback, MAG-14

`CellStreamerNode` consumes transient EffectShader requests from the shared
game World. It keys active visuals by `(EFSH, target reference)`, so a second
Play refreshes the existing RenderingServer instance. Stop frees it, finite
durations free it after elapsed host time, and leaving the tree frees every
remaining instance. Each active effect owns one sphere mesh instance and one
material, with no scene node. The current material is a generic blue emissive
fill. EFSH texture inputs, authored colors, blend modes, fade curves and
attachment to skinned surfaces are not read yet.

## Invisibility rendering, MAG-34

`SkinnedActorNode` reads invisibility from the actor's shared World state and
sets visibility on every RenderingServer mesh instance. The initial appearance
bind reads that state before creating instances, and later appearance rebuilds
reuse the same cached state. Animated and ragdoll poses share these mesh
instances, so the visibility change applies in either render mode. A later
process update restores visibility after the effect expires or breaks. The
game tests cover detection and break actions, while host unit tests cover the
RenderingServer visibility mapping. Live Godot coverage of actors outside
`SkinnedActorNode` remains open.

`godot/` holds a project stub and `soe.gdextension` pointing at `target/`, so `cargo build`
plus opening `godot/` in a 4.4 or later editor loads the extension. The initial
round had no Godot binary. Later sections record actual host runs, and the
2026-09-19 reference checks use the installed Godot 4.7.2 Flatpak.

## Units and axes

One Skyrim unit is 1.428875 cm, a cell is 4096 units (58.5 m), LAND heights are in eighths
of a unit. Skyrim is Z up with Y north, Godot is Y up with north at -Z. The map
`(x, y, z) -> (x, z, -y)` has determinant +1, so handedness and winding survive it. Godot
draws clockwise faces as front faces. Whether Skyrim's shapes come out counter-clockwise
after the map is not settled until a real NIF renders, so `MeshOptions::flip_winding` is a
switch, default on, and the generated normals follow it.

## Determinism

`data_source::workers::ordered_map` returns builder results in input order for
both sequential and threaded runs. The TST-02 test hashes those results into
the game's per-tick `host.builders` partition, so worker output participates
in the same divergence report as world and VM state.

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

### Editor cell tree and selected view, EDT-03

The Godot project enables `addons/soe_cell_editor`, an `EditorPlugin` dock with a
worldspace and cell tree beside a `SubViewport`. After loading a Skyrim `Data` directory,
`CellStreamerNode::editor_cell_tree` exposes winning `CELL` records with resolved form ids,
editor ids, exterior worldspace and grid coordinates, and indexed placed-object counts.
Interiors appear under their own tree group. Selecting a row calls
`CellStreamerNode::open_cell_id`, which resolves that exact cell id and draws it through
the same `DataSource::stream_cell_id` and `CellStreamerNode::add_cell` path used by the
host renderer. It clears the previous selection first, so the viewport contains one cell.
The viewport camera frames the rendered bounds after selection.

This milestone supplies a read-only cell tree and rendered cell preview. It does not edit
cell records, expose reference selection, or add transform gizmos. The Godot project opens
the cell view only after the author enters a valid `Data` directory or launches the editor
with `SKYRIM_DATA` set.

`PlayerNode` starts at most `soe_game::EXTERIOR_CELL_LOADS_PER_UPDATE` exterior cell loads
per update. The same value is exposed to Papyrus as the `Exterior cell loads per update`
budget category through `Utility.GetBudgetCount`, `GetBudgetName`, `GetCurrentBudget` and
`OverBudget`.

`stream_cell_id` walks the cell index, which keeps a placed object whose winning
record carries the deleted flag out of the drawn list and files it under
`deleted_references` instead (`docs/esm.md`, "Dirty plugins"). A deleted
reference still resolves through the load order and the game starts it disabled,
so the host never draws one and never has to test for it.

`StreamPlan` is the unit the host schedules. `update_with_budget` caps its selected loads by
triangle and byte totals when the caller supplies a `StreamCost` for each candidate cell.
Costs may be measured or estimated. Loads stay nearest first, and a cell that does not fit
either remaining limit is deferred while later smaller cells may be selected. A zero limit
is unlimited. The ordinary `update` keeps its cell-count behavior and does not apply mesh
budgets. `SurfaceArrays` exposes triangle counts and its vertex and index buffers, which
callers can use to calculate or estimate costs. The walking player path measures
prepared surface arrays and calls `update_with_budget` when its exported
`exterior_triangle_budget` or `exterior_byte_budget` is positive. The limits
apply to selected mesh uploads. Preparation may decode more candidates on that
tick so a smaller later cell can fit. Zero on both exported settings leaves
the earlier cell-count behavior.

`CellStreamerNode::stream_object_lod` reads and draws existing BTO object LOD
blocks from `meshes/terrain/<world>/Objects`. It uses the world's LOD settings
for the block anchor and level, and the same capped distance streamer and byte
budget as BTR terrain LOD. Blocks inside the full resolution radius are not
requested. Each rendered block remains keyed by its worldspace and
`LodBlockCoord`, and release or budget eviction frees its instances. Use
`update_object_lod` to move the BTO streamer as the focus changes, and
`clear_object_lod` to release it. BTO surfaces retain the worldspace form as
their source identity because the merged file does not encode individual
placed-reference identities.

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

```text
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

### Editor marker survey (2026-09-20)

`DataSource` keeps the form-level bit 23 filter for FURN and DOOR. For models
selected through STAT, CONT, FURN and DOOR, it also removes only TriShape and
BSTriShape blocks reached through an `EditorMarker` node. xEdit's
`Core/wbDataFormatNif.pas`, `TwbNifBlock.GetIsEditorMarker`, is the source: it
defines an editor marker per named NiObjectNET block. Collision still reads the
full model, so the visual filter does not remove physical geometry.

The installed base-master survey found 248 marker shapes in 113 models. Of
those models, 111 contained only marker shapes. The two mixed models retained
their ordinary sibling shapes. The production loader test compares its emitted
surface and triangle totals with the decoded nonmarker meshes for every
available surveyed model. It found 41 ordinary visible controls and nine
baseline VFS-not-found paths, which it reports without treating them as marker
filter failures. The serial one-CPU run completed in 18 seconds and peaked at
13.6 GiB RSS.

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

```text
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

```text
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

## Environment maps, RND-12 (2026-09-20)

The host carries the authored environment cubemap, mask and scale through
cell loading, texture caching, material identity and batch identity. This
extends the emission material path from RND-06, recorded as its dependency.
The NIF reader already exposes these values, so no parser change is needed.
The environment mapping flag gates the cubemap slot. Environment-map and
multilayer-parallax scale fields retain their values.

Sources read before implementation:

- `nifxml/nif.xml` declares shader flags, slots four and five, and the scale
  fields of BSLightingShaderProperty.
- `skyrim-community-shaders/package/Shaders/Lighting.hlsl` samples a cube in
  the world reflection direction, multiplies by the authored scale and the
  mask's red channel, and falls back to normal-map alpha without an explicit
  mask. Its static cubemap path informed this implementation.
- `openmw/components/nifosg/nifloader.cpp` was inspected, but its environment
  shader type falls back to the default shader and does not supply this path.
- `godot-docs/classes/class_cubemap.rst` and `class_imagetexturelayered.rst`
  describe samplerCube upload and face order, positive X, negative X,
  positive Y, negative Y, positive Z, negative Z. The spatial shader reference
  supplies view-space normals and the inverse view transform.

The existing DDS reader supplies all six faces and their mip chains. Cubes
must contain six square 2D faces. Incomplete legacy cubes and cube arrays
are rejected before upload. Compressed faces retain their blocks through
Godot upload and share the source texture cache. A GPU cubemap cache shares
one resource across material variants. No dependencies or per-reference
nodes were added.

Environment materials use Godot shader materials. The shader converts the
reflected view ray into world space, then into Skyrim's Z-up axes before
sampling the authored cube. It adds the masked reflection to the indirect
color term while retaining diffuse lighting, normal mapping, emission,
alpha handling and optional decal depth bias. A mask is linear data, while
cubemap color uses the shader's sRGB conversion. Normal alpha remains a
reflection mask and never becomes transparency.

`environment_fixture_preserves_cube_faces_masks_and_scale` writes NIF,
plugin and DDS inputs independently and exercises the production cell
loader, including compressed face order, mip completion and material
separation. `incomplete_environment_cube_is_rejected_before_upload` checks
the typed rejection. `SOE_ENVIRONMENT_FIXTURE` retains generated inputs for
`godot/environment_test.gd`. That test checks six face directions, authored
scale, explicit red masks, fallback normal alpha, flag-off and zero-scale
controls, alternate cube and mask paths, and decal reflection in both cache
orders. Limits are declared before running: 0.08 sRGB channel error for
nonzero values and 0.03 for dark values. No proprietary asset is embedded.

## Emissive materials, RND-06 (2026-09-20)

The host carries BSLightingShaderProperty emissive RGB and multiplier into
Godot materials. Slot two is loaded as a glow texture only when
`SLSF2_Glow_Map` is set. The slot has other uses on skin and hair shaders,
so its presence alone does not enable glow. Authored emission without a glow
map remains a constant color.

The local references are `nifxml/nif.xml` for the slot and flag,
`OpenMW/components/nifosg/nifloader.cpp` for emissive color and multiplier,
`community-shaders/package/Shaders/Lighting.hlsl` for their multiplication
with the glow sample, and `godot-docs/classes/class_basematerial3d.rst` for
Godot's emission operator and energy multiplier. The NIF reader already
retains these fields. Only its independent test writer needed an emission
parameter. No format parser or new dependency was added.

The standard material uses Godot's multiply emission operator when a glow
texture is bound, and color-only emission otherwise. The decal
shader receives the same color, multiplier and sRGB texture. Material and
batch keys include all three so shared diffuse maps cannot combine distinct
emission states. The existing VFS and source texture cache resolve and share
glow maps, including loose-file overrides. Production geometry still uses
RenderingServer instances, with no new per-reference nodes.

`emission_fixture_preserves_authored_material_and_gates_shared_texture_slot`
constructs eight independent NIF materials through the existing NIF writer,
loads them through the cell pipeline, and checks authored values, texture
selection and separate material/batch keys. `SOE_EMISSION_FIXTURE` retains the
generated five-master, NIF and DDS fixture for `godot/emission_test.gd`.
The render test uses the production streamer, material cache and texture
upload. It checks standard and decal paths in both cache orders. Before its
first run, the limits were set to 0.08 sRGB error for authored linear emission
strengths 0.25 and 0.5, and 0.03 for dark pixels. A non-emissive control stays
dark, a black glow texel masks emission, and slot two without the glow flag
does not mask constant-color emission. No installed game asset is required.

The Godot 4.4.1 Forward+ run passed all sixteen cases on 2026-09-20.
Readback red values were 0.5373 and 0.7373 for the two authored strengths,
and zero on dark controls and black glow samples. Explicit `force_draw`
keeps the automated fixture independent of desktop window visibility. The
full gate passed 1,197 top-level Rust tests and eleven Python tests, with
all optional tools present. The determinism tests also run three child
process checks, which are excluded from that top-level count.

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
`CellStreamerNode::stream_cell` keys a `StandardMaterial3D` cache on diffuse and normal
paths, alpha mode, cutout threshold and double-sidedness. Identical material states share
one material, and a separate path cache shares texture uploads across variants. The diffuse image becomes `TextureParam::ALBEDO`, and when a normal map is
present it becomes `TextureParam::NORMAL` with `Feature::NORMAL_MAPPING` turned on. Skyrim's
BC7 or BC5 normal maps carry a specular mask in the alpha channel, not opacity, and Godot's
`normal_texture` only ever reads RGB for the surface normal, so that alpha channel is inert
here rather than fed into anything, until a specular term is added. Alpha mode comes from
`MaterialDesc::alpha`: `Opaque` leaves `Transparency::DISABLED`, `Cutout(threshold)` sets
`Transparency::ALPHA_SCISSOR` with that threshold, `Blend` sets `Transparency::ALPHA`.
`double_sided` sets `CullMode::DISABLED`, otherwise Godot's default back-face cull applies.

`crates/godot-host/tests/real_cell.rs` reports the new stats alongside the existing ones:

```text
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

The `StandardMaterial3D` cache moved from a per-call local to the node. It now includes
alpha and culling state in its key, with a separate path-keyed texture upload cache,
so a texture uploaded for one cell serves every later cell that names it. `last_stats`
reports the materials a call created, not the cache size.

### Runtime enable and placement reflection, HST-04

A batch bakes each source surface's placement into its vertices once, at load
time. Nothing updated a batch again after that, so a script-driven move,
disable or Creation Kit style drag never reached the mesh or its collision
after the first tick. `CellStreamerNode::sync_reference_changes(&mut World)`
closes that gap and is called once a tick from `PlayerNode::advance_game`,
before anything that frame reads geometry or collision, so an enable or
placement change lands the same tick it is written to `World`. The host reads
`World::ref_placement` every tick. HST-12 also writes simulated movable poses
into these same fields.

Collision and rendering both gate their expensive path on a per-reference
cache of the last applied `(visible, Iso)`, called every tick but doing real
work only on an actual change:

- Collision (`CellBodies::sync_placements`, in `godot_physics.rs`) keeps
  the cache on each `StaticEntry`. An unchanged reference costs one
  comparison. A changed one either moves the body between `space` and
  `Rid::Invalid` (cheap, so this part runs even for a reference this cache
  has never seen), or, for an actual placement change, frees the body and
  its shapes and rebuilds them through the same `flatten`/`build_body` path
  `add_placed_statics` used, since a Godot static body's collision shapes
  bake their world placement into their own data or local transform and
  cannot be nudged by moving the body alone.
- Rendering keeps its cache on the `CellStreamerNode`, keyed by reference
  across every batch. It rebuilds a merged `ArrayMesh` when a member changes
  and updates a `MultiMesh` member's transform when its reference changes.
  All batches containing the changed reference update on the same tick.

Either side's delta is always computed against the reference's placement at
load time, never against the last placement applied, so a reference moved
many times over many ticks accumulates no drift. An earlier version of
`sync_placements` reassigned its baseline to the latest placement after each
move, which is exactly the composing-onto-an-already-moved-copy mistake the
"always rebuild from an immutable baseline" rule above exists to avoid, and
was caught and fixed before any external run exercised a second move of the
same reference.

Hiding a reference also differs by side, because each side's resource model
differs. Collision removes the body from the physics space
(`body_set_space(body, Rid::Invalid)`), matching vanilla's "not present in
the simulation" contract, and a later re-enable puts it back with
`body_set_space(body, space)`. Rendering cannot remove vertices, since every
reference after the hidden one in the shared index buffer would need
renumbering, so a hidden member's whole vertex range collapses onto its first
vertex instead, a zero-area triangle fan that draws nothing and costs no
buffer resize.

Both sides rebuild from an immutable, never-mutated baseline recorded at load
time (`StaticEntry::source` for collision, the new `Instance::arrays` for
rendering) rather than composing a move onto an already-moved copy, so a
sequence of small edits cannot drift from what `World` actually holds. The
move itself is a delta against that baseline's own placement:
`delta = target_iso.then(&origin.inverse())`, where `origin` is the
reference's placement as first loaded (`BatchTableMember::origin`, sourced
from `LoadedSurface::placement`, `Iso::IDENTITY` for LAND) and `target_iso`
comes from `World::ref_placement`. Applying `delta` to a baked point or
direction reproduces the moved result directly, with no hand-derived
world-space transform to keep in sync with the loader's own placement math.
Position, normal and tangent arrays each round trip through `units.rs`'s
Skyrim/Godot converters before and after the delta applies, since `delta` is
built in Skyrim space like every other placement in this crate.

HST-12 also reads rigid-body poses back into World on each gameplay tick. A
per-reference prune of the render cache on cell unload remains absent. It is
cleared wholesale on `clear` and `exit_tree`.

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
winners once for the whole block, `decode_land_heights` turns each cell's 33 by 33 `VHGT`
into world-unit heights, `reconcile_seams` stitches the block's shared edges (see RND-15
below), and `terrain_surface_from_heights` turns the reconciled heights into 32 by 32 quads
in Skyrim units, `VNML` normals when present, wound counter-clockwise from above so
`build_surface`'s flip lands them like a NIF's faces. `CellStreamerNode::stream_exterior`
is the GDScript entry point and `last_stats` carries the block's totals, the worldspace's
editor id and the cell list.

`WhiterunOrigin` is grid (4, -2) of `WhiterunWorld`, whose `WNAM` parent is Tamriel with the
`PNAM` "use land data" bit set (CommonLib's `kUseLandData`). Only three of the nine cells
around the origin carry their own `LAND`, the city draws Tamriel's under the other six, so
`resolve_exterior_block` also collects the parent's cells at the block's grid positions and
`stream_exterior` takes the parent's `LAND` where the child has none. Nine of nine cells then
have terrain. Whether the shipped engine draws the parent's land under a child cell that has
its own is not settled here, the override wins.

For each terrain cell, `terrain_texture::bake` samples four 128 by 128 quadrant images. Each
quadrant starts with its `BTXT` base texture, or `textures/landscape/dirt02.dds` when its
texture is zero or unavailable. Each `ATXT` layer resolves its `LTEX` record, `TNAM` texture
set and `TX00` diffuse path, then blends into the base using the `VTXT` opacity points on the
17 by 17 quadrant grid. Opacity is bilinearly interpolated. Source textures repeat four times
per quadrant. The baked RGBA image is attached to the terrain surface's ordinary albedo
material and uploaded through the existing texture path. This uses the LAND/TXST layouts in
TES5Edit and the UESP LAND description. Terrain3D's control map is a separate layer-index
representation, but this host already streams one ordinary mesh per cell, so baking keeps
the rendering path on Godot's existing mesh and material servers without adding Terrain3D.
`rnd_01_terrain_texture_check.rs` checks the installed Whiterun data path through the
`LoadedSurface` texture payload. `CellStreamerNode::terrain_surfaces` exposes the uploaded
terrain materials for `godot/rnd_01_terrain_texture_check.gd`, which checks the live
RenderingServer instances and their albedo images. `godot/pan_capture.gd` orbits and pans that
block through 25 shots for a quick visual sweep. Distant geometry is Bethesda's precomputed
LOD (the `BTO` and
`BTR` files under `meshes/terrain`, to be consumed, never generated), not loaded, so nothing
beyond the requested ring exists. No water plane. `WhiterunWorld` lists grid (0, 0) twice among
its winners, both streamed. Interior streaming by proximity is still unstarted.

**RND-15, the 2026-09-23 sky gaps.** `pan_capture.gd` showed thin sky-coloured gaps along a
few terrain cell edges from four of its 25 angles. Cause: each cell's terrain used to mesh on
its own from its own decoded `VHGT`, so two neighbouring cells only met exactly where their
height data already agreed, and Bethesda's own `LAND` agrees at every edge inside one
worldspace by construction. The gap sat at the boundary between the block's three
`WhiterunWorld` cells and the six cells falling back to Tamriel's `LAND` at the same grid
position: Tamriel's landform there was never sculpted to meet WhiterunWorld's city terrain, so
the two data sources left a height mismatch of a few world units at the cells where they meet,
wide enough to show sky through from a shallow angle. The fix, `reconcile_seams` in
`crates/godot-host/src/data_source/exterior.rs`, decodes every block cell's effective heights
(own or parent) first, keyed by grid position rather than cell id, then groups every array
index across the block that stands for the same physical vertex, up to 4 of them at an
interior block corner, and sets the whole group to its members' average before any cell's mesh
is built. A vertex the data already agrees on averages to the value it already had, so an edge
already agreeing is left unchanged in practice. Averaging moves both sides of a real mismatch
an equal small amount rather than picking a "correct" worldspace, since nothing here says
WhiterunWorld's authored terrain should win over Tamriel's at a border neither one owns.

An earlier version of `reconcile_seams` closed the parent-worldspace gap with two sequential
passes per cell, east-west against the east neighbour then north-south against the north
neighbour, both mutating the same heights map. A cell with both an east and a north neighbour
inside the block, 4 of the 9 `WhiterunOrigin` cells, has its north-east corner sample on both
edges, so the north-south pass re-read a value the east-west pass had already moved and
overwrote it again, leaving the east neighbour's own copy of that same corner stale. Since the
two passes read their processing order from a `HashMap`'s key iteration, which the standard
library gives no cross-run guarantee for, which corner won could also change between two
`DataSource::open` calls against the same install. That compounded the sky gaps at exactly the
point most likely to show one, a mesh corner every touching triangle shares, and broke
reproducibility for anything that diffs or caches streamed terrain, `pan_capture.gd`'s own
regression captures included. The current fix resolves each physical vertex from all of its
copies at once, up to 4 at an interior block corner, instead of from whichever pairwise pass
happens to run last, so the result no longer depends on iteration order.

### PRF-02: streaming a block one cell at a time

`docs/performance-limits.md` (Recorded validation, 2026-09-19) reports the real exterior
benchmark (`WhiterunOrigin`, radius 1, 3x3 block) stopping during startup at 8,232 MiB RSS
against its 8,192 MiB budget, before a single frame sample was taken. `DataSource::stream_exterior`
built the whole block first (`Vec<(ExteriorCell, LoadedCell)>` for all nine cells) and only then
handed it to `CellStreamerNode::stream_exterior`, which converted and uploaded each cell to
`RenderingServer` in a second pass. The whole block's decoded meshes and textures stayed
resident through that second pass on top of whatever the RenderingServer upload itself held,
so the peak fell where both were alive at once.

Measured headless (`cargo run --example`, a scratch probe read against `/proc/self/status`
`VmRSS`, deleted before this change was committed): opening the data source (five base
masters plus load order and cell index) reaches about 1.3 GiB RSS, and streaming the nine-cell
Whiterun block on top of that reaches about 2.1 GiB, so the block's own raw geometry and
texture data is roughly 700 to 800 MiB. Two other candidate causes named in
`docs/performance-limits.md` and `docs/performance.md` were measured and ruled out: lazy
archive indexing (deferring `Archive::open`'s index read) costs about 0 MiB across all 21
non-voice archives (97,417 files) in this load order, so it only helps wall-clock open time,
not RSS, and the block carries 160 unique BC-compressed textures (about 325 MiB) with zero
RGBA8 fallback decodes, so texture decode bloat is not the driver either.

`DataSource::stream_exterior_block` (used by `crates/bench` and the `real_exterior` test,
which read the whole block at once to report per-cell totals) is unchanged. A new sibling,
`DataSource::stream_exterior_each`, streams the same cells in the same order but calls a
closure with each `ExteriorCell`, its `LoadedCell` and whether it carries a terrain surface,
one cell at a time, instead of collecting them. `CellStreamerNode::stream_exterior` now calls
`stream_exterior_each` and does the RenderingServer conversion and upload inside that closure,
so each cell's raw CPU-side buffers are dropped before the next cell is read. Peak RSS during
the streaming pass now holds at most one cell's raw data (roughly 80 to 90 MiB average for
this block) instead of all nine (roughly 700 to 800 MiB), a reduction on the order of 600 MiB
against a budget the run was missing by 40 MiB.

This closes the CPU-side half of the gap between the headless baseline (about 2.1 to 3.3 GiB,
the higher figure from the `real_exterior` integration test's own harness overhead under
`/usr/bin/time -v`) and the reported 8,232 MiB windowed failure. The remainder is the Godot
process itself: renderer and driver baseline, `RenderingServer` GPU-side uploads, and
`ArrayMesh`/`StandardMaterial3D`/`ImageTexture` resource overhead, none of which this change
touches. Whether the fix closes the 40 MiB gap in practice can only be confirmed by the
windowed run itself (`tools/performance.py --backend godot --scenario exterior`), which is
reserved for the lead measuring on an idle machine, not for an agent. See `docs/performance-limits.md`
for the pass or breach result and the frame-time percentiles once that run has been made.

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

The animation stack was finished and invisible: `crates/hkx` reads skeletons,
clips and behavior graphs, and since `behavior-2` it walks the vanilla
`mt_behavior.hkx` and plays the standing idle at full weight. Nothing reached a
screen. This connects the two.

### Runtime animation requests, HST-10

An appearance-bound `SkinnedActorNode` keeps its actor `GlobalFormId` and a
cursor into the shared world's animation event log. On each `advance`, it
passes new events for that actor to its HKX interpreter before updating the
graph. Events for other forms and events already present when the actor was
bound are skipped. The cursor advances across the whole log, so one request is
submitted at most once to that host actor. The interpreter's emitted graph
events continue through the node's `animation_event` signal after its pose is
uploaded, once per event returned by `take_events()`. That interpreter method
also returns submitted input events, so the signal does not report that a
requested animation finished. `World::animation_event` currently records both
script requests and listener notifications. The host does not instantiate a
graph for non actor references, which `ObjectReference.PlayAnimation` requires.

This uses Godot's `RenderingServer` skeleton RIDs and
`skeleton_bone_set_transform` for pose upload, as documented in the local
`godot-docs/classes/class_renderingserver.rst`. The request bridge only covers
actors explicitly bound through `PlayerNode.bind_actor_appearance`. Animated static references
still use their NIF transform controller path, and general reference animation
graphs are not instantiated by this bridge.

Three pieces, none of which touch Godot except the last:

- `soe_hkx::pose::to_model_space_into` composes the interpreter's local pose
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

After each pose upload, the actor node emits every graph event by its authored
name. `PlayerNode.bind_actor_appearance` connects those signals to the shared
game and reports graph unload when the visual leaves the scene tree. This
resolves Papyrus event listeners and animation waits against the bound actor.

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
applies it as `M * v`. `Affine::from_nif_skin` transposes the raw bind
transforms. The earlier claim that scene-node rotations needed no transpose
was disproved by the washbasin regression below. Placed scene transforms now
receive the same convention conversion before parent-child composition.

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

## Sound out (2026-09-18, branch `playback`)

`audio.rs` is the conversion and the slot rules with no engine dependency,
`godot_audio.rs` is the `SoundBank` node. Same split as `skinned.rs` beside
`godot_actor.rs`.

### Godot's mixer, not a second one

The plan row offered `cpal` or `rodio`. Neither is taken. gdext already links
Godot's audio stack, and the user's ruling on physics was that where the engine
ships a subsystem we use it rather than linking our own. A device crate beside
it would mean two mixers, two device backends and two places for a format to be
wrong, for no capability Godot lacks.

This is the one host module that does **not** bypass the scene tree. The cell
streamer and the skinned actor talk to `RenderingServer` by RID, but positional
audio in Godot is a property of `AudioStreamPlayer3D` and `AudioServer` has no
RID call that places a sound in space. Nodes are the only route to the
attenuation the done check asks for.

`AudioStreamWav` carries the samples, not `AudioStreamGenerator`. A generator
exists for audio that has not been produced yet, pushed a block at a time from a
callback. Every sound here is fully decoded before it plays, so a generator
would add a ring buffer and a way to underrun and nothing else.

### What the conversion has to do

`soe_audio::Pcm` is interleaved 32 bit float. `AudioStreamWav` reads 8 bit, 16
bit, IMA ADPCM or QOA and has no float format, so `to_i16_bytes` clamps and
scales to 16 bit. Clamping rather than letting a sample wrap: a decoder that
overshoots by a hair would otherwise produce a click of the opposite sign.

`to_mono` folds multi-channel down before a sound is placed, because a stereo
buffer keeps its own left and right image and fights the position the 3D player
is giving it.

`VoicePool` holds 16 slots. A voice line may take the oldest effect when the
pool is full, an effect may never take a voice line, and a sound never evicts
its own kind. So a crowded room drops footsteps rather than cutting dialogue.

### Measured on the installed game

`tests/real_audio.rs`, gated on `SKYRIM_DATA`. Footsteps are `.wav` and need no
decoder beyond `hound`, so they play in the default build. Voice lines are xWMA
inside a FUZ and need the `xwma-ffmpeg` feature.

| Check | Result |
|---|---|
| Footstep conversion, 200 files | 200 carried signal, 0 silent, 0 failed to read |
| Footstep sample rates seen | 22050, 32000, 44100 |
| Voice lines sampled | 57 of 60 carried signal |
| Loudest sampled line | `maleelfhaughty/voicepowers_shout01b`, peak 0.85, 339 byte LIP |

The voice test surveys rather than taking the first path alphabetically, and
the reason is worth keeping. That path is
`crdogvoice/creaturedialoguedog__0005b2f0_1.fuz`, which decodes to peak 0.0015.
Creature dialogue entries are routinely silent placeholders, so asserting on the
first one measured Bethesda's content rather than this crate's conversion. The
survey is what separates the two: if the decoder were broken, most of the 60
would be silent, and 57 are not.

### Untested

The last step, a speaker. These tests prove the chain up to the exact bytes
`AudioStreamWav` is handed, which is where a conversion bug lives. Whether a
device makes noise needs a windowed Godot run with a `SoundBank` in the tree,
`open_data` pointed at the install, and `play_effect` called with a footstep
path. Footsteps are driven by the character controller. Dialogue response
audio is now driven by the dialogue menu.
`SoundBank::open_data` and the profile-taking `DataSource` entry point build
their VFS from resolved profile archives and loose roots. Both direct paths
retain the supplied data directory. The selected
`sLanguage` admits matching voice archives and skips competing locales. A
missing selected voice archive reports an error and does not fall back to
English. `DataSource::open(data_dir)` retains its exact supplied data root and
does not require voice assets, since it still builds records from the five base
masters rather than a profile's complete plugin list.

## Input and camera (2026-09-18, branch `input`)

`camera` and `input` carry the rules and have no Godot dependency, so both test headless.
`godot_player` reads devices, steps the physics world and writes one camera transform.

### The binding file is Bethesda's

`interface/controls/pc/controlmap.txt` ships inside `Skyrim - Interface.bsa` and documents its own
format in its header: a user event name, then the keyboard, mouse and gamepad ids that fire
it, three remappable flags, and an optional grouping flag. Blank lines separate input
contexts. Reading that format is what makes a mod's control map rebind this engine the way
it rebinds the real one, so `input::ControlMap` parses it rather than inventing a config
file of our own. It takes all 169 mappings across 18 contexts of the PC file, including
the chord syntax (`0x2a+0x0f`), the alternative syntax (`a,b`), the optional flag field, and
event names that contain spaces, which is why the line is split on tabs and not whitespace.

The September 18 tests selected the first archive entry ending in
`controlmap.txt`. The archive also contains console maps, so the reported 135
mappings, virtual-key encoding and absence of keyboard movement described the
wrong platform. Running PlayerNode on September 19 exposed the bad lookup.
The tests now read the exact PC path and establish these facts:

| Question | Answer | How it was settled |
|---|---|---|
| Which key table does the keyboard column use? | DirectInput scan codes | W is `0x11`, F1 is `0x3b`, F12 is `0x58`, Enter is `0x1c`. CommonLibSSE-NG's `include/RE/B/BSKeyboardDevice.h` agrees. |
| Does it carry the keyboard defaults? | Yes | The gameplay context binds movement, activation, jump, sneak, sprint, Run, Toggle Always Run, Ready Weapon and Toggle POV. |

`Bindings::vanilla_keyboard` provides a fallback when data is absent. The PC
map overrides it through the same VFS as other assets, so loose replacers win.
Only its gameplay context supplies gameplay actions. The hold-to-walk action
is named `Run` in that file, and Toggle Always Run uses Caps Lock. Camera
offsets remain this engine's choices, as `camera` documents separately.

The `Right Attack/Block` gameplay event is also registered from the PC control
map. Its press edge arms one right-hand melee contact in PlayerNode. The
authored animation's `HitFrame` marker still supplies the contact pose and
reaches the same shared Game path as other melee contacts. A single-key or
single-button action can also be pressed through Godot's Input action API,
while chorded bindings continue to require every device input.

MAG-22 adds `Left Attack/Block` and `Right Attack/Block` as held gameplay
actions. PlayerNode compares each frame with the prior held state and calls
`Game::begin_hand_cast` once on a press, then `Game::end_hand_cast` on release.
The actions use the PC control map's mouse, keyboard or gamepad binding. Losing
input focus releases both hand requests so a missed device-up event cannot
leave a hand latched.

### The camera basis

Skyrim's yaw is zero facing north and grows clockwise from above, so forward is
`(sin yaw, cos yaw, 0)`. Godot's camera looks down its own `-Z` and `units` already maps
Skyrim's north onto `-Z`.

`Eye::basis_columns` returns the three columns Godot wants, built from cross products rather
than from `Basis::from_euler`. The reason is narrow: a wrong Euler order or a missing sign
compiles cleanly and renders wrong, and it would sit behind the `godot` feature where no test
reaches it. As columns it is plain arithmetic in `camera`, and the tests check a known facing
in both directions, orthonormality, and right-handedness across the whole yaw and pitch
range. The pitch clamp at 89 degrees is what keeps the cross products from degenerating, and
a test asserts the clamp stays under 90 rather than trusting the constant.

### What drives what

Movement takes the flat forward only, so looking up never slows a walk. Speed comes from
`soe_physics`, no second copy: sneak gives `WALK_SPEED`, sprint `RUN_SPEED`, and the walk key
inverts whatever always-run is set to, so it reads `JOG_SPEED` or `FAST_WALK_SPEED` either
way round. Sneak and always-run are toggles held in `Driver`, not held keys. A sneaking
character cannot jump.

The character is `soe_physics::SimpleWorld`, not a Godot body, for the reason `docs/physics.md`
gives: the simulation's authority stays in the engine so a headless run and a rendered one
cannot drift. The third person boom raycasts against that same world and pulls in short of
whatever it hits, keeping `collision_padding` clear, so the camera cannot end up inside a
wall. Activating casts from the eye along the full look direction, pitch included.

### Untested here

The initial input branch had never executed PlayerNode in Godot. The
`host-player-run` follow-up adds `godot/player_test.gd` for that boundary and
`godot/walkthrough.gd` for interactive movement. InputMap is refreshed after a
late control-map load. Rebinding releases old held actions, full chords are
checked, and toggle edges are consumed once while single-key taps between
ticks are retained. PlayerNode releases mouse capture on exit.

The `device-bindings` follow-up consumes the mouse and gamepad columns.
Mouse motion drives look. Right/left modifier scan codes map to Godot's shared
modifier keys, so side-specific chords are not distinguished. The walkthrough
is a movement prototype. It does not activate game references, cross doors,
play footsteps or run the gameplay VM.

### Papyrus camera controls, HST-17

The CK `Game_Script.wikitext` describes `SetCameraTarget` as setting the camera
target actor, `ShowFirstPersonGeometry` as showing or hiding first-person
geometry, and `SetSittingRotation` as setting the sitting camera offset.
`skse64/scripts/vanilla/Game.psc` specifies that sitting offset in degrees.
`soe-game` stores these transient requests in `World::camera_controls`, and
`PlayerNode` applies them on its next game advance. A selected actor's current
reference position becomes the camera anchor. The first-person geometry flag
is retained as host camera state for the renderer. Sitting rotation offsets
the camera yaw while leaving movement facing unchanged.

`PlayerNode.camera_state()` reports the applied target FormID, geometry flag
and sitting offset for host inspection. When a skinned appearance is bound to
the player, the geometry flag controls its RenderingServer instances in first
person. Third-person view keeps the player appearance visible. The
`hst_17_camera_controls.rs` test exercises the state readbacks and verifies
the third-person camera yaw and position respond to the sitting offset.

### Mouse and gamepad bindings

`input::devices` translates the codes declared in CommonLibSSE-NG's
`BSWin32MouseDevice.h` and `BSWin32GamepadDevice.h`, cross-checked against
SKSE's `InputMap.cpp`, into the indices documented by Godot's `@GlobalScope`.
Godot exposes the left, right, middle and two side mouse buttons and vertical
wheel pulses. Bethesda mouse codes 5 to 7 have no Godot counterpart and remain
unsupported. Gamepad buttons use Bethesda's masks, with codes `0x9` and `0xa`
for the triggers. Stick codes `0xb` and `0xc` bind the `Move` and `Look` events.
These use the gameplay context and VFS overlay already used by keyboard input.

Each controller keeps its own state. A chord cannot combine buttons from two
controllers. Keyboard, mouse and pad bindings contribute to one action, so
pressing a second device while the action is held does not toggle it again.
Received short taps survive until the next simulation tick. Rebinding clears
pending presses and seeds held state without creating another press. Unplugging
a controller clears its buttons and axes while preserving presses already
received. Losing window focus clears all input, and focus restoration waits
for fresh events.

Sticks use a radial dead zone of 0.2 and rescale the remaining range to one.
Movement speed follows stick magnitude and diagonal input never exceeds the
selected movement speed. The strongest controller vector wins, with device
ID order breaking ties. Triggers fire above 0.5. Stick look uses elapsed time
and the exported `gamepad_look_speed`, initially 800 mouse units per second
before the camera's sensitivity. These are engine defaults, not measured
vanilla response curves.

InputMap publishes keyboard, mouse and pad button/trigger events. A chord
publishes its final button for inspection, while PlayerNode checks the whole
chord from received events. Synthetic `Input.action_press` remains available
for simple keyboard-only actions. Device-bound actions and chords require
input events. Godot 4.4 can report a synthetic action's just-pressed flag one
process frame after its held state. A latch consumes that delayed flag once.
Synthetic taps wholly between samples during that latch may coalesce. Physical
presses use the event queue and bypass the latch.

`godot/device_test.gd` checks mouse chords and wheel pulses, pad chords and
alternatives, short taps, mixed-device holds, trigger rebinding, disconnection,
focus restoration, stick direction and frame-rate independence, and automatic
physics/process input. Rust tests also check analog movement strength,
dead-zone boundaries, invalid axes and controller ownership. Physical device
drivers, vibration, hardware feel and non-US keyboard layouts remain untested.

The device and existing player scripts passed on Godot 4.4.1 and 4.7.2.
The 4.7.2 player run also loaded the installed PC control map, built 361
Bannered Mare collision bodies and moved the player after settling.

### Running the player (2026-09-19)

Build the release extension, import the project once, then run either script:

```sh
rtk proxy cargo build -p soe-godot-host --release
rtk proxy ../tools/godot/Godot_v4.7.2-stable_linux.x86_64 --headless --path godot --import
rtk proxy ../tools/godot/Godot_v4.7.2-stable_linux.x86_64 --headless --path godot --script player_test.gd
rtk proxy ../tools/godot/Godot_v4.7.2-stable_linux.x86_64 --path godot --script walkthrough.gd
```

Set `SKYRIM_DATA` to the installed Data directory for the walkthrough and the
real-cell portion of the test. The test without data still executes PlayerNode,
its camera setup, mouse-look events, rebindings, chords, short taps and action
edges inside Godot. It passed on 4.4.1 and 4.7.2. With data on 4.7.2 it built
281 collision bodies, settled the player near `(-45.3, -16.3, 321.2)` and
moved about 90 Skyrim units north in 30 steps at 60 Hz. The exact-PC-map tests
also passed against the installed archive, alongside 126 host unit tests.

These are automated checks, not a manual play session. The launcher uses
WASD, mouse look, Space, Ctrl and F, with Escape to exit. For a rendered
smoke capture, set `SOE_CAPTURE_PATH` to a writable PNG path and use
`--max-fps 60`. It disables mouse capture and exits after saving frame 120.
An initial windowed smoke attempt ended with SIGTERM. A later run saved a
player-camera capture and exited successfully on the RTX 5070 Ti. The user
then tested movement interactively and reported rug flicker, missing beds,
broken kitchen flooring and slow startup. The capture proves textured
geometry is drawn, not visual correctness. These defects remain open pending
a comparison with the merged reference fixes. Non-US keyboard layouts,
side-specific modifiers remain unvalidated.
The local Godot Input documentation supplied the distinction between held
actions, just-pressed edges and injected input events used by the test.

In a restricted environment, give Godot writable `XDG_CONFIG_HOME`,
`XDG_DATA_HOME` and `XDG_CACHE_HOME` directories. A windowed run also needs
access to the desktop display. The Rust gate here used `CCACHE_DISABLE=1`
and a writable temporary `XDG_DATA_HOME` for the comment-lint database.
The full gate passed with 821 tests and all three clippy passes. Taplo and
markdownlint-cli2 were absent and skipped. Live PC-map and Godot runs above
were separate from the gate, whose game-data environment variables are unset.

Integration with master at `6d990e7` passed the combined gate with 852 test
passes. The release extension was rebuilt, and Godot 4.7.2 repeated the
installed-data player test successfully with 281 collision bodies and about
90 units of movement. This confirms the automated movement check survives
the reference/audit integration, not that the reported visual defects are fixed.

## Ordered workers and data-source split (2026-09-19)

The data source now uses one bounded scoped worker helper for master parsing,
archive opening and mesh construction. Results are joined in input order.
Archives still enter the VFS in load order, preserving last-archive precedence.
Every worker is joined even if another worker panics. A failed batch returns
`DataSourceError::WorkerPanicked`, rather than shortening the mesh result list
and potentially attaching later geometry to the wrong reference.

Texture keys use `soe_bsa::normalize`, matching the VFS's case and separator
rules. Per-key `OnceLock` entries share successful and failed loads across
workers, and missing-texture reports are sorted. Tests cover concurrent aliases,
negative caching, worker limits, panic handling and archive override order.
The small archive fixture is reduced from the bsa crate's test-private
`archive::testdata::sse_bsa`, because that helper is not exported to dependents.

Texture handling and exterior streaming now live in `data_source/textures.rs`
and `data_source/exterior.rs`. Existing public texture paths are re-exported.
The actor and cell renderers share their Godot world-scenario lookup through
`godot_world.rs`. No new Godot node exception was introduced.

The gated Bannered Mare test compares reference order, geometry arrays and
texture payloads at one, two and eight workers after warming the cache.
Per-stream counters are exact only for non-overlapping stream calls, since
concurrent callers share counters. Persistent workers and deferred archive
payload reads remain separate measurement tasks.

Exterior grid selection uses unsigned absolute differences, avoiding signed
overflow at extreme CELL coordinates. Negative radii still select no cells.
Boundary and negative-radius cases have a unit test.

## Reference corrections, 2026-09-19

`godotwind/src/core/nif/nif_material_info.gd::get_cache_key` is the comparison
for material identity. The host now preserves every field consumed by
`standard_material_from`, including different cutout thresholds. Uploads
remain shared by texture path. The engine-free material-key test inserts
opaque, cutout and blended variants with both culling modes in both orders.
Glow, environment and other shader features remain unwired.

`godot_physics` uses `PhysicsServer3D.body_add_shape`'s transform argument.
Native boxes, spheres, capsules and convex hulls replace the former box-hull
approximations. See [physics](physics.md) for geometry tests and measurement
limits, and [the reference map](reference-audit.md) for future work.

## Capacity checks

`godot/bench.gd` now fails when its frame p99 exceeds 16.7 ms or its worst
frame exceeds 33 ms. Environment overrides must be finite and positive.
Headless runs fail because they cannot establish rendering performance.
`godot/bench_metrics_test.gd` checks metric arithmetic headlessly.

`godot/stress.gd` isolates geometry and unique-texture capacity with server
RIDs and no per-instance nodes. `tools/performance.py` runs fresh-process
ladders, repeats and refines breaches, and retains measurement logs. A soak
repeatedly clears and rebuilds in one process. See
[performance-limits.md](performance-limits.md) for commands, budgets, actual
coverage, and the distinction between these renderer probes and gameplay.

## Furniture omitted from the walkthrough (2026-09-19)

The player walkthrough exposed missing beds. Both rendering and collision
accepted only STAT bases. The shared model selector now also reads FURN
MODL paths, excluding furniture marked as editor markers. Sources are
`engine-refs/uesp-modfileformat/FURN.wikitext` and xEdit's
`wbRecord(FURN)`/`wbGenericModel` in `Core/wbDefinitionsTES5.pas`.
The existing ESM string decoder is reused. No new dependency or parallel
record store was added.

The installed-data regression in `collision/furniture_tests.rs` failed
before the selector change with all 29 Bannered Mare furniture references
absent from render output. It now passes with zero omitted, including four
beds whose NIFs contain supported static collision. It also checks that the
render and collision paths select the same number of model-bearing references.
This is geometry/collision evidence, not proof of final pixels or activation.
The test prints the bed FormIDs, model paths and placements for reproduction.

`CellStats::furniture_with_model` and the host's `last_stats["furniture"]`
report furniture separately from statics. Synthetic tests cover both model
types, furniture editor markers, empty paths, malformed STAT data and
unsupported base types.

The full gate passes with 856 test passes, including Taplo 0.10.0 and
Markdownlint CLI2 0.23.2 installed through Homebrew. The rebuilt Godot 4.7.2
player test passes with 310 collision bodies, up from 281. Its movement
endpoint changes because furniture now participates in collision. A fixed
camera at Skyrim `(-80, 140, 410)`, aimed at `(-80, 240, 360)`, shows the
noble double bed in a rendered capture. Captures stay outside the repository.
The editor import loaded the extension but aborted during shutdown after
sandbox socket errors. Subsequent game-mode tests and captures exited normally.

Sitting, sleeping, activation, other non-STAT/FURN object types, runtime
enable-state integration, rug flicker and kitchen-floor defects remain open.
The startup follow-up below replaces the separate player and renderer loads.

## Placed floors, rug decals and startup (2026-09-19)

The kitchen-floor gaps were misplaced model geometry. Several Whiterun floor
NIFs carry a quarter-turn export transform on their root node. The host applied
that transform in addition to the REFR placement. OpenMW's
`components/nif/node.cpp::NiNode::read` documents discarding the block-zero root
transform to prevent incorrect orientation. The host now applies that rule to
single-root placed STAT/FURN models, excluding Bip01 roots. Both rendering and
collision use the same prepared NIF. Collision payloads stay unchanged. The
washbasin follow-up below additionally converts local scene rotation matrices.
The raw NIF reader and actor skinning path are untouched.

A gated test reads `WRIntFloorDirtStr01.nif`, checks its authored quarter turn,
then verifies that placement resets only the root. Fixed-camera captures from
Skyrim `(-1040, -384, 190)` toward `(-1320, -384, 55)` show the formerly misplaced
stone borders aligned with the room perimeter and the black floor gaps filled.
This does not establish every placed model's correctness or settle other NIF
matrix-convention questions.

Whiterun rug detail meshes carry the NIF decal flag and disable depth writes.
The host previously ignored these flags. Decals now use a mesh shader with a
slope-scaled reverse-Z offset, following OpenMW's
`components/nifosg/nifloader.cpp::handleDecal`. The shader preserves the host's
diffuse, normal, alpha and culling inputs and applies the decal's depth test and
write flags. Geometry is not raised or moved. Material and batch keys include
decal depth state, with tests keeping four depth variants separate from a plain
material sharing the same textures. The offset is a Godot adaptation, not a
claim of bit-identical Skyrim rasterization. Shader compilation and captures
were checked on Godot 4.7.2 Forward+.

Startup previously opened the same five masters and archives twice. A headless
baseline measured 2.827 seconds for renderer data, 4.168 seconds for player data,
111 ms for streaming and 22 ms for collision. These are single-run warm-machine
observations, not cold-cache benchmarks. `PlayerNode::use_streamer_data` now
shares an immutable `Arc<DataSource>` explicitly with the renderer, including its
VFS and texture cache. Standalone `load_data` remains available. No global cache
or parallel record database was introduced.

`CellStreamerNode::begin_load_data` opens data on a CPU worker and
`poll_load_data` collects completion on the main thread. The walkthrough shows
the current phase and elapsed time while loading. Mesh creation and GPU uploads
remain synchronous on the main thread, as the local Godot thread-safe API guide
recommends. Dropping the streamer joins an outstanding worker before extension
unload, so exiting during a load can wait for that CPU operation to finish.

`godot/startup_test.gd` checks failure, retry, rejection of overlapping requests,
frame progress during loading, PC bindings and source lifetime after the streamer
is freed. With installed data it passed while processing 263 frames during a
4.393-second load. The real-data host tests passed 146 tests, and the player test
still passed with 310 collision bodies. The full gate passed with 865 test passes
and every configured linter, including Taplo and Markdownlint. Installed-data and
windowed checks ran separately from that gate. The updated windowed launcher
reached ready in 3.325 seconds: 2.960 seconds opening data, 304 ms rendering and
27 ms setting up the player. The remaining time includes yielded frames and
scene setup. This is time from the script's loading phase, not process launch.

Captures and measurement logs remain outside the repository. White editor-marker
geometry and incomplete material features are still visible. Still captures
alone do not establish that rug flicker is absent during movement. The user
subsequently walked the room and confirmed that the rugs no longer flicker.
These checks
do not turn the movement prototype into a complete gameplay walkthrough.

## Floating candle supports and camera jumps (2026-09-19)

Several candles were drawn without their supporting furniture. Nightstands,
drawers and wardrobes use CONT bases, not FURN. The shared model selector now
accepts CONT MODL paths, following `engine-refs/uesp-modfileformat/CONT.wikitext`.
Rendering and collision still consume the same winning records and prepared
meshes. Container geometry does not implement inventory access, opening
animations or loose-object simulation. Container counts are reported separately
in `CellStats::containers_with_model` and `last_stats["containers"]`.

The installed test requires every model-bearing container in the Bannered Mare
to appear in the surface output and checks static collision on its nightstands,
drawers, wardrobes and end tables. No candle positions or game assets are edited.
The inspected horn chandeliers already include their chain and rope surfaces.
The user's subsequent screenshots identified the remaining floating objects
as washbasin stands, covered below, not those chandeliers. The user confirmed
that the other floating objects now have their supporting chests and closets.

The controller's full-height downward snap could pass through a thin floor.
The bounded descent described in `docs/physics.md` removes the large jumps on
the upstairs and kitchen probe routes. Before the change, their largest
single-tick vertical changes were 28.824 and 21.675 units. Afterward the same
300-tick routes at 60 Hz reached 5.289 and 4.813 units. Regression limits are
eight units for these routes. The main-room route permits a full 36-unit step
because it crosses furniture edges, and measured a 19.786-unit descent before
the local-rotation correction below and 30.741 units afterward.
These routes do not prove every possible path through the room is smooth.

`PlayerNode` previously updated its RenderingServer camera only on physics
ticks. It now retains two feet/height poses and interpolates them each render
frame, following the local Godot guide
`tutorials/physics/interpolation/advanced_physics_interpolation.rst`. Mouse
rotation is consumed without interpolation. Cell entry resets pose history,
and manually driven `advance` calls retain immediate camera updates. Third
person still checks its boom against collision from the displayed pose.
Unit tests cover intermediate position, crouch height, fraction clamping and
history reset. This is interpolation, not an added stair-height easing system.

### Inverted washbasin stands

The two screenshots identify `Furniture/Noble/NobleWashBasin01.nif`, references
`0:0C903D` and `0:0CA022`. Its mesh children have quarter-turn rotations.
The raw NIF reader expands Matrix33 according to nif.xml's column naming, but
the placed-model path applied those matrices without the transpose used by
`nifly`. Unlike identity or half-turn matrices, these rotations expose the
error: the stand was inverted and its lowest vertex floated 34.279 units above
the reference origin.

Sources are `engine-refs/nifly/src/Objects.cpp::NiAVObject::Sync`, the direct
byte synchronization in `include/BasicTypes.hpp::NiStreamReversible::Sync`,
and row-dot-vector multiplication in `include/Object3d.hpp::Matrix3`.
`placed_model` now transposes each local NiNode, NiTriShape and BSTriShape
rotation before scene composition. Translation, scale, vertex data, collision
payloads and REFR placement are unchanged. Rendering and collision consume the
same adapted model. Keeping this at the placement boundary avoids changing the
raw NIF API or applying a second conversion to the existing actor-bind path.

`washbasin_feet_reach_the_authored_origin` fails before the correction at
34.279 units and passes afterward with a one-unit floor-contact tolerance.
The installed furniture, floor, container-support and walking-route tests
also pass. Fixed-camera captures of both references on Godot 4.7.2 show the
stands upright with their feet on the floor. The player runtime test passes
with 361 collision bodies. The user then walked the rebuilt room, reported
that everything seemed correct and closed the window. This is acceptance of
the reported movement-room defects, not proof that every asset or gameplay
system is complete. The tracked launch session confirmed exit code 0.

### Walkthrough process ownership

Keep the process session ID returned by the launch tool and poll that exact
session for exit. Do not substitute a process-name search, since it can inspect
a different namespace or find another user's run. Rebuild the extension only
after the tracked process has exited. `walkthrough.gd::_finalize` prints
`WALKTHROUGH stopped` on normal shutdown, including exit during loading.
The process exit code remains the authority if a crash prevents that callback.
The local Godot `MainLoop._finalize` documentation defines the shutdown hook.

## Host integration audit follow-up

The audit found that opaque decal variants could remain in the opaque render
pass and be overwritten by the surface underneath. Every decal variant now
writes alpha, keeping cutout discard and authored blend alpha, so Godot draws
it after opaque geometry. This follows the local Godot spatial-shader and
Material documentation. Transparent materials do not contribute to shadow
casting or screen-depth textures, including decals retaining depth writes.

The shader generator can export its exact production variants for a rendered
regression. Set `SOE_SHADER_FIXTURES` to a temporary directory, run
`cargo test -p soe-godot-host --lib export_decal_shaders`, then run Godot with
`--path godot --script decal_test.gd` and the same variable. A real renderer is
required. The test checks twelve alpha/depth variants in both creation orders
over an opaque floor, with a foreground occluder and cutout holes. Godot 4.7.2
Forward+ passes on the RTX 5070 Ti. The previous shader text fails the opaque
depth-writing case with a blue floor pixel where the decal should be visible.

Synthetic NIF tests now exercise child rotation/translation composition and
the Bip01 and multiple-root exceptions using the NIF crate's fixture writer.
The startup runtime test also frees a streamer before collecting its worker,
exercising join-on-drop. These additions came from an independent read-only
audit. The audit found no further demonstrated correctness blocker.

Validation of the combined host snapshot: the full gate passes with 879 Rust
tests and 11 Python runner tests, with no missing lint tools. The serial
installed-data host library run passes all 155 tests. Godot 4.7.2 startup
passes with 396 responsive frames during a 2.733-second data load, then checks
source lifetime and worker shutdown. Player checks pass on 4.4.1 without game
data and on 4.7.2 with 361 collision bodies. The Jolt check on 4.7.2 builds
361 bodies and 365 shapes with zero conversion failures and settles at
y 8.295 after 87 frames. These are separate from the gate's skipped data paths.

An initial parallel installed-data run ended with SIGTERM. The serial rerun
completed. A startup run retaining the first world while loading a second
also ended with SIGTERM. The shutdown test now frees the first world before
starting its second load. An initial import/runtime attempt could not find the
shared target directory's extension. The test setup now points to the rebuilt
library, and startup exits with failure if the extension classes are missing.

## Door picking groundwork (2026-09-19)

The `host-gameplay` work adds DOOR models to the shared rendering and collision
selector. It reads the generic MODL path and excludes bit 23 editor markers,
following `engine-refs/TES5Edit/Core/wbDefinitionsTES5.pas`, `wbRecord(DOOR)`.
The existing NIF collision reader and placement transform are reused.

`DataSource::placed_cell_colliders` retains the owning reference for each
shape. `CellCollision::references` maps physics body IDs back to resolved
FormIDs, including references with several bodies. `PlayerNode` retains that
map on cell entry and reports the reference from an activation ray through
`last_activate_reference`. Its old hit flag remains available. Both results
reset on cell entry and on steps without an activation press. No new nodes,
record copies or dependencies are introduced.

The installed Bannered Mare regression passed with all six doors producing
visible surfaces, mapped collision bodies and ray hits resolving to their
own reference IDs. It also retained the checks for 29 furniture references
and four beds. This is a Rust data/collision check, not a rendered Godot run.

The next host changes share an `Arc<LoadOrder>` with `Game`, including its
cached cell index, and load scripts through the same VFS. The host gains a
dependency on the existing `soe-game` crate, with no new external packages.
`start_gameplay` starts scripts and quests, then applies the selected
walkthrough spawn. This is a play-from-here path, not the new-game intro.
Each player step writes the
controller pose into the game, dispatches activation, ticks the VM, then
applies script moves and door landings back to the host. The movement-only
tests can still run without starting the game.

When gameplay is running, the controller reads paralysis from the shared
World before applying frame input. A live Paralysis magic effect replaces
horizontal direction and speed with zero and blocks jumping, while physics
continues to step for gravity. Once the effect expires, the next controller
step applies movement input again. Tests
`mag_42_paralysis_blocks_player_locomotion_and_jump` and
`mag_42_player_locomotion_returns_when_not_paralyzed` cover both states.

### Furniture seating, HST-09

Player activation of a FURN reference reserves an active Sit marker in the
shared game state and moves the actor to the furniture reference transform.
The player releases the reservation after moving more than 24 Skyrim units
from that transform or changing cells. Activating the same furniture again
also releases the reservation. The marker selection and saved actor state are
described in [game.md](game.md#furniture-activation-hst-09).

### Player sleep, HST-11

Activating a FURN reference with an available active Sleep marker completes
an eight-hour player sleep through `Game::sleep_player_at_furniture`. The
shared game path emits the sleep events, advances the calendar, and releases
the marker on wake. The sleep marker requirements and interruption result are
described in [game.md](game.md#player-sleep-hst-11). The host activation path
currently always completes normally. The game API accepts an interruption
result for interrupted completion callers.

The ordinary load-door path uses deferred activation, including
`arm_doorway`, `commit_doorway` and later `settle_unload`. Destination geometry
and collision are prepared together before replacing host resources. Exterior
destinations resolve their worldspace from the logical cell and select a
3 by 3 grid around the landing, adding worldspace persistent references.
Inherited LAND follows the existing parent-land rule and now contributes
collision with the same positions as rendering. Cell identity remains intact.

`godot/gameplay_test.gd` drives an unlocked Bannered Mare door outward and
back through the game runtime, requiring a changed/restored cell identity,
draw instances and supporting collision at each landing. This harness calls
the same activation helper as the input ray, with explicit reference IDs.
After the round trip it turns toward the door and injects the E key through
the device input path, requiring the picked FormID and another exterior
crossing. Its result is separate from the Rust gate.

Godot 4.4.1 and 4.7.2 headless runs passed with the installed five-master data:
Bannered Mare `0:01605E` to exterior `0:01A270` and back, then E picked
`0:016072` and crossed again. The exterior had 545 collision bodies and the
returned interior had 373. Both landing checks reached supported ground.
Startup loaded 14,026 scripts, attached 28,143 instances and started 423
quests, reporting nine missing script attachments. These are not resolved
by the host integration. No rendered-image or speaker-output claim follows
from the headless test.

## Lockpick activation requests (HST-03)

When player activation reaches a locked, non-key-only reference without a
matching key, `PlayerNode` retains its FormID as the pending lockpick target if
the player carries a `Lockpick`. `pending_lockpick` exposes the target and
authored lock level to an interaction UI. `resolve_pending_lockpick` applies
the UI result through `Game::resolve_lockpick`. Success then resumes ordinary
deferred activation, while failure consumes a pick and leaves the reference
locked. Keyed activation remains on the regular game path.

The UI-08 draft is absent from this worktree, so no lockpick minigame is
presented and the player cannot resolve the pending request through the
current host controls. The public host methods provide the handoff point. No
Godot interaction test exercises that handoff yet.

The integration gate passed with 915 Rust tests and 11 Python runner tests.
All three clippy passes and every lint tool ran. The cell-index API change
also removed unused mutable bindings in the benchmark and navmesh test,
without changing their behavior.

An initial test accepted the cell selected by startup scripts and therefore
tested a different location. The corrected test asserts the requested
Bannered Mare identity after startup. The first import aborted when sandbox
restrictions denied an editor socket. Direct runtime execution succeeded.

### Animated static collision, HST-01

`AnimatedStaticNode` can build a trimesh collider below each generated mesh
when `collision_enabled` is set before `load_nif`. The controller updates the
mesh transform, and the collider remains its descendant, so Godot applies the
same scene transform to both. `open()` resets controller time and starts its
per-frame evaluation. The collision option defaults off. This path is for
small animated references and does not change bulk cell batching.

The `hst_01_door_swing_test.gd` Godot check loads a synthetic NIF controller
with a rotation track, advances it, and confirms the generated static body
follows the visual mesh transform. It does not connect ordinary door
activation in `PlayerNode` to an animated door instance. Plain gameplay doors
still keep their authored mesh pose and static collision when game state marks
them open.

Locked player picks use the shared game's key check. Without the key they
remain closed, pending lockpicking UI. Resource preparation is synchronous.
Exterior walking loads and unloads nearby grid cells through the budgeted
planner when either mesh budget is set. Ordinary door activation still does
not instantiate `AnimatedStaticNode`, so a logical open state does not swing
the rendered door or move its collision. Portal transitions and revised AI
remain deferred until existing Skyrim gameplay works.

### Cold door fade fallback (2026-09-20)

Door texture preparation now has a 50 ms budget fixed before measurement.
The source for that boundary is OpenMW's 50 ms delayed loading screen in
`apps/openmw/mwgui/loadingscreen.cpp`, read beside this repository's earlier
44.7 ms Bannered Mare cold texture trace. The full fixed-corpus distribution
is recorded in [performance.md](performance.md#cold-interior-texture-preparation-2026-09-20).

`DoorFade` uses a strict comparison. Preparation equal to 50 ms does not
activate it, while one nanosecond more does. A trigger shows one black
`ColorRect` under a fixed `CanvasLayer`, then reaches clear within 150 ms.
Those are two player-owned control nodes, with no node per world reference.
The overlay ignores mouse input. GDScript diagnostics expose the last texture
preparation time, active state and activation count.

The policy sums the existing `CellStats::texture_decode_time` across the
destination cells. That is cumulative preparation work, so parallel cell work
can make it larger than elapsed wall time. A warm destination reports no
texture misses and does not fade. The installed release survey used one worker
and found one 48.417 ms non-trigger and eleven triggers from 68.865 to 241.598
ms, so the corpus exercises both decisions.
Unit tests cover one nanosecond below, exactly at and one nanosecond above the
budget, plus the 150 ms lifetime. The gameplay harness checks the policy and
actual overlay visibility after every exterior and interior crossing.
The installed headless Godot run observed a 367.029 ms cold outbound
preparation and an active overlay. Its warm return and input crossings each
reported 0 ms and kept the overlay hidden. All three crossings passed the
policy check, and both triggered-lifetime checks were inactive after 120
updates at 60 Hz. The monitored process exited zero after 46.0 seconds and
peaked at 15,273,864 KiB RSS.

Preparation remains synchronous. The last old-cell frame stays presented
during that work. The overlay starts only after preparation, so it cannot hide
or bound the main-thread stall. An over-budget destination enters behind black
and fades in within 150 ms. Nearby-door prefetch remains HST-07.

remain closed, pending lockpicking UI. Resource
preparation is synchronous, with no fade or measured frame-time bound yet.
Runtime reference enable, placement and animation changes
are not yet reflected in all drawn objects. Portal transitions and revised
AI remain deferred until existing Skyrim gameplay works.

### Walking exterior grid boundaries

`PlayerNode` now feeds its controller position into `CellStreamer` after each
physics step. The walkthrough loads a radius-one square, retains cells through
radius two, and applies at most two cell loads and four cell unloads per tick.
The planner is cloned while new cells are prepared, and committed after their
resources are installed. Preparation errors leave the previous planner intact.
Triangle and byte budgets belong to HST-08.

Rendering instances and static physics bodies retain their owning CELL. A delta
adds the requested cells and frees only departed cells, preserving the handles of
retained resources and the player's character. LAND collision uses the same helper
as door landings, including inherited parent-world terrain. Worldspace persistent
cells stay resident independently of the grid. Game attachment events follow the
actual resource delta, while moving the logical focus alone sends no unload event.
Load doors reset the planner and replace the location through the existing path.
This adds no per-reference nodes.

Sources are local `openmw/apps/openmw/mwworld/scene.cpp`, `Scene::changeCellGrid`,
for retaining active cells while changing the grid, and CK wiki `OnCellAttach`
and `OnCellDetach` for script event audiences. The existing host planner supplies
ordering and hysteresis. Existing ESM CELL indexing and LAND decoding supply the
record semantics, so no new format reader is needed.

`examples/exterior_fixture.rs` generates five small master files containing a flat
LAND grid through the existing ESM writer and height encoder. Run it with a new
output directory, then set `SOE_EXTERIOR_FIXTURE` to that directory when running
`godot/exterior_stream_test.gd`. Its Godot 4.4.1 headless run passed: initial residency
was nine grid cells plus one persistent cell, the first walking crossing added three
cells, and the second added three while unloading the column beyond hysteresis.
Retained render and physics handles matched, and the player remained grounded.
`exterior_resources` and `cell_instances` expose those handles for diagnostics.
This proves resource lifetime and collision behavior, not rendered appearance or
frame-time performance. The installed-data `gameplay_test.gd` regression also passed
in Godot 4.4.1: Bannered Mare to Whiterun and back, grounded at both landings, then
another crossing through E-key activation. Its process was pinned to one CPU to
limit parallel preparation. It finished in 64 seconds and peaked near 16 GiB RSS.
Unrestricted runs received SIGTERM while loading, before reaching the assertions.
The full lint gate passed with 1,169 Rust and 11 Python tests, with all optional
lint tools present. Installed-data checks are separate from that gate.

## Music playback (AUD-05)

`PlayerNode.select_music_type(file, object, draw)` resolves the supplied MUSC
through the shared `Game`, evaluates MUST conditions against the player, and
passes the selected leaf to its SoundBank. A silent MUST fades the current
music out. Other tracks decode through the active VFS into stereo
`AudioStreamWav` resources and use two non-positional `AudioStreamPlayer`
children. A transition fades the old MUST using its authored fade-out and
brings in the selected track using the MUSC fade. Abrupt transitions skip both
ramps. Starting another transition stops the earlier outgoing slot, so it
cannot continue under the latest selection.

Loose `.xwm` music and xWMA files in FUZ require the optional
`xwma-ffmpeg` feature. The host does not apply the MUSC ducking amount, queue
policy, or track cycle/order flags. It uses those records' eligible ordered
entries to select one track per call. Mixer playback is implemented, while
physical speaker output remains unverified.

## Footstep audio integration (2026-09-19)

Starting PlayerNode gameplay now creates a SoundBank sharing the same immutable
DataSource and VFS. Grounded controller travel selects a movement group and
alternates foot tags, then a supporting ray supplies its NIF material. The game
record resolver supplies the impact and descriptor. The player uses the second
impact sound slot when present, based on the shipped player/NPC descriptor
pairs, with sound 1 as fallback. Unknown source material uses DOBJ's DLMT only
when collision carries no material ID. Unknown non-null IDs are not guessed.

Cadence is a distance fallback pending animation-event integration, with strides
of 55, 70, 90 and 130 game units for sneak, walk, run and sprint. Those values
are engine policy, not measured vanilla animation timing. Standing still,
airborne movement and large discontinuities do not emit steps. Cell changes
and script-driven placement reset cadence. Tests cover those exclusions and
equivalent travel split into 30, 60 and 120 updates.

SoundBank applies descriptor file choice, gain and pitch. Its existing sixteen
AudioStreamPlayer3D slots now share a FIFO cache capped at 64 MiB of decoded
mono 16-bit clip data. Active stream references and resource overhead are
outside that cache budget. The cache is cleared on source replacement. No new
decoder dependency was added.

One Camera3D replaces the player's bare camera RID because Godot's 3D mixer
enumerates Camera3D nodes before consulting listeners. An AudioListener3D alone
left player movement silent. This behavior is visible in Godot 4.4's
[`AudioStreamPlayer3D::_update_panning`](https://github.com/godotengine/godot/blob/4.4/scene/3d/audio_stream_player_3d.cpp).
The camera serves rendering and listening. Bulk geometry and physics still use
server resources. The player gains eighteen child/descendant control and audio
nodes: one camera, one SoundBank and its sixteen players. This is a fixed player
cost, with no new node per placed world reference. Its frame cost is unmeasured.

Output handling remains partial. Non-positional player sounds disable panning
and distance attenuation. Positional sounds use Godot's inverse-distance model
with record distance bounds. Authored five-point curves, speaker matrices,
reverb sends, descriptor priority, loop/envelope modes and resident alternate
routing are not fully executed. Conditional descriptors are refused. Footstep
sets now come from the game actor's current race, skin and equipped armor,
with selection limits described in [game.md](game.md). Animation timing,
landing/scuff sounds and NPC footstep delivery remain pending. The current
controller does not supply submerged state to audio.

## Dialogue voice playback

When a selected INFO response starts, the dialogue menu resolves its voice
asset from the response and current speaker. An explicit TRDT sound override
does not use the generated voice path. Otherwise the path uses the response
INFO's owning plugin filename, the DIAL and QUST editor IDs, the response
INFO's local FormID, its response number and the speaker's VTYP editor ID.
For shared response data, DIAL and QUST come from the response INFO's owning
topic.
When their combined length exceeds 25 characters, the quest ID is capped at
10 characters if it is longer than 10, and the topic ID is capped at 15.
For a shorter quest ID, the topic ID may use the remaining characters up to
the combined limit. This follows Mutagen's `GetQuestAndTopicStrings` rule.
NPC speakers take VTCK from their Traits template source. Talking activators
take VNAM. The current speaker reference position is converted from Skyrim
units to Godot metres and passed to `AudioStreamPlayer3D`. The filename rule
follows `../engine-refs/Mutagen/Mutagen.Bethesda.Skyrim/Records/Assets/VoiceType/VoiceTypeAssetLookup.cs`.
The runtime already has the selected speaker, so it resolves that speaker's
voice type rather than enumerating the static candidate set used by the asset
lookup.

The menu remembers the voice slot returned by `SoundBank`. Starting a later
response stops the prior dialogue slot first. Closing the dialogue menu and
advancing past the final response also stop the tracked voice. SoundBank
refuses to stop a slot that has since been released or reused for an effect.
Vanilla FUZ playback requires the opt-in `xwma-ffmpeg` feature and an installed
`ffmpeg` binary. The default host build reports `Unsupported::Wma` for these
assets. The dialogue tests use a mock output and do not verify Godot playback.

The selected FUZ's LIP block is decoded with the voice buffer. SoundBank
samples the native mouth targets against that player's playback position,
spreading the stored frames over the decoded voice duration. The
`dialogue_mouth_weights` method returns the current weights in LIP target
order. Stopping or finishing the voice clears the track and returns its
weights to zero. Lines without a decodable LIP block keep the mouth weights
empty or neutral.

`godot/sound_descriptor_test.gd` verifies real descriptor playback and cache
reuse through AudioEffectCapture. Its Godot 4.7.2 run captured 20,480 frames with
peak 0.01579 and exactly one decode across two plays. This is mixer output,
not proof of speaker output. `godot/footsteps_test.gd` exercises actual player
input and movement, with stationary silence and nonzero mixer output required.

The movement test passed on Godot 4.4.1 and 4.7.2. Both produced four steps
with zero errors across the same Bannered Mare route. Captured peaks were
0.03193 and 0.03201. Existing input/camera checks also passed on both versions,
and the 4.7.2 load-door round trip and E-key crossing passed after the camera
change. These runs were headless and do not verify physical speaker output.
The full gate passed with 934 Rust tests and 11 Python contract tests. All
three clippy configurations and all lint tools ran. Comment-lint used its CPU
fallback. The installed sound survey and Godot tests ran separately from the
gate's tests that skip when external-data variables are absent.

After actor-specific selection and implicit-player construction were connected,
the 4.7.2 movement test again passed: four steps, zero errors, 110,592 captured
frames and peak 0.07653. This run exercises the player's record-backed skin
selection. Equipment changes have game-layer tests, not a host inventory UI.
The 4.4.1 run also produced four steps, zero errors and peak 0.07653. The 4.7.2
door round trip and E-key activation passed with the corrected player identity.
The subsequent full gate passed with 942 Rust tests and 11 Python tests, all
lint tools run. An initial gate failed when a BSA test's 64 MiB temporary file
exceeded the `/tmp` quota. The passing rerun used a project-filesystem TMPDIR.

After authored locks and inventory keys were connected, the 4.7.2 door round
trip and actual E-key crossing passed again. Key use and lock state are tested
in the game layer, including a keyed load-door transition and unload retention.
The final gate for that increment passed with 950 Rust tests and 11 Python
tests. All three clippy configurations and all lint tools ran.

## Static contact queries, CMB-30

`PlayerNode` prepares server static bodies from the same `CellCollision`
colliders used by its Rust controller. `enter_cell` installs the initial set.
A successful logical location replacement installs the destination set,
including LAND triangles, and releases the previous body and shape RIDs.
Failed rendering installation releases the newly prepared query resources.
Leaving the tree releases the current set. No per-reference node is added.
The collider vector is a transient transfer input, then drops after both
physics paths have consumed it. It is not another mutable gameplay store.

Sources are the local Godot `PhysicsServer3D` documentation for `body_set_space`,
`body_add_shape`, `space_get_direct_state` and `free_rid`, plus the existing
`godot_physics::add_statics` converter and `DataSource::load_location` path.
The converter is reused without a second format reader or physics library.
Actor collision and animated weapon segments remain the separate CMB-28 row.

`godot/combat_collision_test.gd` checks installed-data startup, an exterior door
and return, first-obstruction queries with a probe behind a world surface,
replacement of the old query geometry, and removal at node exit. It requires
`SKYRIM_DATA` and the compiled extension, and runs headlessly through Godot.
The combined Rust lint gate passed with 1,205 reported Rust results and 11
Python tests. The full installed-data run passed on Godot 4.7.2 with Jolt
on 2026-09-20 after FMT-27 changed archive storage to on-demand reads.
The interior installed 373 bodies and 377 shapes, the exterior installed
545 bodies and 568 shapes, and the return restored the original counts.
Old geometry no longer obstructed queries after replacement or node exit.
The process exited successfully with peak resident memory of 3,499,988 KiB.
An earlier run with eager archive retention terminated before collision
startup after reaching 13,654,212 KiB across its process tree. Those memory
measurements use different scopes and are recorded separately.

## Authored controller geometry surveys

CMB-29 supplies decoded skeleton controller phantoms through
`soe_physics::shape::Collected::phantoms`. Their geometry is in game units,
with material and filter fields retained for actor-body selection. Static
location collision consumes only `colliders`, so phantoms cannot accidentally
become immovable level geometry. `docs/physics.md` records the source evidence
and installed survey. The host examples `controller_geometry_survey` and
`character_metadata_survey` inspect the installed NIF and HKX sources without
copying assets. CMB-28 installs and synchronizes actor bodies, and CMB-33
supplies generated controller geometry when authored geometry is absent.

## Ranged release latch, CMB-08

`combat::RangedRelease` stores the actor and bow or crossbow selected when a
shot is armed. It calls `Game::release_projectile` once on `bowRelease` and
clears the pending shot on `attackStop` or `bowCancel`. The returned request
contains the launch vector and winning AMMO/PROJ inputs. This latch does not
move projectiles or resolve impacts, which remain CMB-22 work.

The synthetic host tests cover one returned request, duplicate release markers
and cancellation with the ammunition count unchanged. No Godot node or
per-reference resource is created for this release state.

## Projectile flight, CMB-22

`combat::ProjectileFlight` advances the release request with fixed-step
gravity, checks each segment through `PhysicsDirectSpaceState3D.intersect_ray`,
and stops on its first actor or obstruction. Actor bodies resolve through the
existing World-owned RID map, and the firing actor's bodies are excluded. The
flight ends at PROJ range if it has no contact. Contact is latched once, so
later updates cannot report another target or obstruction. Flight state is
transient and adds no node or per-reference resource.

Synthetic tests exercise configured gravity, repeatable fixed steps, range
termination and one-time actor and obstruction contacts. The query uses a
segment ray, so PROJ collision radius and actor impact damage are not applied
by this increment.

## Recoverable missed arrows, CMB-23

`ProjectileFlight::recover_missed_arrow` consumes one recorded world
obstruction impact and asks the shared `Game` to create a dynamic AMMO
reference at that point. A second call cannot create another reference.
Recoverable state comes from an arrow AMMO record whose header and DATA flags
allow play. Actor contacts and range expiry do not leave an arrow reference.
The host flight itself remains transient. SOEW persists the placed reference
through the existing reference table. No scene node or extra save payload is
added for the arrow.

## Actor contact bodies and pose segments, CMB-28

`godot_physics::ActorBodies` gives every resident actor one static query body
on the PhysicsServer3D space, with no scene node per reference. Static, not
kinematic: Godot's Jolt backend (`modules/jolt_physics/objects/jolt_body_3d.cpp`,
`set_transform`) only stores a kinematic body's new transform and applies it
with `MoveKinematic` on the next step, so a HitFrame query in the same tick
missed a moved actor and a body created that tick sat at the origin. The
headless check below failed that way before the change. PlayerNode keeps
one store and rebuilds it from `World::contact_actors` after each auto-stepped
physics tick, and again before an armed HitFrame queries. The pure
`combat::plan_body_sync` decides the work: an actor already holding a body at
the same scale only moves, a scale change rebuilds its capsule, and an actor no
longer listed loses its body. A cell change, a disabled reference or a loaded
save therefore empties or refills the store on the next tick without a
separate teardown path. `exit_tree` frees every RID. A new physics space frees
the old bodies before building new ones.

Each body carries the capsule from `soe_physics::generated_controller_shape`
at the actor's controller scale, flattened with the same primitive conversion
the static location bodies use. The body transform is the reference position
converted to Godot metres and axes. Rotation is not applied, because the
capsule is upright.

The contact query now reads two owner maps: bodies registered through
`register_melee_body` and the synchronized actor bodies. Every RID of the
attacker in either map is excluded. `intersect_ray` returns the nearest hit
along the segment, so the first actor or unmapped obstruction wins.

`aimed_spell_projectile_contact(spell_file, spell_object, contact_body)` maps
the contacted actor body RID through that same owner table and applies the
aimed spell through `World::cast_spell_on_projectile_contact`. Unknown bodies
and rejected casts return false. The caller discards the projectile after a
successful contact. This callback allocates no projectile node and stores no
projectile or save state.

`cast_location_spell(spell_file, spell_object, x, y, z)` sends the selected
Skyrim world position to `World::cast_spell_at_location`. The player's static
physics world raycasts from the player feet to the point. Missing physics data
or any geometry hit rejects the cast before the world applies effects. The
world checks the authored SPIT range and centers each effect's area at the
selected point.

Pose-derived segments come from two new entry points.
`melee_pose_contact(name, skeleton)` finds the armed hand's weapon node in a
Skeleton3D (`WEAPON` then `Weapon` then `NPC R Hand [RHnd]` for the right hand,
`SHIELD` then `Shield` then `NPC L Hand [LHnd]` for the left) and takes its
global pose. `melee_bone_contact(name, transform)` takes that world transform
directly. Both run the segment from the node origin along its negated basis Z
column, which is Skyrim local +Y after the axis map in `units.rs`. The length
is `World::melee_reach` converted to metres. `sync_contact_actors` and
`contact_actor_body` expose the store for diagnostics, `contact_actor_bodies`
lists every body with its owner and World position in Godot space,
`last_melee_target` names the actor the last HitFrame query reached, and
`equip_melee_weapon` gives and equips a weapon so a headless script can arm
the player.

`SkinnedActorNode::load_melee_character` loads the vanilla one-handed
`1hm_behavior.hkx` graph on the human rig. `send_graph_event` sends an input
to the loaded graph, and the node emits authored markers after installing each
animated pose. `set_graph_variable` sets a named graph value, including
`iRightHandType` for the equipped weapon type. `load_character` still loads
`mt_behavior.hkx` for locomotion.
`PlayerNode::melee_skinned_contact` accepts a marker from a rig bound to the
attacker. It reads the weapon attachment from the rig's model-space pose, the
same buffer used for skinning, and resolves the segment using the armed hand's
reach. For a bound rig, position, rotation and scale come from the actor's
current shared World placement. A rig without a matching weapon or hand bone
rejects the contact. CMB-04's `Right Attack/Block` press arms this contact for
the right hand, and the authored `HitFrame` remains the only event that queries
and applies damage.
`last_melee_obstruction` exposes the first unmapped body RID when an
obstruction stops a swing. It is invalid for an actor contact or a miss.

Sources read: `godot-docs` PhysicsServer3D and PhysicsDirectSpaceState3D for
body state, body modes and ray exclusion. `activeragdoll/src/utils.cpp`
`GetWeaponNode` for the `WEAPON` and `SHIELD` node names, and
`activeragdoll/include/math_utils.h` `ForwardVector` for the blade axis.
`CommonLibSSE` `MiddleHighProcessData::weaponBone` confirms the engine keeps
a weapon node per actor process. Its `NiMatrix3::SetEulerAnglesXYZ` supplies
the shared reference rotation used by both placed meshes and bound rigs.
The local Nemesis `1hm_behavior.xml` names
`attackStart`, `HitFrame` and `iRightHandType`. `docs/hkx.md` records the
linked graph and clip path used by the same interpreter.

Open: the capsule is the generated template for every race. The authored
`CHAR_CONTROLLER` phantom path in `DataSource::actor_controller_shape` needs a
race to skeleton path lookup the World does not expose yet. Actor bodies
follow `RefState::position`, so an NPC is only as current as the system that
writes its placement. The fallback to the hand bone keeps the WEAPON node's
axis, which is only approximate for a hand.
Checks: `crates/game/tests/cmb_28_contact_actors.rs` covers the World list
and the reach formula across a cell change, and
`crates/godot-host/tests/cmb_28_body_sync.rs` covers the body sync plan and
the segment, the animated attachment transform and a SKYRIM_DATA gated
one-handed graph HitFrame. `godot/cmb_28_contact_test.gd` runs on installed data from the
Bannered Mare through its load door on Godot 4.7.2. Every body sits at its
World position, bodies carry across the door with unchanged RIDs, an armed
HitFrame reaches the NPC beside the pose, an intervening server body stops
the contact and reports its RID, and an authored `1HM_AttackRight` HitFrame
reaches an obstruction through its live animated rig in both cells. Teardown
removes every actor body from
its space. On that
data `Game::melee_hit` then refuses the contact with "unsupported combat
behavior: perk entry points", so hit resolution is still open and belongs to
the combat rows, not to this one. Actor bodies are static query bodies, not
kinematic: Jolt applies a kinematic transform only on the next physics step,
so a body placed and queried in the same tick sat at the origin.

## Death ragdoll rendering, CMB-19

`SkinnedActorNode` loads the human `hkaRagdollInstance` from the same skeleton
HKX as its animated rig. A bound actor reads the shared World's death flag on
each host tick. At death, the node freezes its animated pose, creates one
dynamic `PhysicsServer3D` body and collision shape for each authored ragdoll
body, and creates cone twist or hinge joints from the authored body-space
frames, rebasing one frame so both pivots meet at the last animated pose.
The bodies use the decoded mass, friction, restitution and shape geometry.
Capsule, box, sphere, convex hull, translated and transformed
shapes are converted to Godot metres and axes at the actor's reference scale.
World-endpoint constraints use a static anchor RID. A joint's connected bodies
do not collide with one another. Hinge angular limits and the symmetric spans
of cone twist joints are sent to the server.

The low resolution ragdoll skeleton prefixes human bone names with
`Ragdoll_`. `ragdoll::RagdollBoneMap` removes that prefix for matching and
assigns animated descendants to their closest ragdoll ancestor. The initial
body pose follows the final animated model pose. Each rendered skin matrix
then follows its assigned body's movement relative to that initial pose. The
bound mesh instances use the actor's World placement. `render_mode` and
`ragdoll_bodies` expose the current representation and body RIDs for host
diagnostics.

`PlayerNode` removes its static contact capsule for dead actors on the next
body sync. Its `kill_actor` and `resurrect_actor` host calls forward to the
shared World and sync that capsule. On resurrection, the visual node frees
every ragdoll body, shape and joint RID and resumes its animation interpreter.
`PlayerNode` recreates
the actor's contact capsule from the World on its next sync. These server
resources are transient and are rebuilt from the World death flag, so the
SOEW save format is unchanged. The automatic visual transition applies to
actors bound through `bind_actor_appearance` to a loaded `SkinnedActorNode`.

The local `activeragdoll` Havok structures and `soe-hkx` reader establish the
body order and body-space joint frames. The Godot class reference under
`engine-refs/godot-docs/classes/class_physicsserver3d.rst` supplies the body,
shape, joint and RID lifetime calls. The host uses a cone twist span for the
ragdoll joint's cone and twist limits. Godot's cone twist API does not express
the separate authored plane range, asymmetric limits or position motors.

## Stagger hit reactions, CMB-27

A bound `SkinnedActorNode` watches its actor's shared `stagger_end_time`.
`load_character` loads the vanilla master graph, which connects locomotion and
stagger behavior. When a new active deadline appears, the rig writes the graph's
`staggerMagnitude` input and sends `staggerStart` once. At expiry it sends
`staggerStop` once.
The graph selects and plays its authored hit reaction, then follows its own
transitions back to the locomotion state. This rendering step only reads World
state and advances the animation interpreter. It never resolves the hit or
changes health, so the combat event cannot apply damage twice.

`activeragdoll/src/main.cpp`'s `StaggerActor` and the local Nemesis master,
stagger and one-handed behavior graphs establish the variable and event names.
CMB-14 stores only the stagger deadline, not hit strength or source direction,
so the host uses magnitude `1.0`. The master graph does not expose
`staggerDirection` from its referenced stagger graph, so the host cannot select
a hit direction through the outer graph's variable table. The gated
`cmb_27_vanilla_hit_reaction_plays_and_returns_to_locomotion_without_damage`
test checks the installed master graph plays its authored stagger clip and
returns to the original active state. Exact hit direction, stagger tier and
same-frame repeated deadlines are not represented by the current World event
state.

The `cmb_19_` unit tests check bone assignment, frame conversion and the
death-to-resurrection mode transition. The installed-data gated
`cmb_19_vanilla_human_death_has_renderable_bodies_and_joints` test checks the
vanilla human's 18 bodies, 17 constraints and mapped animated bones.
`godot/cmb_19_ragdoll_test.gd` drives a World death and resurrection in Godot
and checks the body modes, physics space, contact capsule, render mode, Jolt
body movement and uploaded skin movement. It does not assess the resulting
motion visually.

## Native save generations

`PlayerNode::save_game_generation(directory, retained)` writes the shared
runtime through `Game::write_generation`, including the current controller
pose. `recover_game_generation` explicitly selects and loads the newest
verified native generation under the current scripts and load order. The
existing dedicated directory accepts an absolute path or `user://`, and
retention must be between 2 and 64. The [generation protocol](save-resilience.md#sav-63-verified-generations)
documents integrity checks, retention and recovery diagnostics.

## Scripted ragdoll and knockback requests, CMB-38

`SkinnedActorNode` follows the actor's explicit CMB-38 ragdoll override when
one is set, otherwise it follows the death or unconscious state. It drains queued impulses for
its bound actor and applies them to the first ragdoll body through the Godot
physics server. Impulses use the host's Skyrim-to-Godot axis map and metre
scale. A command drained before a ragdoll exists is discarded. The host path
uses Jolt. `SimpleWorld` has no ragdoll solver. The conversion is a host
approximation because the CK pages do not state the force unit expected by
the physics solver.

Results are dictionaries with `ok`, generation and path. Generation IDs are
strings to retain their full unsigned range. Rejected candidates carry an
integrity, definitions or read reason. Recovery adds `state_loaded`, clears
stale melee contact state and synchronizes the player pose/resources. If the
resource installation fails after native loading, it stops automatic stepping
and reports `state_loaded: true` with `ok: false`. This is a file recovery path,
not a claim of whole-host atomic loading or missing-mod recovery play.

The methods compile in the full feature gate. Synthetic Rust tests execute
the shared Game file APIs, corruption recovery and interrupted rotation.
Interactive Godot resource installation after recovery remains untested.

## Native menu framework

The HUD uses one CanvasLayer with three ProgressBars and a compass Label.
PlayerNode refreshes it each process frame from the shared Game and its camera
yaw. Health, magicka and stamina read their current and modified maximum actor
values, then clamp the fill fraction to 0 through 1. The compass tape centers
the nearest 45 degree cardinal or intercardinal direction under the heading.
These are Godot-native controls. SkyUI's `MeterWidget` and `MiniMapWidget`
informed the meter percentage and player-angle behavior, and CK
`GetActorValue` and `GetActorValueMax` pages informed the values. Scaleform
execution is not part of this HUD.

`ui_06_hud.rs` checks all three shared actor values, current and modified
maximums, a changed health value and the north-to-east compass update. Cargo
tests cover the Rust data path.
The controls compile with gdext, but their appearance in a running Godot
window has not been exercised.

UI-01 adds a modal Control tree to PlayerNode. One CanvasLayer, one
PanelContainer, two VBoxContainers, one Label and one Button form the fixed
shell. A Theme supplies shared text and panel styling. Opening sets keyboard
focus and releases the mouse. Escape and the Close button close the menu.
Closing frees presenter controls, clears held gameplay actions and restores
prior focus and mouse mode. Mouse restoration waits for window focus when
closing while unfocused. Repeated opens and closes do not add shell nodes.

### MCM text and toggle rows, UI-11

`mcm_menu::ConfigPage` stores text/value and toggle rows in authored order.
It returns SkyUI option IDs using the page number in the high byte and a slot
from 0 to 127 in the low byte. `set_enabled` changes the disabled flag while
preserving other option flags. Hidden rows are omitted from presentation, and
disabled rows remain visible without accepting selection. `render_rows`
builds native Godot labels, buttons and check buttons. A selected enabled row
sends one Papyrus `OnOptionSelect(int)` event with its option ID to the
configured script form.

The local SkyUI `SKI_ConfigBase.psc` header and source define the text/toggle
option types, disabled and hidden flags, page-strided option IDs, 128 positions
per page and selection event shape. Godot Controls replace its Scaleform
presentation under the project intent. UI-05 script registration and page
lifecycle work is not present in this worktree, so the option model and
presenter are callable host pieces rather than a complete unmodified MCM script
execution path. UI-11 tests exercise their protocol without Skyrim data.

### MCM slider and menu dialogs, UI-12

`ConfigPage` also stores slider rows with their current value, authored minimum,
maximum and interval, and menu rows with their choices and selected value.
Selecting a slider or menu row dispatches `OnOptionSliderOpen(int)` or
`OnOptionMenuOpen(int)`. The returned dialog parameters use the row's current
value as both slider start and default, and the current menu choice as both
menu start and default. Accepting a slider clamps to its authored range and
snaps to its interval before updating the row and dispatching
`OnOptionSliderAccept(int, float)`. Accepting a menu updates the displayed
choice and dispatches `OnOptionMenuAccept(int, int)` with its choice index.
Cancelling closes the dialog without changing the row or dispatching an accept
event. `SKI_ConfigBase.psc` defines no cancel event. The callback names and
arguments match that source, while `SKI_ConfigManager.psc` confirms the manager
routes slider and menu selection and acceptance to those callbacks. The
`ui_12` fixtures use synthetic scripts and do not require Skyrim data.

### MCM color and key-map options, UI-13

`ConfigPage` adds integer color and key-map rows. A key remap sends
`OnOptionKeyMapChange(option, keyCode, conflictControl, conflictName)` to the
configured script, including empty conflict strings when no conflict exists.
The callback does not change the row value. The config script can apply its
decision with `set_keymap_value`. Opening a color dialog sends
`OnOptionColorOpen(option)`, and accepting the active dialog sends
`OnOptionColorAccept(option, color)`. The config script applies an accepted
value with `set_color_value`. Cancel clears the active dialog without sending
an accept callback or changing the row. Disabled and hidden options reject
these callbacks. The callback names and argument order follow the local
SkyUI `SKI_ConfigBase.psc` header and source, while conflict details follow
`SKI_ConfigManager.psc`. Its Scaleform-bound presenter cannot serve the chosen
native Godot UI, so this host models the public callback contract. Synthetic
tests cover callback eligibility, conflict arguments at the API boundary,
acceptance and cancellation without Skyrim data.

The host keeps the scene tree running. PlayerNode routes its existing Game
through MenuState::tick and Game::tick_menu_mode. While a menu is open, host
physics, camera input and activation do not advance. The game clock, ordinary
Papyrus waits and game-time waits pause, while menu-time waits continue. After
closing, the next player step resumes the existing Game and synchronizes any
scripted player move through the normal host path.

Native presenters use PlayerNode::game and game_mut to read and mutate the
shared form-backed runtime. menu_content exposes a Control container owned by
the framework. menu_opened and menu_closed signals run after their state change
through gdext's reentrant base guard, allowing callbacks to inspect PlayerNode.
Dialogue and MCM presenters remain unimplemented and can use this shell with
Crafting and MCM are not implemented here. Their menus can use this shell and
their existing gameplay APIs.

The inventory presenter now uses this shell. It renders the SkyUI categories
All, Weapons, Apparel, Potions, Scrolls, Food, Ingredients, Books, Keys and
Misc, then lists positive totals from the selected holder in FormID order.
Food is ALCH ENIT's `Food Item` flag. Item controls toggle only weapons and
armor through World::equip_item and World::unequip_item, so slot conflicts and
worn extra data remain in the game layer. The public inventory_menu reader
takes a Game, holder and category, allowing the container menu to reuse its
form-backed rows without a second inventory model. Drop, use, charge, favorite
and instance-specific actions remain outside UI-02.

When activation opens a container, its menu presents the same category buttons
and lists the container's positive inventory totals. The rows come from its shared
`RefState::inventory`, including authored `CNTO` entries loaded from the xEdit
`CONT` record. Taking a row transfers one item through `Game::transfer_item`,
then refreshes the selected category from the shared game state. A rejected
transfer leaves the source and player inventories unchanged. xEdit's local
`TES5Edit/Core/wbDefinitionsTES5.pas` defines `CONT` with `COCT` and `CNTO`
contents. CK's local `GetContainerForms_-_ObjectReference.wikitext` describes
enumerating container forms, and `OnContainerChanged_-_ObjectReference.wikitext`
documents reference moves between holders. These host rows show inventory
totals, not a separate list of loose references.

The UI-15 gift presenter opens when Game has a pending `ShowGiftMenu` request.
It lists positive inventory totals after applying the optional form or keyword
filter. Selecting a row transfers one item through `Game::gift_menu_transfer`
and refreshes the rows. Closing the modal menu resolves the latent Papyrus call
with the accumulated nonnegative gift value. Both giving and taking use the
same source and target transaction path.

The `inv_04_` inventory presenter tests activate a synthetic container, check
its listed contents, verify both rows after a successful transfer, and verify
both inventories remain unchanged when recipient quantity overflow rejects a
transfer.

The native I shortcut opens inventory once gameplay has started. This shortcut,
like Escape for the modal shell, is not yet remapped through controlmap.txt.
Rows scroll, and category selection survives equipment changes. Closed or other
menus reject stale inventory actions. Scene-tree edits use the same reentrant
guard as the shell so focus callbacks can inspect PlayerNode safely.

The magic presenter (UI-03) reuses the same shell. It lists the holder's
known, player-castable spells (SPIT spell type 0, the "Spell" entry in
xEdit's SPEL enum) in FormID order, each row showing name, base magicka cost
and any hand it already occupies. Powers, Lesser Powers and Abilities are
read from the same known-spell set but never occupy a hand, so they are
filtered out, matching the CK wiki EquipSpell/UnequipSpell hand argument
(0 left, 1 right, 2 other/voice). Two always-visible hand buttons pick the
active hand before a spell click sends it there, rather than gating the list
behind a hand choice. Clicking a spell calls World::equip_hand_spell through
the new magic_menu reader, which mirrors inventory_menu's Game-plus-holder
shape. The active hand is presenter-only UI state and resets on reopen, but
both equip slots live in ActorState's `left_spell`/`right_spell` and survive
an engine save and load from `SAVE_VERSION` 32 onward (`docs/game.md`), so a
reload keeps the on-screen indicator matching what the actor last had
equipped.

The native M shortcut opens the magic menu the same way I opens inventory,
gated on an active Game. No shortcut is yet remapped through controlmap.txt.

Creation bench activation opens a native Control recipe list, including an
empty state when the bench has no eligible COBJ recipes. Rows show the recipe
name, output and output count. Rows blocked by missing ingredients are
disabled. The first available recipe receives keyboard/gamepad focus when the
list opens. Confirming an available row calls
`Game::craft`, which rechecks eligibility and commits ingredient and output
inventory events. The host identifies creation benches from their FURN WBDT
mode. This presenter uses the existing transaction path and adds no crafting
state to the host.

Sources read for CRF-06: Godot's `godot-docs/classes/class_control.rst` for
native Control focus and input behavior, and `skyui/src/CraftingMenu/` for the
crafting menu's recipe list and selected-recipe confirmation flow.

`Game.ShowTrainingMenu(actor)` requests this presenter through a transient
World request. The presenter shows the trainer's skill, current value, tier
cap, DLG-16 price and remaining per-level sessions. The purchase button is
disabled when the player lacks gold or has reached the skill or session cap.
Purchases call the shared World transaction and redraw the offer afterward.

The native J shortcut opens the quest journal through the same menu shell.
It reads loaded active quest state from the shared Game, omits QUST type 0,
and hides the title for miscellaneous type 6 quests. It lists only objectives
whose runtime state is displayed, with completed and failed markers. While
the journal is open, PlayerNode compares the current rows with its last
snapshot each process frame and rebuilds the presenter when shared quest
state changes. The menu does not maintain quest or objective state of its
own.

The QUST layout rules come from `uesp-modfileformat/QUST.wikitext` and
`TES5Edit/Core/wbDefinitionsTES5.pas`. The local CK `Quest_Script.wikitext`
lists the objective display, completion and failure operations. The presenter
reads the existing `World::QuestState`. It adds no save fields.

The objective map presenter resolves each QUST target through its runtime
quest alias. Reference aliases point to their placed reference, while
location aliases use the location's `MNAM` world marker. It returns the
resolved Skyrim position, authored compass lock flag and a visibility value
derived from the objective's displayed state and the target reference's
enabled and deleted state. Target CTDA conditions are evaluated against the
current Game and VM, and a target is omitted when its conditions are false or
cannot be evaluated. The presenter reads the shared world and adds no saved
state. QSTA layout rules come from `uesp-modfileformat/QUST.wikitext` and
`TES5Edit/Core/wbDefinitionsTES5.pas`.
No Godot map control currently consumes these markers, so their on-screen
display and spatial projection remain unverified.

The lockpicking presenter (UI-08) opens when player activation reaches a
locked, non-key-only reference and the player has a lockpick but no matching
key. Its angle slider and Turn lock button submit attempts to
`Game::attempt_lockpick`. A miss consumes a pick and leaves the menu available
for another angle. A successful turn closes the menu and activates the same
reference through the ordinary Game path. The menu is presenter state only.
CK Lock and GetLockLevel pages plus xEdit's XLOC definition establish lock
state, level bands and the key-only level 255 case. The angle rule is the
game crate policy documented in [game.md](game.md#authored-locks-and-keys-2026-09-19).
Synthetic `ui_08` game tests cover both outcomes. The Godot control interaction
has not been exercised in a running scene.

## Dialogue menu, DLG-10

`open_dialogue_menu(file, object)` begins a conversation between the selected
speaker and the player through `Game::begin_dialogue`, then renders the
currently offered topics as focusable Godot Buttons in the modal menu. Each
button carries its topic FormID back to `Game::choose_dialogue`. The selected
line text is shown with a Continue button, which acknowledges it through
`Game::advance_dialogue` and either shows another response or returns to the
offered topics. Closing the modal menu closes the game conversation. Choice
eligibility, stale selection checks, response text and fragment execution
remain in the game layer.

`dialogue_menu::entries` reads the current eligible choices from the shared
Game. It displays each authored prompt, falling back to the topic name and
then its FormID when the prompt is absent. The menu does not keep a second
copy of dialogue state. The local Godot `Control` class documentation informed
the native control and focus path. The checked out SkyUI source has no dialogue
menu implementation, so its other menu assets did not define this presenter's
choice behavior. `CommonLibSSE/include/RE/D/DialogueMenu.h` confirms the
original menu is a separate menu-mode UI surface, while this host uses Godot
Controls under the project's native UI intent.

Sources read for UI-03: `ck-wiki` for the EquipSpell/UnequipSpell hand
argument and the SPEL spell type enum, `TES5Edit/Core/wbDefinitionsTES5.pas`
for the SPIT subrecord layout, and `skyui/src/ItemMenus/magicmenu.fla` plus
`skyui/src/Common/` for the known/equipped split this presenter mirrors. Now
`godot-host/tests/ui_03_magic_menu_adversarial.rs` covers `magic_menu`
against an unbuilt actor and a deleted-but-undeleted SPEL record, both fixed
during UI-03 (`magic_menu::entries` now forces the actor to build before
reading its known spells, and `World::spell` rejects a deleted winning
override). The existing inventory_menu and menu shell tests were left
unchanged since neither exercises spell state.

inventory_runtime.gd clicks categories and item controls against the shared Game,
checks equipment indicators and preserves the selected filter. It also checks
missing-game rejection, focus callbacks and stale handlers. The
inventory_fixture example writes synthetic masters containing a sword and food
in the player inventory, with a static floor in WhiterunBanneredMare. On Unix it
links three installed archives into the output directory. Run it with a fresh
output directory and the installed Data path, then set SOE_UI_TEST_DATA to that
output when launching the headless script. No installed records are copied.
Sources read locally were Godot's Control, Theme, Node and SceneTree class
docs, gdext node ownership, secondary API registration and reentrant signal
tests, and SkyUI SKI_ConfigBase's page/open/close lifecycle. The SkyUI source
informs the future content boundary only. Its Scaleform UI and MCM execution
are not ported by this increment. The only dependency addition is the existing
workspace Papyrus crate under dev-dependencies for clock tests.

The menu_clock Rust test calls the same coordinator as PlayerNode and checks
Wait, WaitMenuMode and WaitGameTime across open and close. The menu_runtime.gd
headless scene-tree test checks native controls, theme, focus, signal callbacks,
input isolation, mouse restoration, node lifetime and shutdown. With
SOE_UI_TEST_DATA or its fallback SKYRIM_DATA set, it also starts the shared Game
in WhiterunBanneredMare and checks that the
same game clock and host position pause and resume. It runs from a disposable
Godot project pointing to this worktree's extension, so an existing interactive
window keeps its loaded library. A small synthetic master set with installed
mesh archives exercises this path without loading the full installed records.
That fixture verifies host integration, not a full installed-game session.

The native mod configuration presenter (UI-05) opens through
`PlayerNode.open_mcm_menu`. It lists registered config quests by their script
`ModName` and renders buttons for the selected script's `Pages` property.
Selecting a config or page calls the shared Game MCM lifecycle methods, and
closing the modal calls `mcm_close`. Page contents and SkyUI's option controls
are not presented here. The page lifecycle follows the local SkyUI
`SKI_ConfigBase.psc` and `SKI_ConfigManager.psc` sources.

## Image-space lifetime stack, MAG-25

`image_space::ImageSpaceStack` derives its ordered frame snapshot from the
shared World. PlayerNode synchronizes it after startup scripts and each game
tick, and clears it when replacing the data source. The diagnostic
`gameplay_image_space` method exposes modifier identity, strength and age.
No additional scene nodes or server resources are allocated for applications.

The headless host test applies, fades and removes modifiers through Game,
checks the production snapshot and replaces the World to verify cleanup.
The full gate compiles the PlayerNode integration with Godot enabled. No
interactive renderer run or performance measurement is claimed.

The lifetime sources and tested timing policies are documented in
[game.md](game.md#image-space-modifier-lifetimes-mag-25). Authored IMAD curves
remain in their winning records. MAG-71 owns their evaluation and visible
post-processing through the host stack, including renderer resource cleanup.

## Image-space renderer prerequisites, MAG-71

`ImageSpaceStack::render_layers` samples the active modifiers' authored IMAD
curves through the shared World and returns their channels beside each layer's
active strength. It does not attach an Environment, CameraAttributes or
Compositor to the camera. `godot/walkthrough.gd` constructs a fixed background,
ambient light and directional light. Those constants are not the cell or
weather's authored image space. MAG-72's shared base IMGS pipeline is absent
from this worktree, so the host does not apply the sampled values to rendered
frames. The rendered acceptance check remains open.

MAG-72 owns resolution and rendering of base IMGS HDR, cinematic, tint and
depth fields for CELL/XCIM and explicitly selected WTHR/IMSP inputs. Its
observable check includes rendered fixtures and cleanup when the selected
source changes. It does not own autonomous weather or climate simulation.
MAG-71 retains animated curves, active-stack composition, all authored blur
modes and modifier renderer cleanup. Both rows require rendered evidence.

Sources read under `engine-refs`:

- xEdit `Core/wbDefinitionsTES5.pas` and `Core/wbDefinitionsCommon.pas`, plus
  Mutagen `Records/Major Records/ImageSpaceAdapter.xml` and `.cs`, establish
  curve layouts and flags. CommonLibSSE-NG
  `include/RE/T/TESImageSpaceModifier.h` and `include/RE/I/ImageSpaceData.h`
  distinguish base HDR/color data from modifier-only effects.
- Community Shaders `package/Shaders/ISHDR.hlsl` consumes exposure history,
  bloom, cinematic, tint and fade inputs. `ISBlur.hlsl`, `ISDoubleVision.hlsl`
  and `ISRadialBlur.hlsl` show separate passes and their shader parameters.
  `src/Features/LinearLighting.cpp` reads the combined sunlight scale for
  directional lighting. These sources show the required inputs. They do not
  establish the complete mapping from each IMAD curve to shader constants.
  The current HDR shader also contains explicit changes to vanilla contrast
  and display mapping, so copying it would not prove vanilla parity.
- Godot `classes/class_environment.rst`, `class_cameraattributes.rst`,
  `class_cameraattributespractical.rst` and
  `tutorials/3d/environment_and_post_processing.rst` describe color
  adjustments, tone mapping, bloom, exposure and depth-of-field controls.
  These are available implementation mechanisms. Their names do not prove
  equivalent units, transfer functions or ordering to Skyrim.
- Godot `tutorials/rendering/compositor.rst` documents custom passes with
  RenderingDevice access in Forward+ and Mobile. Callbacks run on the render
  thread and require synchronized state and explicit resource cleanup.
  `tutorials/shaders/advanced_postprocessing.rst` describes depth access and
  reconstruction. No custom compositor was installed during this audit.

`mag_71_host_frame_keeps_curve_samples_with_their_active_weights` checks that
the host frame preserves the sample and the independent active weight. The
game test `mag_71_samples_authored_color_blur_depth_and_hdr_curves` checks
synthetic animated color, blur, depth and HDR keys.
`mag_71_installed_imad_curves_sample_winning_records` is gated on `SKYRIM_DATA`
and samples each winning IMAD at its active normalized age. These checks do not
create rendered image fixtures or verify GPU resource cleanup.

The repeatable installed survey and its exact counts are recorded in
[game.md](game.md#image-space-rendering-prerequisite-audit-mag-71). It verifies
record layout and dependency reach. No Godot process was launched, no frame
time was measured, and no visible image-space behavior is claimed. MAG-71's
full rendered acceptance check remains in BACKLOG.csv.

## Record-driven skin refresh (INV-08)

After loading a character rig, `PlayerNode.bind_actor_appearance(node, file, object)`
connects that existing `SkinnedActorNode` to the gameplay World and DataSource.
The node derives its visible addons each tick, including ticks while animation
is paused. A changed skin, race or equipment set reloads authored ARMA model
paths through the shared archive and loose-file overlay. Unchanged views avoid
mesh loading. Failed views are cached too, so unchanged bad assets do not reload
or log on each tick. A changed view or an explicit rebind retries preparation. Failed model preparation retains the previously displayed surfaces.
`ActorError::RigMismatch` reports missing rig bones before a live replacement.
For a bound actor, the node also reads `World::is_invisible` and toggles the
existing RenderingServer instances while the effect is active. It restores
visibility when the effect expires or is dispelled. This covers bound skinned
actor rigs, not actors drawn through another host path.

The node owns the replaceable surface arrays. Only the existing animation rig
has the loader's process-lifetime allocation, so repeated skin changes do not
leak new rigs. Geometry remains RenderingServer instances and skeleton RIDs on
the existing character node. This introduces no new per-reference node path.
The loaded rig and its animation graph remain in use. Selecting new race animation
rigs, face/head-part generation and first-person scene assembly are separate from
this third-person addon refresh.

Covered addon slots remove corresponding BSDismemberSkinInstance partition
triangles. The loader handles both SSE shape indices and partition-local indices
with vertex maps, validates bounds and preserves Godot's winding conversion.
Local `nifxml/nif.xml` defines Skyrim body parts 30 to 61 as equipment visibility
slots. The existing `nif::blocks` partition reader supplies the data, so no new
parser or dependency was added. Other dismember states are unchanged.

`actor` unit tests cover overlapping feet/calves in both partition layouts.
`real_skinning` loads installed body and hand meshes through live skin changes,
checks failed-model rollback, null skins and unchanged inventory. Its cached-view
cost threshold is 2 ms per actor, written before measurement. A five-race survey
checks authored human and beast addon paths against the installed archives.

## Facial viseme pose mapping (ANM-13)

`facial::sample_facial_pose` samples a `VisemeTrack`, maps its shapes through
the supplied race's `PHTN` target names and `PHWT` weights, then applies matching
FaceGen TRI deltas to the TRI base vertices. The returned weights retain every
race target in record order. Targets without a matching TRI morph remain in the
weight list and leave the geometry unchanged. Each sample starts from base
vertices, so an idle cue whose authored weights are zero restores the neutral
pose without accumulating prior frames. `sample_shape_weights` accepts weighted
shapes directly for consumers of native LIP curves.

The bridge uses `audio::viseme::morph_weights`, `nif::HeadTri::apply_morph`,
and the race parser. The local `nifly/include/Animation.hpp` models the
`NiMorphData` NIF block as named frame morph vectors, while `nifxml/nif.xml`
documents that block and its controller separately from FaceGen TRI. The
`TES5Edit/Core/wbDefinitionsTES5.pas` and Mutagen Skyrim `Race.xml` sources
confirm that `PHTN` carries target names and `PHWT` stores per-phoneme weights.
This API returns evaluated positions for the caller to install on a face mesh.
It does not load a race-specific TRI or update a Godot rendering instance.

`anm_13_viseme_sequence_drives_race_declared_morphs_and_returns_to_neutral`
checks that a sequence applies its race-declared target to the matching TRI
morph, leaves unrelated vertices unchanged, and returns the head to its base
pose for an authored idle cue.

The installed five-race survey assembles six model paths and 22 shapes, including
Argonian and Khajiit tails. Both tails name `TailBone01` through `TailBone05`, which
the human rig does not contain. Breton, High Elf and Nord addons match that rig.
ANM-55 reads the sex-specific skeletal `ANAM` NIF and behavior project `MODL`
from the actor's current race. It follows the project data to the character's
HKX skeleton and graph. When the NIF skeleton has bones absent from that HKX
skeleton, the host adds those named nodes with their NIF parent and local
transform. `SkinnedActorNode.bind_actor_appearance` loads that rig with the
actor's full selected skin. When the live race or sex changes, the node loads
the new rig and appearance before replacing the previous view. Every selected
skin shape must map all its bone names onto the race rig or the replacement is
rejected. Paths use the data source VFS and its file overlay.

## Facial expression target mapping (ANM-59)

`facial::expression_weights` maps CK expression ids 0 through 16 to the
corresponding named head blend shape and returns weights for available
expression names only. `apply_expression` sets the selected shape to the
requested percentage and zeros competing expression shapes while preserving
other morph values. The actor NIF pipeline does not currently load Skyrim
`.tri` targets or upload Godot blend shape arrays, so this mapping is not yet
connected to a rendered vanilla head.
The installed body/hand/null refresh fixture measured a 8.2 us cached view against
the declared 2 ms budget. This measures record resolution, not rendered frame time.

A Godot 4.7.2 headless check used five synthetic masters with a player, a skin
containing body/hands/feet addons and one interior static floor. Meshes and animation
came from the installed archives. PlayerNode gameplay binding replaced the original
two shapes and 2,948 triangles with four shapes and 4,668 triangles, with no unmatched
bones. Paused upload and subsequent ticks passed with process exit status 0.
Full installed-data startup received SIGTERM before loading finished, exit 143,
so that larger startup is not claimed as verified. Rust checks execute the node's
same `AppearanceCache::prepare` path, including live record mutation, failed-view
caching and explicit rig mismatch results.

`anm_55_actor_race_rigs` loads installed Nord, Argonian and Draugr skeleton and
behavior paths and requires every selected skin bone to map onto the chosen rig.

The existing `godot/skinned_test.gd` passed on Godot 4.7.2 with the changed
surface ownership: two shapes, 2,948 triangles and 29 matched mesh bones. Its
idle-to-walk transition completed with exit status 0.

Skinned actor meshes resolve each shape's NIF material textures through the
same VFS and DDS loader as streamed meshes. `SkinnedActorNode` opens the nine
vanilla texture archives alongside its mesh and animation archives, creates a
`StandardMaterial3D` from each shape's material and decoded textures, and
attaches it to the uploaded mesh surface. The installed-data check
`rnd_05_vanilla_skinned_body_resolves_its_authored_diffuse_textures` verifies
that the vanilla male body has authored diffuse paths and that each path
resolves to image data. This verifies material inputs, not a rendered capture.

## Nearby interior prefetch, HST-07

PlayerNode checks nearby load doors every 250 ms of simulation time after
applying the Game pose. The game query compares the player's position only
with enabled, non-deleted DOOR references in the same coordinate frame: the
same interior cell, or in an exterior any grid cell of the same worldspace
plus its persistent cell.
It follows each source door's resolved XTEL link and selects the nearest
interior destination within 500 Skyrim units, with FormID breaking distance
ties. Destination coordinates never participate in proximity selection.

One background job prepares the selected interior's meshes, textures and
controller/query collision inputs through the existing `load_location` path.
The job shares immutable load-order and archive handles, uses one decode
thread and owns a private texture cache. Rejected or stale jobs therefore do
not fill the main source's session texture cache. Prefetch does not build game
references, change residency, tick far-side actors or queue cell events.
Activation retains the ordinary Game door lifecycle and cold-cell decision.

The speculative budget admits at most 4,096 authored references before work,
then at most 384 MiB of mesh-array and unique texture payloads and 100,000
collision primitives after preparation. There is at most one outstanding job
or retained destination. These are admission limits, not a peak allocation
or wall-time guarantee. Parser temporaries, container capacity and metadata,
collision shape storage and allocator overhead are outside the byte count.
The collision primitive count is a separate limit. Oversized destinations
use the ordinary synchronous transition when activated.

The host only joins a worker after it reports completion. Selection changes,
source-cell transitions and out-of-range doors invalidate old results without
waiting for a running decode. The worker discards cancelled results when it
finishes. Source replacement and node shutdown join outstanding work so the
shared archive handles do not outlive their owning task. A matching completed
location is consumed exactly once by `sync_game_pose`. GPU upload, physics
server installation and an unfinished or rejected destination still cost time
on the transition. This work does not establish a bounded cold crossing.

Sources read were local OpenMW `apps/openmw/mwworld/scene.cpp`, specifically
`preloadTeleportDoorDestinations`, the existing game's authored XTEL resolution
and doorway event split, and the host's location/collision preparation path.
OpenMW informs proximity scheduling. Its renderer and cell ownership do not
fit this form-backed Godot host, so this code reuses the engine's existing
resource builder. No new package, node per reference, or alternate game-state
store was added.

`godot/hst_07_test.gd` uses installed five-master data and requires
an exterior landing near the Bannered Mare return door to prepare that linked
interior while the player stays outside and rendering stays unchanged. It requires one
job, a nonempty payload within 384 MiB, a cache hit on return, draw instances
and supporting collision. Its predeclared foreground-step budget is 50 ms,
and its preparation deadline is 30 seconds. The limits are specific to this
acceptance run, not a claim about every authored cell or machine.

On 2026-09-23 the script failed on master `9ae933b` under Godot 4.7.2 with
installed data. No prefetch job started (`started` 0, `payload_bytes` 0), one
gameplay step took 497,919 us, and the transition rebuilt its resources. HST-07
was open again until this script passed on master.

On 2026-09-24 the crate's SKYRIM_DATA-gated test
`interior_prefetch::tests::installed_bannered_mare_prefetch_admission` failed
at the previous 128 MiB cap with `references=697 bytes=195769624
primitives=19146 admitted=false`. Decoded payload across five installed
interiors (WhiterunBanneredMare 186.7 MiB, RiftenBeeandBarb 195.6 MiB,
SolitudeBluePalace 230.7 MiB, WhiterunJorrvaskr 269.9 MiB,
WhiterunDragonsreach 295.3 MiB) showed that cap rejecting all of them, so
`MAX_PAYLOAD_BYTES` moved to 384 MiB and the script's `PAYLOAD_BUDGET` with it.

The `started` 0 failure had a separate cause, found on 2026-09-25 by running
the script. Vanilla files exterior load doors in the worldspace's persistent
cell. The return door of the Bannered Mare is `0:01A6F4` in `0:01A270`, the
persistent cell of worldspace `0:01A26F`, while the first exterior step
refocuses the logical cell on the grid cell under the player, `0:01A276`. The
same-cell filter therefore never saw the door. In an exterior the query now
also accepts doors filed in any grid cell of the same worldspace and in that
worldspace's persistent cell, which share one coordinate frame. OpenMW's
`preloadTeleportDoorDestinations` walks the doors of every active cell in the
same way. Interiors still compare only their own cell, and no other interior
or worldspace is ever compared.

The prefetch also dropped a changed selection. A request made while a job was
running cancelled that job and returned, so the new door waited for another
selection tick after the old job finished, and a return to the same door lost
its job. The newest selection is now held as one queued loader and starts when
the running job is collected, and a return to the running door clears its
cancel. A cancelled job is no longer counted as a rejection.

On 2026-09-25 the script passed under Godot 4.7.2 and the 4.4.1 API floor with
installed data: one job, 195,769,624 prepared bytes, ready after 859 ms (4.7.2)
and 1,149 ms (4.4.1), one cache hit on return, and a largest gameplay step of
1,598 us and 3,212 us while the prefetch ran. The script checks that the
player stays outside the prefetched interior rather than in the landing's
cell id, because exterior streaming refocuses that id by position.

Known gap: the first exterior step after the landing took 519 ms (4.7.2) and
707 ms (4.4.1) before any prefetch started. The script reports it as
`landing_step_us` and excludes it from the prefetch step budget. It belongs to
the exterior landing path, not to this row.

## Actor controller geometry resolver, CMB-33

`DataSource::actor_controller_shape(skeleton_path, scale)` in `collision.rs`
reads a skeleton NIF straight through `soe_nif::parse` and
`soe_physics::collect`, then calls `soe_physics::controller_shape` to return
either the skeleton's largest authored `CHAR_CONTROLLER` phantom or a
generated capsule sized by `scale`. It reads the skeleton file directly
rather than through `data_source::placed_model`, since that helper rewrites
a placed reference's root transform for scene placement and a standalone
skeleton has no reference to place. This mirrors the pattern the existing
`actor.rs` skeleton loading and the CMB-29 test already use.

This method resolves geometry only, and nothing in `godot-host` calls it
yet. CMB-28, installing an actor's controller body into the Godot physics
server and keeping it synchronized with the actor's live transform, has not
started: a search of `godot-host/src` for existing actor or skeleton
collision wiring found only `.hkx` animation-skeleton loading in `actor.rs`,
unrelated to the `.nif` collision phantoms this method reads. When CMB-28
adds the actor body pipeline, this is the function it resolves shape and
transform from, with `game::World::actor_controller_scale` (`docs/game.md`)
supplying the scale argument.

## Generated object LOD, EDT-25

`DataSource::load_object_lod_block` reads a shipped BTO through the NIF
reader and surface builder. If the BTO path is absent, it calls
`generate_object_lod_block`, which walks winning REFR records in the resolved
load order and selects the matching level's authored MNAM mesh from each
visible distant STAT. If that mesh is missing, it uses the STAT's MODL mesh
and samples one triangle in every 4, 8, 16 or 32 triangles for levels 4, 8,
16 or 32, compacting the referenced vertex attributes. It excludes deleted
and initially disabled references, references with enable parents unless
visible when distant, and persistent references marked never fade or full
LOD. The reference transform, scale,
worldspace LOD anchor and block origin are applied before each shape reaches
the same surface builder and texture cache used by BTO geometry.

The implementation follows TES5Edit's `Core/wbLOD.pas`, especially
`TwbLodSettings.BlockForCell` and the Skyrim object export in
`BuildObjectsLOD`. The row's named `tes4skyrim/terrain_lod.py` was not present
in the local reference corpus. Generated blocks report contributing reference
count, built and source triangle counts and the NIF mesh byte estimate used
by the same residency budget as BTO blocks. The generated result is held in
memory and is not serialized to a BTO file. Skyrim tree billboard generation
and atlas baking are not part of this path.

## Distant-terrain LOD residency, RND-03

`DataSource::generate_terrain_lod_block` reads winning CELL and LAND records
for a selected worldspace block. Where PNAM requests parent land, its LAND
fills positions without a child LAND. It samples authored heights onto a 33
by 33 tile grid for levels 4, 8, 16 and 32, edge extends missing height
samples, and writes triangles only over cells with LAND. The returned BTR
bytes use the `soe-nif` writer and can be passed to
`DataSource::load_lod_block_bytes` for preview through the same mesh and
surface builder as archived BTR files. A block without LAND returns `None`.

The generator writes a land shape with LOD landscape shader flags. It does
not generate diffuse or normal textures, a water shape, or the vanilla
`BSMultiBoundNode` chunk root.

`crate::terrain_lod` decides which BTR terrain LOD blocks should stand in
for full resolution LAND terrain (`exterior.rs::terrain_surface`) beyond the
cells the cell streamer keeps loaded, and tracks a recorded byte budget for
them. `data_source::lod::DataSource::load_lod_block` reads and builds one
block's BTR file. `CellStreamerNode` drives both, see "The node" below.

### BTR format and naming

Mutagen's `SkyrimModelAssetType.cs` states BTR is "the same file format as
.nif", and TES5Edit's `wbBSArchive.pas` lists `.btr` in the same `atMesh`
asset class as `.nif`. `data_source::lod` reads a BTR file through
`soe_nif::parse` and the same `mesh_data_from` and `build_surface` pipeline
`load_static_surfaces` already uses for placed statics. A BTR's vertices are
local to its block, a real level 4 Tamriel block builds to 0 to 4 cells on
x and y under an identity NIF transform (`tests/terrain_lod_placement.rs`),
so `load_lod_block` composes each shape's transform under a translation to
the block's south-west cell, the part a `REFR` placement plays for a static.

The file path, `meshes/terrain/<world editor id>/<world lowercase>.<level>.<x>.<y>.btr`,
is confirmed against one real shipped file, `meshes\terrain\Vyn\vyn.32.-5.9.btr`
in Wrye Bash's Enderal SE vanilla file list
(`engine-refs/wrye-bash/Mopy/bash/game/enderalse/vanilla_files.py`, line 198),
and against TES5Edit's `miWorldspaceFilterLODClick`
(`engine-refs/TES5Edit/Build/Edit Scripts/Worldspace browser.pas`, lines
895 to 935), which builds a block's LOD texture and mesh from the same base
name and swaps only the extension. Only the level 32 case is an attested
real file name. Levels 4, 8 and 16 follow the same pattern by construction,
not by a second attested example.

The LOD block sizes themselves, 4, 8, 16 and 32 cells to a side, come from
TES5Edit's tree and object LOD generator (`Core/wbLOD.pas`,
`LODLevelMin`/`LODLevelMax`), the only LOD block sizes the corpus documents.
No corpus source documents landscape LOD generation the way `wbLOD.pas`
documents tree and object LOD, so `LodBlockCoord`'s block alignment mirrors
`TwbLodSettings.BlockForCell`, with integer division truncated toward zero
relative to the worldspace's anchor, including negative offsets.

### Residency and budget

`TerrainLodStreamer` mirrors `streaming::CellStreamer`'s hysteresis: blocks
between `full_res_radius` and `lod_radius` (ring distance in cells) are
wanted, nearer or farther blocks are stale, loads and unloads are capped per
tick and sorted nearest first or farthest first. `full_res_radius` should
match the cell streamer's own `load_radius` so the two never disagree about
which cells are near. One streamer covers one LOD level, a caller that wants
more than one level resident at once runs one streamer per level with
disjoint radius bands, finer levels nearer the focus. Picking a level by
distance automatically is left to that caller, unstarted here.

The residency budget is recorded, not estimated ahead of the read.
`Mesh::approx_bytes()` (new in `soe_nif`, see `docs/nif.md`) sums each
vertex's natural width plus four bytes an index. It charges normals
unconditionally, since this crate's own `build_surface` generates a full
normals buffer for a shape with none whenever `generate_missing_normals` is
on (the default), and never charges vertex colors, since neither
`MeshSource`, `MeshData` nor `SurfaceArrays` carries a color field for one
to reach. Close enough to weigh a mesh against a budget without claiming to
match any one renderer's exact buffer layout. `DataSource::load_lod_block`
sums this across a block's shapes into `LoadedLodBlock::approx_bytes`, and a
caller passes that to `TerrainLodStreamer::record_bytes` once the block is
actually built, so `usage_bytes`, `over_budget` and `evict_to_budget` (hard
cap, farthest first) reflect what was loaded rather than a guess made before
the read.

A missing BTR file is `Ok(None)`, not an error: a worldspace's LOD coverage
rarely fills every block at every level, Tamriel's edges and any level above
the one an area was baked at are the ordinary case. `load_lod_block` matches
`soe_bsa::Error::NotFound` the same way `Vfs::read`'s other callers already
do for a missing texture.

### The node

`CellStreamerNode.stream_terrain_lod(world, cell_x, cell_y, full_res_radius,
lod_radius, budget_bytes)` resolves the worldspace by editor id, reads its
`lodsettings` file for the block grid anchor and finest level (level 4 on a
grid at cell (0, 0) when the file is missing), then settles a
`TerrainLodStreamer` around the focus cell. Each planned block is read with
`load_lod_block`, its bytes recorded, the budget enforced, and the blocks
that survive are drawn as `RenderingServer` instances in the node's
scenario, one per surface, kept apart from the cell instances so
`cell_instances` still reports full resolution cells only. A block loaded
and evicted in the same step is never uploaded. Pass the radius
`stream_exterior` drew at as `full_res_radius`: a block is planned only when
all of its cells lie beyond it, so LOD starts where full resolution LAND
stops and the two never overlap.

`update_terrain_lod(cell_x, cell_y)` is the per-frame form: one block read
and two released a call, then the budget. `terrain_lod_resident_count`,
`terrain_lod_usage_bytes` and `terrain_lod_stats` report residency, recorded
bytes, the budget and what the last call loaded, found missing, failed and
released. `clear_terrain_lod` frees the blocks, and `clear` and `exit_tree`
free them with everything else.

On this machine's install, Tamriel's `lodsettings` anchors the grid at
(-96, -96) at level 4. Around cell (0, 0) with a full resolution radius of 2
and a LOD radius of 12, the node draws 45 blocks, 66 instances and 93,065
triangles for 2,659,852 recorded bytes in 61 ms, with no block missing or
failing. With the budget set to half of that, 1,329,926 bytes, 22 blocks
stay resident at 1,298,612 bytes and 23 are released. Checked headless on
Godot 4.7.2 against the release build.

### Open items

- The node does not drive `update_terrain_lod` from `process`. The caller
  calls it when the focus changes cell, the same way it answers the cell
  signals, so a BTR read never lands on a frame the caller did not choose.
- `TerrainLodStreamer` covers one LOD level. Picking a level by distance and
  running several streamers at once (mentioned above) is unstarted.
- `evict_to_budget` is a hard farthest-first cutoff with no hysteresis of
  its own, so a focus that sits exactly on a budget boundary could evict and
  reload the same farthest block repeatedly. The cell streamer's own
  load/unload radius gap avoids this for full resolution cells, the same
  gap is not yet given to eviction here.
- No LOD texture or splat handling, `data_source::lod` builds surfaces with
  whatever material the BTR's shapes carry, same as any other NIF, but nothing
  here addresses a LOD-specific low-resolution texture atlas if one exists.

## Repeated rock instancing, RND-04 (2026-09-24)

The batching versus instancing decision above was reasoned, not measured, at
the time it was written. `src/batch.rs` already carried the full instancing
path from an earlier round: `INSTANCE_MIN_PLACEMENTS`, `instancing_candidates`,
`InstanceGroup`, and `godot_layer.rs` already tries instancing first and falls
back to `batch_surfaces` for the remainder. None of that changed here. What
was missing, and what `INSTANCE_MIN_PLACEMENTS`'s own doc comment named as
outstanding, was a benchmark comparing the two paths on memory and time
instead of judgment.

`crates/godot-host/tests/whiterun_bench.rs` adds
`repeated_rock_benchmark_measures_batching_vs_instancing`, next to the
existing Whiterun streaming benchmark and under the same constraint: Rust
only, no Godot binary. It builds one small rock mesh (300 triangles) placed
40 times at scattered positions, once as 40 already baked `LoadedSurface`
values (the batched path) and once as 40 surfaces sharing one unbaked mesh
identity with per-instance transforms (the instancing path), then runs
`batch_surfaces` and `instancing_candidates` on each and times both over 300
calls. Memory is a CPU-side buffer-byte count (positions, normals, tangents,
UVs, indices, bone data), the same proxy this document already used for an
earlier ad hoc RSS probe, chosen because it is deterministic and needs no
Godot process. Measured on this lane:

- Batched: 530,560 bytes (518.1 KiB) for one merged `Batch`.
- Instanced: 15,184 bytes (14.8 KiB) for one `InstanceGroup` plus 40 stored
  transforms, about 35 times smaller.
- `batch_surfaces` over the 40 placements: p50 544.7 microseconds, p99 649.2
  microseconds.
- `instancing_candidates` over the same 40 placements: p50 10.0 microseconds,
  p99 15.8 microseconds, about 55 times faster at p50.

Instancing wins on both axes at this repeat count, so `INSTANCE_MIN_PLACEMENTS
= 4` and the instancing-first routing in `godot_layer.rs` stay as they are.
This closes the row's done check on the Rust side. It does not measure
Godot's own `MultiMesh` upload and draw cost, or a real exterior's frame p99
with the RenderingServer running, both of which need a windowed Godot run
against a live scene, same as the rest of this document's frame-time numbers.
That measurement is still open and belongs with the other "Numbers" table
entries once someone runs one.

## Per-cell occluder trial, RND-07

`batch::cell_occluder_mesh` selects at most four opaque source surfaces per
cell, ordered by descending triangle count with source order breaking ties.
Cutout and blended surfaces are excluded because they contain visible holes
or transparency. Candidate vertices already converted to Godot coordinates
are copied into one mesh. A `MultiMesh` surface receives its placement
transform before it is copied. `CellStreamerNode` creates one server occluder
instance for a cell when the runtime exposes Godot's 3D occluder methods, and
frees both associated RIDs when the cell unloads or the node clears. The
exported `cell_occluders` switch defaults off pending a measured improvement.

Godot's local `ArrayOccluder3D` and `OccluderInstance3D` docs state that
occlusion culling has CPU cost and is usually not useful for large open
scenes. The trial therefore uses the existing windowed `bench.gd` sampler
with the same scene and camera. `SOE_BENCH_OCCLUDERS=0` disables both the
occlusion setting and generated meshes. Setting it to `1` enables both.
The first Bannered Mare run loaded 318 statics and 145 draw instances with
zero mesh failures, but X11 reported `BadAccess` and the process did not
produce a frame percentile. Baseline p99: not recorded. Occluder p99: not
recorded. The feature remains off until both values can be collected and the
occluder run is lower.

## Water planes, RND-02 (2026-09-24)

A cell with water now streams one extra `LoadedSurface`: a flat quad at the
cell's recorded `XCLW` height, sized to the exterior grid square or, for an
interior cell, to the union of the cell's own already-built surface bounds.
`DataSource::water_surface` reads the cell's own `CELL` record after the
reference scan in `stream_cell_id` finishes, rather than from a `REFR`, since
an interior lake has no placed reference carrying it, only the cell's `XCLW`
and `XCWT`. `esm::records::cell::Cell` already parsed both fields, so no `esm`
change was needed.

Sources read: `TES5Edit/Core/wbDefinitionsTES5.pas` (WATR, lines 10304 to
10429) and the UESP `WATR.wikitext` page for the record layout, both agreeing
that DNAM is a 228 (base game) or 232 (Special Edition) byte block with three
four-byte `wbByteColors` structs (Shallow, Deep, Reflection) at offsets 40,
44 and 48. `openmw/components/nifosg/nifloader.cpp`'s water construction
confirmed a single flat plane with no runtime deformation is the right
starting shape, matching this task pack's own reading of the vanilla engine.
No `esm` record module exists for WATR yet, so `data_source/water.rs` reads
ANAM, DNAM and NAM2 with the same generic `Record::find` pattern
`data_source/models.rs` already uses for FURN and DOOR's `MODL`, rather than
adding a dedicated view for one record type touched by one surface builder.

`mesh::WaterMaterial` (opacity, shallow/deep/reflection colors, one noise
texture path) sits on a new `Option<WaterMaterial>` field on `MaterialDesc`,
left `None` on every ordinary NIF-sourced surface. `MaterialDesc` was the
right extension point over `LoadedSurface`: every non-defaulted
`MaterialDesc { .. }` construction site in the crate is the single
`material_desc_from` call in `data_source.rs`, while `LoadedSurface`'s
construction sites are more numerous and not all defaulted. `batch::BatchKey`
reads only the material fields that already existed, so water surfaces batch
by their (empty) diffuse and normal paths like any other untextured surface,
with no batching code changed. `CellStats::water_plane_count` counts 0 or 1
per cell, matching `statics_with_model`'s style, since a cell has exactly one
water plane no matter how many things could in principle reference its WATR.

An exterior cell whose `XCLW` is absent or holds the reserved `0x7F7FFFFF`
value takes its worldspace's water. UESP's `CELL` page reserves that value,
and xEdit's `wbCELLAfterLoad` shows it as "Default". The height comes from
the worldspace's `DNAM` default water height and the type from its `NAM2`,
both inherited from the `WNAM` parent when `PNAM` sets "Use Water Data".
OpenMW's `mwworld/cell.cpp` makes the same choice for ESM4 cells.
`DLC1VampireCastleExterior01` records the marker and draws at the
`DLC1VampireCastleWorld` height of -14000 units, which is -200.025 metres.
Interior cells never fall back, they need their own `XCLW`.

`godot_layer::water_material` maps `WaterMaterial` onto a
`StandardMaterial3D`: the shallow color is the albedo, `ANAM` over 255 is
its alpha with alpha blending below full opacity, and culling is off so
the plane shows from underneath. `MaterialKey` covers the water fields so
two water types never share a cached material. `CellStreamerNode` gains
`water_planes()`, one dictionary per drawn water instance with its cell,
RID, height, override material, opacity and shallow color, and
`last_stats` gains `water_planes`.

`godot/rnd_02_water_planes_check.gd` streams `DLC1VampireCastleExterior01`
and fails unless one plane sits at the recorded height with a material whose
albedo and alpha match the `WATR` record. It passed headless on Godot 4.7.2
and on 4.4.1, the API floor, against the installed game.

Left open, listed here rather than silently dropped:

- **TNAM (MATT material) is not read.** WATR's `TNAM` names a `MATT` record,
  which itself names shader textures, not a plain diffuse and normal pair.
  No `esm` view or `godot-host` reader for `MATT` exists yet. `WaterMaterial`
  carries only ANAM's opacity and DNAM's three colors, so a water plane's
  authored material beyond flat color is not represented.
- **NAM3 and NAM4 are not carried.** WATR names three noise texture layers,
  only NAM2 is read onto `WaterMaterial::noise_texture`. One layer is enough
  to prove the plane streams and renders before the shader mapping that would
  use any of them exists.
- **No per-instance shading.** One `MaterialDesc` per cell's water plane,
  built once from the cell's resolved WATR winner. Wave animation and other
  per-reference shader parameters were out of scope for this row's starting
  point and remain so.
- **The interior footprint is a heuristic, not an authored bound.** Skyrim
  records no interior water boundary in `esm`'s current CELL view. This
  implementation unions the bounds of whatever the cell already streamed
  before the water lookup runs, in Skyrim units, and falls back to a fixed
  exterior-cell-sized square at the origin for a cell whose only content is
  its water. A dungeon lake's plane can therefore be sized larger or smaller
  than the authored water's true extent, since nothing here reads a specific
  water boundary shape.
- **Deep and reflection colors, noise layers, fog and waves are not
  shaded.** The material is a flat blended color. They wait for a water
  shader.

## Grass placement, RND-11 (2026-09-24)

Vanilla never places grass through a `REFR`. A cell's `LAND` quadrants paint
`LTEX` textures over a 17 by 17 alpha grid, an `LTEX` names the `GRAS`
records it may grow through a repeated `GNAM`, and the shipped engine
scatters instances over the painted area filtered by each `GRAS` record's
density and slope range (`docs/esm.md`, "GRAS and LTEX views"). No corpus
source read for this row (`TES5Edit`, `Mutagen`, OpenMW's
`groundcoverstore.cpp`, which implements a different, non `ESM4` system)
publishes the exact vanilla scatter pattern, so `src/data_source/grass.rs`
places on a deterministic hashed grid instead of reproducing it bit for bit.

`DataSource::grass_surfaces_for_cell` walks each quadrant's 16 by 16 alpha
quads, resolves the painted `LTEX` and its `GNAM` grass list through the
load order, filters by ground slope (`quad_slope_degrees`, a bilinear
gradient over the cell's already decoded height grid) against the `GRAS`
record's declared range, and scatters a bounded, deterministic number of
instances per quad. `instance_count` divides the record's density by the
layer's local opacity into a small per-quad count, backstopped by
`MAX_INSTANCES_PER_QUAD = 6` so an edited or unusually dense record cannot
make one quad's batch unbounded, and `MAX_GRASS_PLACEMENTS_PER_CELL = 512`
caps the whole cell. Each placement's seed comes from `mix`, SplitMix64's
finalizer (Sebastiano Vigna, public domain) applied to stable coordinates
only, the cell's own grid position, the quadrant, the height grid cell and
the instance index, never wall-clock time or shared random state, so
streaming the same cell twice produces the same batch. That determinism is
what the release-on-unload path and this row's done check both rely on.

Placement itself is a synthetic `Refr` built at the placement's world
position and yaw, fed straight into the existing
`DataSource::load_static_surfaces(&model, &refr, true)`, the same private
method STAT, FURN, DOOR and CONT already share, with instancing forced on.
The placements of one cell are collected first, in `ATXT` order, and built
through `build_all` like the cell's statics, so the per-cell
`INSTANCE_MIN_PLACEMENTS` test in `src/batch.rs` (RND-04) sees every copy of a
grass model at once and `instancing_candidates` folds them into one
`MultiMesh` per model and material. `stream_exterior_each` in
`src/data_source/exterior.rs` calls `grass_surfaces_for_cell` right after
building each cell's terrain surface, using the same reconciled height grid,
and folds the instance count into `CellStats::grass_placements`, which
`stream_exterior` reports as the `grass_placements` key.

Release follows the cell. `stream_exterior` records which `CELL` each grid
position resolved to, and `release_exterior_cell(cell_x, cell_y)` frees that
cell's instances, grass `MultiMesh` batches included, so a
`cell_unload_requested(cell_x, cell_y)` from `CellStreamer` can be wired
straight to it. `release_exterior_cell` returns how many instances it freed,
and `cell_instances` drops the cell's key afterwards.

`data_source::grass_tests` drives this on a synthetic `LAND`, `LTEX`, `GRAS`
and `STAT` plugin written by the test: `rnd_11_...` checks that two streams
produce identical placements, that neighbouring cells with the same paint
differ, that a fully painted cell stops at `MAX_GRASS_PLACEMENTS_PER_CELL`
and forms one `MultiMesh`, and that moving a `CellStreamer` focus away emits
the unload of the grass cell and restreaming the new set leaves it out.
`godot/rnd_11_test.gd` runs the same path inside Godot through
`stream_exterior` and `release_exterior_cell`.

## Main menu lifecycle, UI-18

The Game globals `QuitToMainMenu` and `StartTitleSequence` queue transient
host requests on the shared World. PlayerNode consumes them after a gameplay
tick. Returning to the main menu closes any modal menu, drops the active Game,
streaming source, cell and host physics resources, and emits
`returned_to_main_menu`. `is_main_menu` reports this state. Starting a title
sequence first enters the main menu, stores the requested name as the active
sequence, and emits `title_sequence_started(name)`. `active_title_sequence`
returns that name or an empty string when none is active. A plain return clears
any active sequence. Starting a new game leaves the main menu state.

Open gaps, none closed by this row:

- Only the alpha layers (`ATXT`/`VTXT`) are read as grass sources. A
  quadrant's base texture (`BTXT`) can also carry a `GNAM` list, but its
  effective opacity is whatever the alpha layers leave uncovered rather than
  a value the record states outright, so it is left out of this pass.
- `instance_count`'s divisor is a self-designed heuristic, not a published
  vanilla density curve. No corpus source states one.
- Water-level filtering is unimplemented. `Grass` parses the `DATA` block's
  water fields but `grass_surfaces_for_cell` does not read them.
- REGN grass overrides (`RDGS`) are unimplemented, only `LAND`-painted
  `LTEX` grass is placed.
- The `GRAS` `UniformScaling` and `FitToSlope` flags are parsed but not
  applied. Every placement draws upright with yaw-only rotation and uniform
  scale from the height-range jitter alone.
- `wave_period`, the wind-sway parameter, has no shader consumer yet.
- The `CellStreamer` release path in the Godot layer only follows
  `stream_exterior` grid positions. `stream_cell` by editor id does not
  record one, so `release_exterior_cell` cannot drop a cell loaded that way.

## Player trigger crossings, HST-14

`PlayerNode::advance_game` submits the player's previous and current world
positions to `Game::update_trigger_actor_position` after writing the current
pose into the shared world. The game reads XPRM trigger references in the
player's current cell, updates occupant membership and queues Papyrus enter or
leave events. This connects the player controller to trigger scripts without
adding per-reference Godot nodes. Other actor movement systems submit their
own position steps through the same game API.

## Named NIF particle emitters (RND-14)

`ParticleEmitterNode::load_nif(path, emitter_name)` parses an SSE NIF through
`soe-nif`, selects a named volume emitter, and creates one `MultiMesh` and
one `RenderingServer` instance in its 3D scenario. Each `advance(delta)`
updates the bounded `soe-nif` timeline, writes the surviving positions as
MultiMesh transforms and sets the visible instance count. `_process` calls
`advance` while `playing` is true. `particle_count`, `emitted_count` and
`rendered_count` expose the timing and upload counts to
`godot/rnd_14_test.gd`. The node frees its RID on exit.

Each particle draws an untextured, two sided quad in the emitter's initial
color and radius. The node loads a NIF by path and selects an emitter by name.

## Papyrus screenshot capture, HST-19

The `PlayerNode` gameplay tick drains transient screenshot paths queued by
`Debug.TakeScreenshot` and saves the active viewport image as PNG. The node's
`take_screenshot(path)` entry point calls the same Papyrus native and services
its request immediately. `godot/hst_19_screenshot_capture_test.gd`
checks that this path writes a PNG under Godot headless with installed data.
