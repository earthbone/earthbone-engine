# godot-host: streaming, mesh and terrain for the Godot 4 host

## Snapshot adapter, PRF-37 draft

`PlayerNode::start_threaded_gameplay` starts a worker-owned Game using the
streamer's immutable records and VFS. It rejects an already active legacy
Game. `SOE_THREADED_SIMULATION=1` selects this adapter through `start_gameplay`.
The normal entry point otherwise retains the existing direct Game adapters.
Startup script loading, quest initialization, cell binding and navigation
graph construction happen on the simulation thread. Initialization attaches
the plain resident cell IDs captured from the renderer in FormID order,
including the exterior worldspace persistent cell. This follows the existing
`reference_collision.rs` identity source. Contacts still require an attached
exterior grid, so persistent identity does not place every exterior actor.
Subsequent exterior walking residency changes are outside this draft adapter.

In this adapter, physics callbacks step player input and the character
controller, send a coalesced pose and synchronize Jolt actor bodies from
the latest snapshot. They do not advance Game. Process callbacks poll an
immutable snapshot pair and interpolate actor transforms. The streamer applies
them to existing MultiMesh and array-mesh resources without reading World.
Existing actor body synchronization uses snapshot contact shapes. SkyRecords
reads the published sky rather than constructing a gameplay World on the
main thread. No per-reference nodes or runtime dependencies are added.

`simulation_status` reports readiness, initialization errors, tick count,
actor count, package evaluations, tick wall time, PRF-17 search-worker busy
time and continuous lag. `simulation_activate` queues a game activation.
The adapter presents actor transforms, contact bodies and sky state. It does
not consume the legacy menu, save, transition, detection, audio, animation,
runtime light or effect presentation adapters. `stop_threaded_gameplay` joins
the worker before host resources are replaced. This opt-in path is an
integration draft rather than the normal playable host path.

The local Godot thread-safety documentation requires active scene-tree work
to stay on the main thread, and server threading requires explicit settings.
This adapter sends no Godot handles to its worker. Loading/decoding and
navigation retain the existing PRF-17 and AUD-12 paths. The snapshot transform
adapter scans resident batches to locate changed actors, without evaluating
enable chains. PRF-34's reference observer remains a separate draft.

PRF-12's report and F3 overlay append a `main_thread_engine` total computed
from each completed frame's exclusive system scopes. Player input and process
callbacks have covering scopes, so controller catch-up work contributes to
the same render-frame total. Unscoped host idle/render waits and other-thread
work are excluded. The overlay and exit summary list simulation tick and
search-worker busy time separately. Tick time already includes waiting for
navigation workers, so the worker number must not be added to it.

## AI breakdown and overlay, PRF-27

`PlayerNode::ai_breakdown` returns permanent procedure and decision phase slots,
the four process levels, per-actor navigation counters and recent requests,
reaction latencies and stuck actor FormIDs. It retains PRF-11 phase names and
reports microseconds with counts and update intervals. Idle chatter topic
selection has its own measured phase. Unavailable combat style choice and
process scheduling have capability flags.
No optional entity capture is required.

F5 opens the existing system overlay's AI view. It displays all procedures and
twelve decision durations, process counts and cadence, the stuck list, and up
to three path and reaction rows. The complete tables remain available through the
callable. F3 returns to systems, F4 returns to entity capture. The view reuses
the existing panel and label, adding no per-reference nodes. The windowed
`godot/prf_27_ai_overlay_test.gd` check reads the report with entity capture off,
loads installed Bannered Mare NPCs, toggles F5, checks labels and viewport bounds,
captures the window and quits.

## AI phase reporting, PRF-36

`PlayerNode::ai_planning_details` reports inclusive driver phase timings,
actor counts and updates for high, middle-high, middle-low and low processing,
and separate navigation and waypoint worker job counts, busy times and caller
waits. The `ai_movement` row in `system_frame_breakdown` includes that phase
dictionary. F3 displays scheduling, actor processing, procedure preparation,
waypoint dispatch and worker costs below the exclusive system totals. Nested
phase timings describe the last simulation tick and overlap. Worker busy time is not added to main-thread system
time. Caller dispatch and join waits remain charged to AI.

Automatic physics catch-up accumulates detection elapsed time and the latest
player motion, then samples once in the next presentation's first physics
callback. Queries stay on the physics thread and observations use the current
record-backed poses. Queued sound events remain in World until sampled. Manual
`advance` still samples each call. Manual advances, native generation writes
and session recording boundaries flush pending observations. Successful game
replacement, recovery and replay clear the old batch. A failed restored-game
attachment retains it. This host query batch is transient. Saved detection
values remain in World. No scene node is added.

`godot/prf_36_ai_budget_test.gd` uses the windowed walkthrough startup, exits
through its interior door and Whiterun gate, then walks on the Tamriel plains.
It samples 3,600 fixed steps per location through PRF-12, checks that packages
were evaluated, and checks combined plains AI below 0.6 ms p50 and 1.2 ms p99.
The complete driver must also stay below 8 ms. The host integration test runs
this script and the existing PRF-11 Bannered Mare script and collects both
process exit statuses, including after a failed budget. Logs are retained under
`target`. Fixed samples yield to rendering every 60 steps. A second plains
check injects ordinary walking input for 60 wall seconds with automatic
simulation enabled and applies the same limits to F3 presentation frames.
Both scripts quit when their checks end. Build and
import the release extension before running the installed host test.

## Interface art discovery and panel bounds, UI-27

PlayerNode selects FFDec before building its interface cache. It tries a
valid `SOE_FFDEC_JAR` with Java on PATH, then the executable named by
`SOE_FFDEC`, then directories in `SOE_TOOLS_PATH` and the Godot project
setting `soe/tools/path`. That setting accepts an absolute path or a Godot
resource path. Tools directories may contain FFDec directly, under `FFDec`,
`ffdec`, `tools/FFDec` or `SkyUI-Community/tools/FFDec`. A configured path may
also name the JAR or executable itself. Discovery prefers `ffdec-cli.jar`
over `ffdec.jar`, then executable launchers. It also checks nearby `tools`
and `engine-refs` directories, `Projects/Programming/skyrim-modclients/engine-refs`
under each ancestor of `HOME`, the conventional system Java directories, and
`ffdec-cli`, `ffdec` or `ffdec.sh` on PATH. Missing or invalid candidates fall
through to the next location.

The source is local `SkyUI-Community/tools/FFDec/ffdec-cli`, whose JAR
manifest resolves its dependencies relative to the JAR, and `ffdec.sh`,
which documents installed Java and system JAR locations. The host reuses
UI-19's out-of-process converter with its existing headless Java command
and local-only import policy. It logs the chosen JAR or launcher once per
process. Without FFDec it logs the bundled-theme selection and skips cache
conversion. Conversion errors also select the bundled theme. PlayerNode
retains either result across menu openings.

Inventory, container, barter, favorites and magic presentations set their
shared shell to horizontal anchors 0.04 and 0.96 and vertical anchors 0.08
and 0.92, independently of art availability. At 640 by 360 this gives a
588.8 by 302.4 panel. Category buttons use 10-point text and 20-pixel minimum
heights with no vertical separation. The native layout keeps categories
before the item columns, search above the list and the item card beside it, with Take and
Store tabs for containers. This follows `InventoryLists.as`'s bounded
content panel and `ItemMenu.as`'s separate floating card. These proportions
are the native host's viewport adaptation. Converted art never sets control
bounds. Other modal presenters retain their existing shell layout.

The `ui_27_` checks cover discovery precedence, tools-directory JARs,
PATH launchers, executable permissions, absent Java, missing tools and
invalid overrides. Installed-data checks export vanilla container actions
with real Java and FFDec and open a vanilla container with no FFDec, a broken
JAR and a configured FFDec install. The windowed scene compares panel bounds,
checks category/list/card order and bundled art, reopens the container,
captures the viewport and quits. Captures and logs go under `target/ui-27`.
No dependency, saved state or per-reference node path is added.

## Default menu art, UI-24

The UI-20 inventory, container, barter, favorites and magic menus share
`godot/ui/theme`. Its 15 generated Godot meshes contain NORDIC UI 2.4.1's
inventory category icons, Favorites and general magic icons, an item-card
frame, a background and an equipped marker. Item rows reuse the corresponding
category artwork for their ten generic item types. The favorites groups Gear,
Aid and Magic use Lorc's Battle gear, Potion ball and Magic swirl SVGs from
Game-icons.net under CC BY 3.0. The theme manifest names every resource.
`godot/ui/theme/CREDITS` records the authors, links, version, dates, permission
quote and the fallback icon licence.

The local source was
`engine-refs/mods/NORDIC-UI-2.4.1/README.lead`. The
[NORDIC mod page](https://www.nexusmods.com/skyrimspecialedition/mods/49881)
grants use of the author's art and asks users to check SkyUI and SkyHUD
permissions separately. The bundle therefore contains only generated art,
with no original mod files, archives, scripts, fonts, menu layouts, SkyHUD
components, third-party patches or WAV sounds. The source does not establish
the sounds' origin. The [Game-icons licence](https://game-icons.net/)
requires attribution, supplied alongside the icons. Native theme mappings and
Godot controls remain GPL-3.0-or-later.

`nordic_theme::extract` reads the user's NORDIC install through
`SOE_TEST_NORDIC`, SkyUI's archive through `SOE_TEST_SKYUI` and vanilla's
`Skyrim - Interface.bsa` through `SKYRIM_DATA`. The extraction binary takes
an empty output directory as its argument. It scans every SWF and GFX art
definition in both reference archives, including nested sprites. A shape or
bitmap is excluded if its tag type and payload, excluding the movie-local
character ID, match any reference definition. A shape that depends on an
excluded bitmap is also excluded. Duplicate art IDs and malformed envelopes
return errors. The UI-19 converter reads an art-only reconstruction and its
`write_art` entry point writes meshes and bitmaps without movie metadata.
The extractor reads four NORDIC SkyUI reskins, category icons, inventorylists,
itemcard and mapmarkerart. The bundle selects 15 of the 125 definitions that
pass these checks. `nordic/provenance.json` names each selected source and its
SHA-256 fingerprint. No standalone DDS or NIF is extracted.

The extractor reuses Ruffle's tag length reader and the existing UI-19
converter rather than implementing SWF geometry or resource encoding.
`serde_json`, `sha2` and `swf` are direct host dependencies already present
through that converter, with no new resolved packages. No saved state or
per-reference node path is added.

Converted winning install art is tried before the default theme. Category
labels use SkyUI's `inv_*`, `cat_favorites` and `mag_all` names, checked in
local SkyUI Community's `source/swf/skyui/icons_category_psychosteve.xml`.
Generic spells use `mag_all`, since the item icon source has no
`default_spell` label. Panels select the named `background` instance inside
`ItemCard`, while markers use the `EquipIcon` symbol in inventorylists.
Character IDs parsed from JSON are converted to integers before timeline
lookup, following local Godot `classes/class_json.rst`'s numeric conversion
contract. Zero-alpha helper shapes are excluded using both the multiplier
and additive alpha terms recorded in SkyUI's icon XML. Native controls tint
panel meshes for text contrast. These install caches remain external and
uncommitted. NORDIC theme meshes supply missing install art.

The `ui_24_` tests cover duplicate shapes and bitmaps, renumbered IDs, nested
sprites, excluded bitmap dependencies, malformed input, art-only output,
bundle attribution and fresh extraction of the selected resources. The real
install case skips when any of the three data variables is unset. The
windowed scene checks every category and item type, panels and equipped
markers across the five UI-20 modes, with a second run for installed SkyUI
precedence. Both runs capture the viewport and quit. Render checks cover dark
panel contrast and visible gaps in the weapon icon silhouette.

The installed-data run passes all nine UI-24 tests, including both windowed
640 by 360 scenes on Godot 4.4.1 with llvmpipe. The selected art totals
1,343,317 bytes of generated meshes. Captures are written under `target`.

## Entity host capture, PRF-13

The existing system overlay uses F4 to toggle `soe_trace::Profile`, displaying
the top five actors, scripts, quests and plugins by self time in the preceding
second. F3 retains its system view. Switching F4 off or leaving the player
writes `user://soe-profile.json` and `user://soe-entities.json` for Perfetto.
Headless startup also honors `SOE_PROFILE=1`.

PlayerNode collects key-event latency tokens while profiling. A physics step
moves control tokens into the presentation queue after application. Opening
or closing a menu queues its token immediately. RenderingServer's
`frame_post_draw` signal completes the presentation queue. This measures elapsed time from a key
callback through the next applied and presented frame. It does not infer
whether an unbound key changed a visible pixel. Audio playback scopes count
voice and effect starts at the existing `AudioStreamPlayer3D.play` call.
RenderingServer instance creation uses a shared submission wrapper to record
its elapsed time and count in one output entity.

The overlay reuses its panel and label and adds no per-reference nodes. No
saved fields or dependency were added. The windowed `prf_13_` host check
covers F4, an applied camera switch, presentation latency, an audio start,
installed render submissions, summary JSON and an overlay capture. It uses
the existing input path and the local Godot RenderingServer signal contract.
`prf_13_windowed_host_overlay_script_passes_with_synthetic_audio_fixture` in
`crates/game/tests/prf_13_entities.rs` runs the script in a window against the
built extension with SKYRIM_DATA, SOE_GODOT_BIN and a display set, so an overlay
key remap that leaves the script behind fails `cargo test`.

## Hitch dumps, PRF-18

PlayerNode resolves `user://hitches` through Godot's ProjectSettings when it
enters the tree. Both its presentation frames and nested game tick frames
feed `soe-trace`'s always-on flight recorder. A frame above 33.4 ms captures
the preceding five seconds plus one second afterward. A background writer
emits Chrome trace JSON and a one-line summary naming the slowest
spans. Dumps have a ten-second cooldown. Leaving the tree joins submitted
writes and reports errors. Pending captures need their one-second tail to
complete before exit. This adds no per-reference nodes or saved state.

The local Godot `classes/class_projectsettings.rst` documents the user path
resolution. `godot/prf_18_hitches_test.gd` lets startup's dump cooldown pass,
forces an 80 ms pause, parses the dump with Godot's JSON parser, checks prior
frames, tail, counters and summary, then frees its node and quits. It runs
both headless and windowed, without game records. Recorder details and the
headless Rust output path are in [trace.md](trace.md).
`crates/godot-host/tests/prf_18_hitches_host.rs` runs the script headless in
an isolated project under the same gating.

## Cell loading and memory attribution, PRF-20

`CellStats::profile` retains a cell-local `soe-trace` recorder through model
workers, textures, collision preparation and host texture creation. Exterior
terrain and grass enter the same capture. `CellStreamerNode.streaming_profiles`
reports each resident cell's stage durations in nanoseconds, I/O by winning
archive or loose source, the slowest canonical asset, and distinct texture
payload bytes with their sum. CPU durations include waits and sum worker work.
Texture upload measures Godot resource creation, excluding GPU execution.
Warm material and texture cache hits produce no new upload work. Clearing or
releasing a cell removes its report. Shared textures are charged once to each
cell that uses them, including every cubemap face and generated LAND albedo.
Material caches can retain resources beyond cell release, so these per-cell
totals describe cell usage rather than global physical texture residency.

`plugin_record_memory` counts requested capacity of retained record bodies,
subrecord tables and subrecord payloads per defining plugin, including losing
overrides and raw TES4 records. Decoded header metadata, group tables and shared
load-order indices are outside these record-body totals. The implementation
uses the existing ESM structs rather than a second record store.

All builds use a counting Rust System allocator. The allocator adds an aligned
owner prefix, preserves that owner across reallocation and frees on other
threads, and leaves Godot's own heap outside the counters. This adds a small
header and counter updates to Rust allocations. PlayerNode's
`system_frame_breakdown` includes `live_bytes` and `allocations` for every
system in the last completed presentation frame. Reallocations count as calls.
Unscoped allocations use `other`. No saved state or per-reference nodes were
added. These hooks use the Rust allocator contract and the model timing table
in local `godotwind/src/core/world/performance_profiler.gd`.

`streaming_diagnostic` supplies the slowest asset and adds it to the calling
thread's bounded hitch history for PRF-18 dumps. The PRF-20 Rust installed
check loads Bannered Mare and WhiterunOrigin and checks preparation categories,
NAVM handoff, plugin record bodies and texture sums. The headless Godot script
`prf_20_attribution_test.gd` checks actual host uploads, source reports, sums
and profile removal. It measures no rendering throughput.

## PRF-24 runtime metric publication

`PlayerNode::performance_metrics` returns the stable `soe-trace` metric names
and values for the overlay and exit summary. `SoundBank` updates the active
voice gauge each process frame and adds time spent decoding uncached sounds.
This does not measure event-to-first-sample latency or the Godot output
latency. Animation evaluation phases are not connected to a live actor
animator in this host, so their snapshot values remain zero. Path-cache
measurements remain zero because no path cache exists.

## Transition timing, PRF-26

`PlayerNode` emits `transition_requested(kind, name)` before opening or closing
the modal menu shell, dialogue activation, a load-door activation, and native
generation save, quick-save, quick-load and full-load work. The walkthrough's
system overlay waits for `RenderingServer.frame_post_draw` for each request,
then records the elapsed wall time under that transition key. On player exit it
prints and writes `user://soe-transitions.txt` with the sample count, p50 and
worst time. These measurements include requested synchronous work and end
after the resulting frame is drawn. Load-door timing does not include a separate
measurement of the first input callback after resource replacement.
The completion boundary follows local
`engine-refs/godot-docs/classes/class_renderingserver.rst`, which defines
`frame_post_draw` as emitted after all Viewports finish updating.

`godot/prf_26_menu_transitions_test.gd` opens and closes the named modal menu
shells in a windowed run and builds 500 label rows in each. It times its list
construction, string filtering and sorting operations by menu and item count.
The list workload is a synthetic scene check. It does not time each native
presenter's record-backed list construction. Fast travel has no host transition
entry point in this checkout, and its timing is not reported.

## TST-18 walkthrough gameplay timing

`walkthrough.gd` exposes startup cell, position and exterior radius to derived
scripts. Exterior stream statistics include the resolved grid origin, which
the timing script uses for its spawn. Its default remains the Bannered Mare
spawn. Headless startup skips render pipeline preparation and still loads the
same data, collision and
gameplay runtime. `tst_18_walkthrough.gd` inherits that startup for both the
Bannered Mare and a Whiterun exterior block, then settles gameplay for 20 wall
seconds and measures host frame intervals
with fixed player advances for at least 60 wall seconds. It checks that the
logical cell remains the requested cell. It enforces the budgets and active
package checks
in [performance.md](performance.md), and quits after the measurement.
`PlayerNode.ai_planning_details` includes selected supported package and NAVM
triangle counts alongside phase timings and instantiated actors. These are
read-only observations of the shared world. `probe_npc_navigation` queries the
existing graph at the shared positions of NPCs with supported packages and
returns query and success counts. The timing script calls it once per
simulated second within a measured frame to keep lookup cost visible despite
retained planner results, matching AI's package evaluation interval.
No saved state or per-reference node path is added. The monotonic clock follows
local Godot Time documentation.

## Per-system frame monitors, PRF-12

PRF-35 adds `magic_frame_breakdown` for the Game magic tick phases and the
casting and dispatch remainder.
The F3 overlay displays their last, p50, p99 and worst milliseconds, and
`system_summary` appends their session totals. They are included in the
existing magic slot and do not increase the system total or its frame share.
`end_budget_frame` includes their latest tick times under `magic_phases`.
Phase percentiles cover Game ticks, while system percentiles cover host
presentation frames. The windowed `prf_35_magic_test.gd` drives the actual
walkthrough indoors and through the authored city gate to the Whiterun plains,
retains 360 samples per location and checks magic below 0.2 ms p50 and 0.5 ms p99.
The thresholds and before/after measurements are in [performance.md](performance.md).

The Grab Actor hook reads indexed active applications and selects a resident
body in stable target order. Runtime light sync reads indexed summoned
references, decodes LIGH data when creating a light, and updates RID transforms
and fades only when they change. Expiry and visibility still release RIDs.
These paths add no per-reference nodes or saved fields. They reuse the shared
MAG-35 and MAG-66 contracts and Godot RenderingServer RID APIs.

PRF-16's `begin_budget_frame` and `end_budget_frame` delimit one fixed-step
CPU window using the same exclusive PRF-12 slots. Nested begins fail, an end
without a begin returns an empty dictionary, and successful ends return the
wall milliseconds and every system's milliseconds. These temporary windows
do not change presentation history or saved state. `budget_runtime_status`
exposes completed Game ticks, real simulation seconds, package evaluations,
cached navigation triangles and the current resolved worldspace editor id
to reject stopped or misdirected runtime samples. The installed
`prf_16_walkthrough_test.gd` extends the actual walkthrough startup with manual
steps and captures the full CPU scenario, including authored-door activation.
Each manual advance ticks Game once. The open-menu branch owns menu-mode
ticks, while normal gameplay no longer ticks again through a closed MenuState.
Headless startup skips rendering warm-up. Windowed startup retains it.
`gameplay_doors` exposes the linked door's return position and yaw from the
decoded XTEL transform when the return link names the original door. This
supplies the walk approach without hard-coded placements. The source is
local TES5Edit/Core/wbDefinitionsTES5.pas, REFR's XTEL definition. The suite
and its fixed budgets are described in [performance.md](performance.md).

PlayerNode registers one Godot Performance custom monitor per `soe-trace`
system when it enters the tree and removes its own monitors when it leaves.
The names are `SOE/<system>_ms`. A second player uses a category containing
its instance ID to avoid duplicate monitor names. Values are nonnegative
milliseconds from the latest completed presentation frame. The callbacks
read timings without computing percentiles.

The host records input callbacks, gathered controls, player physics and actor
body sync, detection, streaming, audio work and render submission. Game scopes
inside the host frame retain exclusive accounting. Host frames end after
PlayerNode's process work and use elapsed wall time between these boundaries,
including physics ticks, idle time and rendering waits. These scopes include waits inside host calls. They do not measure GPU
execution or Godot's internal mixer/physics work.
`system_frame_breakdown` returns last-frame milliseconds and percentages with
rolling p50, p99 and worst. `system_summary` returns session totals, means
and worst times with rolling percentiles for every system.

The walkthrough's F3 overlay displays these values and writes
`user://soe-systems.txt` on the player's `tree_exiting` signal, also printing
the summary while the node remains valid. The overlay is
one CanvasLayer with a panel and label, with no nodes per placed reference.
`godot/prf_12_monitors_test.gd` reads every monitor in a live scene, checks
duplicate-player ownership and removal, toggles the overlay with F3, verifies
its labels and frame shares, and writes and reads the summary. A windowed run
can save an overlay PNG through `SOE_PRF_12_CAPTURE`.

The same overlay and exit report include Godot's process, physics and
navigation times, object and node counts, orphan nodes, static memory,
rendered object/primitive/draw-call counts, video and texture memory, 3D
physics counts, audio output latency and pipeline compilation counters. The
root viewport reports visible, shadow and canvas objects, draw calls and
primitives plus measured render CPU and GPU milliseconds. The overlay checks
orphan node counts every ten seconds and flags an increase. Render timing is
enabled for the overlay's viewport while it is in the tree.

`CellStreamerNode.render_attribution` groups its live batch RIDs by loaded
cell and derives source-plugin membership from each batch's reference table.
Cell values count the cell's batch RIDs, one draw call per batch and the
uploaded triangle count, with MultiMesh triangle counts multiplied by
placement count. A plugin is counted once for each batch containing one of
its references, so merged batches can appear under more than one plugin. These
totals include resident door surfaces, attributed to each door's cell and
reference plugin with their uploaded triangle counts. These are resident
batch totals. Godot's per-frame viewport counts exclude culled
objects and are reported separately. The installed-data
`godot/prf_30_godot_render_report_test.gd` checks these counts against the
streamer's created batch count and checks monitor, viewport and report fields.

The report retains unmeasured render fields as `null`, with reasons in
`render_attribution.unavailable`. These include per-stage culling, unloaded
references, LOD switches, per-light shadow casters and per-frame material and
mesh creation. The overlay names those unavailable fields. A null value does
not mean zero. Resident batches and aggregate viewport figures do not supply
these measurements, and PRF-30 remains open for their instrumentation.

PRF-30 is partial. The Godot monitors, the per-viewport render info with CPU
and GPU render time, the orphan growth warning and the per-cell and per-plugin
resident batch totals are measured. Still missing: frustum, occlusion,
visibility range, LOD and unloaded cell cull counters per stage, the LOD
switch rate, shadow casters per light, and material and mesh creation counts
per frame. The installed-data test accepts a null with a reason for those
fields, so it does not measure them. The test also cannot reconcile per-cell
counts with Godot's objects in frame, because the cell figures are resident
batch totals and the in-frame figure excludes culled batches. It checks only
that the in-frame count is positive and not above the resident total. Exact
reconciliation needs per-batch in-frame membership, which needs the cull stage
counters above.

The physics row also exposes `physics_frame_breakdown`. It reports the number
of physics callbacks since the last presentation process callback, inferred
catch-up steps, Godot's physics-step duration and PHYSICS_3D active body,
collision pair and island monitors. Wrapped query counts and elapsed time are
grouped by their active `soe-trace` system. The current host adapters wrap
raycasts. Shape-cast and overlap categories have no instrumented host callers.
Catch-up lasting at least one second emits one warning per continuous streak.
The query timing covers wrapped host calls only. Godot's physics-frame monitor
reports the last physics-step duration. The report does not split Jolt
integration time among player, NPC, ragdoll or dropped-item bodies.
Collision-shape build timing is
not yet measured. The breakdown retains `player_controller_us`,
`npc_controller_us`, `ragdoll_bodies`, `ragdoll_us`, `dropped_item_bodies`,
`dropped_item_us` and `collision_shape_build_us` as nil until their measurements
are available. A nil value indicates an unavailable measurement, while a
numeric zero indicates measured work with no elapsed time or bodies.
`godot/prf_25_physics_test.gd` runs three rigid capsule bodies and 100 direct
space rays per physics callback and checks the report's values and categories.

Monitor naming, callable arguments, nonnegative values and removal follow
local `engine-refs/godot-docs/classes/class_performance.rst`, checked against
the [official Performance reference](https://docs.godotengine.org/en/stable/classes/class_performance.html).
The binding uses only the Godot 4.4 API. No built-in process monitor supplies
the frame denominator. Exit ownership follows local
`godot-docs/classes/class_node.rst`. The fixed accumulator is described in
[trace.md](trace.md).

The HTML menu embedding research and proposed PrismaUI compatibility contract
are in [ui-frameworks.md](ui-frameworks.md#html-menus-and-prismaui-compatibility-ui-22).
They describe options only. No HTML renderer or PrismaUI API adapter is
implemented in the host.

## SWF menu execution research, UI-23

`docs/ui-frameworks.md` records the source-level Scaleform and `GameDelegate`
inventory, Ruffle's signature change, compatibility gaps, estimates, and
proposed runtime rows. `ui_23_scaleform_research.rs` checks the documented tag
and API inventory, compares per-class vanilla calls with the supplied SkyUI
ActionScript source when it is available, and checks the proposed estimates
and work split. This host still presents its native Godot menus. UI-19 converts
SWF art and retains ActionScript decompilation for reference, but does not
execute the scripts.

The art converter uses Ruffle `swf` 0.3's `Reader` for typed tags. Its
`read_tag_list` requires an End tag, while the local
`SkyUI-Community/source/swf/skyui/icons_item_psychosteve.xml` records complete
sprites with `hasEndTag=false`. The converter walks sprite envelopes through
Ruffle's length reader and accepts their declared byte boundary. Static
one-frame sprites in the same icon movie also omit ShowFrame. Their final
display list and labels form that frame. Other frame-count mismatches, subtag
truncation errors and nesting beyond 64 levels remain errors. Synthetic
fixtures cover both omissions, nested sprites and partial tags.

Empty-depth Modify and Replace tags leave the display list unchanged, following
normal frame playback in Ruffle's
[MovieClip::place_object](https://github.com/ruffle-rs/ruffle/blob/master/core/src/display_object/movie_clip.rs).
PlaceObject2/3 payloads must be fully consumed by the typed reader. The breaker
fixture for an unoccupied depth contains an extra payload byte and is rejected
for that malformed envelope rather than its empty depth.

## Engine system profiler, PRF-23

The host registers a Godot `EngineProfiler` named `soe_systems` at the Scene
initialization stage and unregisters it when the extension shuts down. When
Godot's debugger enables it, PlayerNode sends each completed presentation
frame as seconds for the frame followed by name and seconds pairs for all
fourteen engine systems. `EngineProfiler` receives these payloads through
Godot's `EngineDebugger.profiler_add_frame_data` API. The adapter and payload
use the local Godot 4.4 `class_engineprofiler.rst` and
`class_enginedebugger.rst` contracts. The editor acceptance script verifies
that the extension registered the profiler. It does not inspect the editor's
Profiler tab display.

## Scripted player input, PLY-01

PRF-22 exposes `record_player_session(seed)`, `write_player_session(path)` and
`replay_player_session(path)` on PlayerNode. Paths are absolute filesystem
paths. Recording and replay use the same PLY-01 driver as headless Game.
Set `auto_step` to false for explicit scripted stepping. Replay restores the
Game snapshot and synchronizes the controller and cell resources, then
`step_player_input` verifies the recorded outcome and each scheduled hash.
`player_session_error` returns the latest divergence and `player_session_hash`
returns the current Game and VM hash as hexadecimal text. A failed replay
clears its pending queue and rejects further steps. Device controls outside
the scripted input layer are not recorded by this transport.

`godot/prf_22_session_test.gd` records 600 steps against the exported PLY-01
fixture, replays the file twice, compares eleven hashes and complete event
logs, then quits. It checks simulation and host transport without measuring
render throughput. PRF-13 summaries also include host presentation system
totals separately from overlapping Game tick totals. No per-reference node
path or native save field is added.

`PlayerNode.queue_player_input(dictionary)` transports commands to the shared
`soe_game::player_input::Driver`. `step_player_input` executes one queued
command at 1/60 second, synchronizes the controller and camera from World,
replaces host resources after a cell change, and presents current dialogue
choices or responses through the existing menu. `advance` also services a
queued command before device input. Door presentation reuses the existing
swing-door nodes.
`player_input_log` returns the same ordered event representation as headless
Game. `clear_player_input` clears the transient queue and log.

Every dictionary has an `action` string. Reference commands use integer
`file` and `object` fields. MoveTo uses `move_to`, a Vector3 `offset` and
`match_rotation`. Turn uses `turn` and a Vector3 `rotation` in radians.
Activation uses `activate`. Authored melee contacts use `attack`,
`weapon_file`, `weapon_object` and `power`. Casting uses `cast`, `hand`
(`left` or `right`), `held` and `has_target`, plus a reference when targeted.
Topic choice uses `choose_topic`. `confirm_menu` names `dialogue`, `gift`
or `craft` in `menu`. Craft names its bench as the reference and its recipe
in `recipe_file` and `recipe_object`. `wait` advances one step. Invalid
dictionaries do not enter the queue. Scripted steps use the Game simulation
clock even while a dialogue menu is visible.

`godot/ply_01_player_input_test.gd` loads the synthetic fixture exported by
the game test with `SOE_PLY_01_FIXTURE`, drives the same eleven commands
windowed, compares complete event logs including Papyrus traces, checks the
controller pose and load-door cell replacement, and verifies a visible
dialogue response. It captures the viewport as `window.png` in that fixture
directory and quits with a failing status if any assertion fails.

## Papyrus log profile routing, TOOL-07

`DataSource` retains the resolved profile INI settings and INI directory.
`PlayerNode` configures Game's Papyrus file sink from them before scripts start.
When `[Papyrus] bEnableLogging=1`, the sink writes `Papyrus.0.log` and named
user logs under `Logs/Script` in that profile directory. Disabled logging does
not create files. The profile settings control trace lines and debug source
detail, and existing numbered logs move up one slot on startup.

## Item names and model cards, UI-25

Gameplay startup and restored-generation attachment use
`DataSource::localized_game`. It loads the profile's selected-language tables
through the existing VFS before menu names are read. Malformed tables fail
startup with the table path. The table parser and winning-plugin namespace
resolution remain in ESM and Game. This fixes the host path that previously
loaded scripts without loading localized text. Initial inventory LVLI entries
now become concrete items in Game, so their nameless list records no longer
reach menu rows.

Inventory, container, gift and crafting menus share an item card beside a
scrolling list. Keyboard focus and mouse hover select the card independently
of the row's action. Crafting rows use the output item's FULL name rather than
the usually nameless COBJ recipe. Missing or unresolved mod names display
`Unnamed item` instead of a FormID or string-table marker.

The card reads authored weight and base gold value, plus weapon/ammunition
damage or armor rating. Weapon and armor physical fields follow their template
chains while the child supplies its display name. Armor uses MOD2/MOD4 ground
models according to the player's selected sex, with MOD2 as the female
fallback. Clothing reports zero armor. Invalid numeric fields and template
cycles return ESM errors. These are record statistics, without skill, perk,
tempering or enchantment price adjustments.

The selected NIF is built through the existing VFS, mesh and material pipeline.
Its centered bounds fit a camera in an isolated transparent SubViewport with
two directional lights. The model turns at 0.4 radians per second while the
menu is open. Refresh and close free the viewport and its meshes. Empty
categories show the unselected card. Missing models leave the item's text
visible with a model status. Only the selected preview allocates MeshInstance3D
nodes, one per surface. Bulk placed references keep their server-resource path.

Sources read were local UESP Mod File Format/TES4 localization flags and item
pages, xEdit `Core/wbLocalization.pas` and item definitions, SkyUI
`src/ItemMenus/ItemCard.as`, and godotwind
`src/tools/asset_viewer/components/preview_3d.gd`. The separate UESP String Table
page is absent from the local dump. The existing xEdit-based ESM reader handles
the table layout. The Godot control implementation reuses the host's renderer
because the reference previews do not consume this engine's neutral surfaces.

The `ui_25_` Rust tests cover card fields, armor and weapon templates,
selected-sex ground models, localized overrides, startup language selection,
malformed tables and installed container, player and merchant inventories.
`godot/ui_25_item_card_test.gd` checks a windowed vanilla sword and armor
preview, rendered pixels, rotation, focus/hover selection, a container transfer,
category refresh and viewport cleanup. It writes screenshots under `target`.

The installed-data survey resolved 80 inventory rows and loaded 45 ground
models. The windowed check passed on Godot 4.4.1 with llvmpipe at 640 by 360.
The selected iron sword used four preview mesh nodes. Its first synchronous
model build took 1,199.392 ms during concurrent Cargo checks, including NIF,
texture and mesh preparation. This measures menu selection latency on that
software-rendered run.

## QuickLoot containers and corpses, UI-26

The host observes the shared crosshair reference and shows an unpaused compact
list beside it for eligible unlocked containers and dead actors. It hides the
list during modal menus, grabs, disabled presentation and invalid targets.
Up and Down select rows, E takes one, T takes all, R cycles the sort, Tab or
Enter opens the full container menu, and Escape closes the list. The card uses
the same record-backed model preview as UI-25. Rows show category glyphs and
an ownership marker. Empty and locked targets have separate states.

Game performs each take against the shared inventory and preserves owner extra
data. API `ModifyInventory` rows can add valid named display entries, following
the contract in QuickLootIE `include/QuickLootAPI.h`. Such presentation-only
rows cannot bypass Game's inventory selection validation during a take.
`soe-commonlib-shim::quick_loot` implements the source-level MIT header surface
without importing GPL code. The host behavior follows QuickLootIE
`src/LootMenu.cpp`, `src/LootMenuManager.cpp`, `src/LootMenu.Sorting.cpp`,
`src/Items/ItemStack.cpp` and `include/QuickLootAPI.h`. The GPL-3.0-or-later
notice and its modding exception remain with the host implementation.

`ui_26_adversarial.rs` checks shared inventory transfers, stolen owner data,
empty and locked states, event cancellation and API-added rows.
`godot/ui_26_quick_loot_test.gd` checks the installed-data host scene, take,
take-all, empty presentation, owned-row marker, item card and full-container
handoff. This does not verify a live original-game probe, physical speaker or
human window observation.

`godot/ply_04_route_test.gd` runs a JSON route through the same PlayerNode
queue in a windowed Godot process. The route has a `start` object with a cell
editor id and a three-number Skyrim-unit position, followed by `steps`. Each
step has a `command` dictionary, optional frame count, and optional checkpoint
name. MoveTo `offset` and Turn `rotation` arrays are converted to Vector3
values. Each checkpoint saves a viewport PNG, and every step records its
action, sampled frame count, worst wall-frame time, and capture path in
`frame_times.json`. Route or capture failure writes a failed result and exits
with status 1. `godot/ply_04_route.json` is a one-step installed-data smoke
route through WhiterunBanneredMare.

Set `SKYRIM_DATA` to the installed Data directory. Optionally set
`SOE_PLY_04_ROUTE` to another route JSON file and
`SOE_PLY_04_OUTPUT_DIR` to the capture and log directory. Run the extension
windowed with:

```text
godot --path godot --resolution 640x360 --script ply_04_route_test.gd
```

## Live detection observations, CMB-32

PlayerNode samples ordered pairs of resident, living actors in the same coordinate space
within the winning `fSneakMaxDistance` after the player's pose reaches Game.
Their FormIDs come from
`World::contact_actor_placements`. A Godot physics ray from eye to eye reports the first
body, including an intervening actor or static obstruction. The query uses
the existing RID-backed actor bodies and allocates no reference nodes.

`detection::light_level` reads the winning CELL/XCLL ambient RGB, LGTM
inheritance, and enabled authored or runtime placed LIGH/DATA radius, color,
negative and off-by-default flags with FNAM fade. It samples current reference
placement and enable state, including `PlaceAtMe` references, and bounds the
resulting record-derived light estimate to 0 to 150. The host sends that
value, sight, motion and queued detection sounds to
`World::evaluate_detection` after each manual step, or once after automatic
physics catch-up as described in PRF-36 above.
PRF-31 builds a uniform actor grid and a separate grid of sound source
positions each frame. A nearby event can belong to a distant actor and does
not grant sight to its owner. Each target's light is sampled once per frame.
`LightSamples` reads `World::light_references_in_cell` for each actor cell and
bins current light spheres in a 1,024-unit grid. A sphere covering more than
512 buckets uses a separate list queried at every point in that cell. All
influencing contributions retain FormID order and the existing radius, color,
negative-light and fade calculation. Runtime visibility overrides authored
initial-disable flags, so Enable can turn an initially disabled light on.
Loading reference state does not override the LIGH Off By Default flag.
That flag still suppresses the light estimate for an enabled reference.
Moved and spawned lights use their current shared placement.
`PlayerNode::detection_time_usec` measures the whole update, including actor
body synchronization, sampling and world evaluation. `detection_worldspace`
reports the active exterior worldspace's editor ID for the walking check.
Body synchronization and detection reuse one resident actor-placement list.
No per-reference scene nodes are added.

PRF-31 selects living actors in attached cells before resolving controller
height. A per-frame spatial pass keeps actors within detection range of another
actor, including NPC pairs away from the player, and observers near a queued
sound. Sound owners retain their placement even when distant. Current effective
enable state follows XESP parents. Detached and merely prefetched cells do not
supply host contact placements. The selection reads current World state each
frame, including moved and spawned references.
Exterior actors also need an attached grid at their current position in their
owning worldspace. Actors filed under the worldspace's persistent cell retain
that parent FormID, while distant placements outside the host grid are excluded.
`DetectionSpaces` shares pair neighborhoods across grid cells and the persistent
cell of one worldspace. Interiors and distinct worldspaces remain separate.
Light callbacks still receive each target's original cell, preserving its light
lookup. The walk requires observations in both exterior phases. Its controlled
sound uses `emit_detection_sound_at`, which queues a sound at one reference
owned by a separate actor through `World::create_detection_event`.

Body synchronization creates selected bodies and retains other eligible bodies
that already exist. It writes a transform only when the feet position changed
or a held/thrown body invalidated the last synchronized pose. Full contact
queries still resolve every eligible controller shape. Detection shares a
`ContactRays` object, direct space state and inverted actor-to-body exclusion
map for its rays. Switching observers replaces the exclusion array, while
repeated rays from the same observer reuse it. The local Godot references
`class_physicsrayqueryparameters3d.rst` and
`class_physicsdirectspacestate3d.rst` specify exclusion RIDs and first-hit results.
`detection_stage_times` reports body selection/sync, light construction,
observations and evaluation in microseconds. The outer timer also includes
settings, event filtering and frame bookkeeping.

The local CommonLibSSE-NG `include/RE/P/ProcessLists.h` distinguishes high,
middle and low actor processes. OpenMW `mwmechanics/actors.cpp` bounds AI work
by actor processing distance. This host instead preserves every eligible nearby
NPC pair and CK `CreateDetectionEvent` source ownership, using the existing
detection range and shared reference table rather than introducing another
actor process store. This adds no dependency or saved state.
A sound event uses its source position and owning actor FormID. Footsteps emit
a detection sound after successful playback. World keeps the stealth pool and
observer state across cell resource replacement.
`PlayerNode::detection_light_references` returns enabled placed LIGH FormIDs and
their Godot positions in the active cell for the live observation check. The
check toggles those references through the shared World and samples the same
actor pair to verify a placed light changes detection.

The estimate does not yet select a live WTHR from climate or apply WTHR
ambient, sunlight and image-space sunlight scale. The current sound path
covers `CreateDetectionEvent` and played player footsteps. The focused tests
exercise record light, occlusion inputs, cross-cell identity and sound source
ownership. The `SKYRIM_DATA` case reads a populated vanilla interior.

The light reader requires the full 92-byte CELL/XCLL structure and 48-byte
LIGH/DATA structure before using those fields. Inherited LGTM/DATA reads its
ambient prefix from the 24-byte layout recorded in TES5Edit, including the
legacy 24-byte template noted in `wbDefinitionsTES5.pas`. The layouts come
from `uesp-modfileformat/CELL.wikitext`,
`uesp-modfileformat/LIGH.wikitext`, and `TES5Edit/Core/wbDefinitionsTES5.pas`
in the local `engine-refs` corpus.

`godot/cmb_32_live_detection_test.gd` samples live resident actors in the
Bannered Mare, toggles enabled placed lights until one changes the shared
detection value, adds a PhysicsServer box between the nearest pair and checks
that sight lowers the value. It then emits an actor-owned sound and checks the
value rises. The same pair's awareness is checked after an unlocked door
changes cells. The test reports actor and ordered-pair counts, elapsed
time for 40 sync and detection samples, time per pair sample, and scene-node
count before and after. Actor body sync and those samples add zero scene nodes.
Run it with installed data and the host extension loaded:

```text
SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" godot --headless --path godot --script cmb_32_live_detection_test.gd
```

On Godot 4.7.2 with installed data, the Bannered Mare run sampled 7 actors and
42 ordered pairs. A wall changed one shared detection value from 33.497 to
-12.832, then an actor sound raised it to 38.581. The same observer and target
FormIDs retained awareness after the door changed cells. Forty sync and
observation samples took 19,580 microseconds, or 11.655 microseconds per pair
sample. The active scene had a node delta of zero through sampling and the
door transition. This times the shared host path in that loaded scene. It
does not estimate the memory or frame cost of a hypothetical node-per-actor
design.
The same check passed on Godot 4.4.1 with installed data: 7 actors and 42
ordered pairs, visible detection 40.985, blocked detection -12.832, and heard
detection 38.581. Forty samples took 21,529 microseconds, or 12.815
microseconds per pair sample. Awareness for the same FormIDs remained present
after the door changed cells, and scene-node count stayed unchanged.
Godot's `PhysicsDirectSpaceState3D.intersect_ray` returns the first hit RID.
The CMB-32 query maps that RID to the actor FormID or treats it as an
obstruction. The local Creation Kit `GetLightLevel` reference gives a 0 to
150 range. Local `LIGH` and `CELL` layouts define the light radius, flags,
fade and ambient/inheritance fields used by the current estimate. RAID's
lighting note says detection lighting differs from screen brightness, but it
does not define the world-light calculation. The implementation therefore
keeps the estimate separate from rendered brightness and does not infer
outdoor WTHR lighting.

## Cell ambience, AUD-09

The MUST track lookup and non-positional `SoundBank::play_ambience` entry
point remain available. `tests/aud_09_godot_ambience.rs` uses a generated WAV
fixture to check that entry point's AudioStreamPlayer start and release in a
windowed 640 by 360 process. PlayerNode uses the ambient SNDR descriptors
connected by AUD-11 below.

## Walkthrough audio, AUD-11

When gameplay starts, `PlayerNode` attaches its `SoundBank` before selecting
cell audio. Music follows CELL/XCMO, regional RDMO or WRLD/ZNAM through
`soe_game::music`. A destination without a selection stops the prior music.
Ambience follows CELL/XCAS, ASPC/SNAM/RDAT and CELL/XCLR regional descriptors
through `soe_game::ambience` and `sound::prepare_for`. Godot non-positional
players apply the selected descriptor's gain, pitch and ordinary loop mode.
Cell changes crossfade those players after the replacement decode completes.
Envelope loop modes return an unsupported playback error. The older MUST
ambience entry point remains available to callers.

Nearby Misc Hello and Idle responses selected by Game play at the speaker's
position through the existing SoundBank voice pool, without opening a menu.
Each response starts once. When its voice ends the host advances the shared
session, including INFO end fragments. Session interruption stops its voice.
Voice filenames follow local Mutagen `VoiceTypeAssetLookup.GetVoiceLineFilePaths`.
The playback and loop APIs follow local Godot AudioServer, AudioStreamPlayer3D
and AudioStreamWAV class documentation. No new per-reference node path is added.
Cell ambience owns one AudioStreamPlayer per selected descriptor.

`SOE_AUD_11_TEST=1` runs the installed-data walkthrough with a forward walk
for 20 seconds. The master-bus capture is drained throughout the walk and the
log reports footsteps, ambience, music and voices that started. The check
fails if any count is zero, movement is under 10 units or the captured peak
is at or below -60 dB. NPC speech is selected during normal gameplay ticks.
The windowed `aud_11_` tests launch this check and inspect its result and exit
status. The vanilla response test resolves and decodes the selected voice
asset independently. Checks skip when SKYRIM_DATA or SOE_GODOT_BIN is unset.

### Papyrus weather image-space selection, SCR-54

`Weather.SetActive` requests flow from `World` into `ImageSpaceStack` on the
next host sync. If no explicit image-space selection is active, the stack reads
the selected WTHR record's daytime `IMSP` slot and resolves its `IMGS` values.
The camera renderer applies those values through Godot's `Environment` and
`CameraAttributesPractical`. An explicit image-space selection takes
precedence. The image-space path uses the daytime slot at all game hours.
The RND-18 sky controller separately renders WTHR clouds, colors, ambient
lighting and transitions through the shared World request.

### Debug collision switch, SCR-21

`Debug.ToggleCollisions` changes `World::debug_collisions_enabled`. The host
applies that value to resident cell collision bodies and the player's query
bodies by setting their physics collision layer and mask to zero while off,
then restoring layer and mask one when on. Bodies synchronized after a switch
receive the current setting on the next host sync. Player movement still uses
the `soe-physics` `SimpleWorld` solver.

### Crosshair and held references, HST-18

`PlayerNode` raycasts from the eye along the camera look direction on each
advance and stores the first hit reference in the shared game world. The ray
uses the host physics space, including resident rigid clutter and static
obstructions. Game Papyrus globals `GetCurrentCrosshairRef` and
`GetPlayerGrabbedRef` return those current host values. HST-22 supplies the
held-Activate path below. The grabbed reference is cleared when the cell is
replaced. These values are transient and are not saved.
The local CK `Game_Script` page documents both getters and their reference
results.

`hst_18_installed_host_crosshair_grab_and_papyrus_getters_track_each_frame`
uses the installed Bannered Mare tankard to check the live ray, the held
physics reference, release, and both Papyrus getters through `PlayerNode`.
`debug_reference_native` invokes only these two Game getters and reports the
resolved reference identity for host diagnostics.

The host also sends the active first-person POV to Game each camera-control
sync. CTDA function 391 reads this value, and function 463 compares its REFR
parameter with the current grabbed reference.

### Carrying loose objects, HST-22

A mapped Activate press captures the reference under the crosshair. A release
before 0.35 seconds performs ordinary activation through Game, including item
pickup and object use. A hold reaching that threshold on the same reference
starts carrying only if its resident body is rigid. Fixed and keyframed
collision remains static, and a Papyrus motion override that makes a movable
body static or kinematic prevents carrying it. A completed hold does not
activate on release. Telekinesis retains its separate input path.

The carry target stays at the captured eye-ray distance in front of the
camera. The body remains rigid with its existing collision layer, mask,
gravity and velocity. Each fixed step applies a central impulse from a damped
spring between the hit anchor and that target. The spring uses acceleration
100 times displacement minus 20 times velocity, compensates the body's current
host gravity, bounds acceleration to 80 metres per second squared and bounds
its impulse timestep to 1/30 second. Release stops applying impulses and keeps
the body's linear and angular momentum. Jolt continues to resolve collisions
while held and after the drop. Simulated poses reach the existing World
reference fields and the renderer through HST-12's sync path.

Successful grabs and releases queue `OnGrab()` and `OnRelease()` on the
reference with no arguments. Focus loss, an open menu, disabled activation
controls, removal of the body, a cell replacement and player exit clear the
hold. `carry_status` exposes the shared crosshair and grabbed identities for
host diagnostics. No saved field, dependency or per-reference node is added.

Sources read were local `nifxml/nif.xml`'s `hkMotionType`, CK
`SetMotionType_-_ObjectReference.wikitext`, `OnGrab_-_ObjectReference.wikitext`
and `OnRelease_-_ObjectReference.wikitext`, Godot's PhysicsServer3D and direct
body-state class references, and godotwind's
`src/core/interaction/carry_controller.gd`. The NIF decoder and resident
body builder already supply dynamic clutter. Godotwind uses node wrappers
and a velocity drive. This host uses its existing RIDs and a spring impulse
so contacts remain solver-controlled. The named CK Controls and UESP Skyrim
Controls pages are absent from the local corpus, and their web endpoints
refused retrieval. The task's held-Activate and release contract supplies the
input behavior.

The `hst_22_` tests cover tap and hold timing, interrupted or changed aim,
spring forces and invalid inputs, fixed-container use, zero-argument reference
events, dynamic versus static collision, and an installed Bannered Mare tankard. The windowed
scene uses the mapped E input, checks collision against a server obstacle,
carries the tankard two metres, releases with momentum, checks table or floor
contact and settling, compares World and rendering poses, checks focus-loss
release, then taps to pick up the dropped item. It captures
`target/hst-22-carry.png`, writes `target/hst-22-scene.log` and quits.

### Destruction-stage models, CMB-37

When a resident reference has a selected destruction-stage model in the
shared `World`, `CellStreamerNode` hides its base geometry and loads the
selected DMDL through the same VFS, NIF, mesh and material path as streamed
cell geometry. The replacement uses RenderingServer instances and adds no
reference node. Advancing to a model-less stage retains the latest selected
DMDL through `World::destruction_model`. Clearing destruction removes the
replacement and reveals the base model. Releasing the owning cell or clearing
the streamer frees its replacement mesh and instance RIDs.

The `cmb-37-test` feature adds a regression driver that selects an installed
modeled REFR, damages its DEST stages in a World, and runs the same
RenderingServer overlay sync used for resident references.
`cmb_37_windowed_reference_damage_replaces_its_model` checks that damage
selects a stage and that the host creates replacement instances for its DMDL.
The format fields follow local xEdit `wbDefinitionsTES5.pas`. CK's
`DamageObject` page states that it applies damage and may fire stage events.

### Character race and sex selection, UI-07

`PlayerNode::show_race_menu` opens the native Control menu for race and sex
selection. `Game.ShowRaceMenu` queues a transient host request that the player
consumes on its next gameplay advance. The list comes from winning, non-deleted
`RACE` records whose `DATA` playable flag is set and whose required attributes
are valid, ordered by resolved FormID.
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
The host processes this request before the race menu request from the same
Papyrus call sequence. The streamer retains each available path once and
reports retained file count, NIF count, TRI count, byte total and paths missing
from the active data VFS. `Game.PrecacheCharGenClear` drops the buffers and
resets residency and byte counts. The asset buffers are CPU-side file data.
This does not upload preview meshes or apply morphs to a rendered actor.

The SCR-30 cache tests check retained file and byte counts, clear every cache
counter after a cache load, and load installed head-part models and
morph files when `SKYRIM_DATA` is set. The headless
`godot/ui_17_chargen_cache_test.gd` check invokes both Game natives through the
player request path and reads the attached streamer's file, model, morph,
missing-path and byte counters before and after the requests. It opens the race
menu after precaching and confirms the assets remain resident. The training
presentation test checks the offer values used for the trainer heading and
purchase button, including an unavailable trainer. The camera visibility test
checks the value passed to the rendered actor instances for first and third
person. These Rust checks do not inspect pixels from a live player appearance.

The CK `engine-refs/ck-wiki/Game_Script.wikitext` defines the paired cache and
clear semantics. xEdit's `wbRecord(HDPT)` and UESP's `HDPT.wikitext` define the
model field and the NAM0 chargen TRI path.

`godot/ui_07_race_menu_test.gd` opens the live menu, presses a different race
record and sex button, confirms through the connected Control callbacks, then
reopens the menu and reads both selections back from the player state. It
passed headlessly on Godot 4.7.2 with installed Skyrim data. The Rust tests
cover native request queuing, playable race validation, one-time actor-value
initialization, and the resulting player actor state.

`Game.ShowTrainingMenu(actor)` reaches the host as a typed menu request with
the actor form. Valid trainers also reach the UI-16 presenter through its
pending request. The host opens the training menu once and consumes the typed
request without presenting it again. The menu heading and purchase label are
built from the current offer returned by `training_menu::entry`, while an
invalid offer displays the unavailable label. Camera-target and first-person
geometry requests are applied through transient camera controls. The first-
person visibility flag reaches each player appearance RenderingServer
instance only in first-person view.

Actor `ShowBarterMenu` requests open a native Control panel titled for the
merchant and list stock from the first resolvable vendor faction. An unavailable
service or an empty stock list displays an empty-state label. Actor
`OpenInventory` requests open the existing container inventory panel for the
requested actor, where the player can take items through the notified inventory
transaction. These requests are consumed from the shared world's transient
menu queue.

`godot/scr_13_actor_menu_host_test.gd` starts the installed Bannered Mare
runtime, invokes Hulda's `ShowBarterMenu` and forced `OpenInventory` through
Papyrus, and checks that each request opens the native item presenter with its
barter or container mode. Its check-only PlayerNode bridge accepts those two
native names and a placed actor editor ID. An empty editor ID selects the player.

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

### Shader particle geometry, SCR-46

`PlayerNode` sends `World::shader_particle_layers` to `CellStreamerNode` each
gameplay frame, centered on the player's placed reference. The streamer uses
one `MultiMesh` RenderingServer instance for each active SPGD form, with a
quad per particle and the record's texture when it resolves through the data
source. Density determines the quad count at 64 per density unit, bounded to
256, and zero density draws none. Deterministic positions fill the authored
box and wrap downward using gravity velocity. Fade strength sets material
opacity. Removed layers and streamer teardown free their instance RIDs.

This host path is an approximate visual presentation. It does not reproduce
engine particle randomness, rotation velocity, initial rotation, or subtexture
animation.

### Cart and horse host tether

`ObjectReference.TetherToHorse` queues a cart and horse FormID after applying
the CK local offset and copying cart placement to the horse. During reference
sync, the host retains that request until both collision bodies are resident,
then connects them with a PhysicsServer pin joint. The pivot coordinates use
the CK cart chassis pivot `(0, 3.5, -0.2)` and horse pivot `(0, 0, -0.5)` in
Havok axes, mapped to Godot metres. `CellStreamerNode::tether_bodies` also
accepts caller-supplied body RIDs and pivots for direct host use. Links are
keyed by both resolved FormIDs. Replacing a link for either endpoint frees the
old joint. Missing or disabled endpoints release a live link, and `clear`,
collision-cell release or node teardown frees joints and pending pairs. The
host does not sample the `HorseSpine2` skeleton bone, build the wheel and pole
constraints, or reproduce the cart suspension and upright recovery action.

### Editor forms dock, EDT-02

`godot/addons/soe_forms` is a Godot editor plugin. It opens plugin paths in load-order order
and names the active plugin, then filters winning forms by an optional four-character
signature and case-insensitive Editor ID substring. `SoeFormTable.search` returns encoded
FormIDs only, with at most 200 results per page. Selecting a row calls `selected_form`, which
creates one `SoeFormResource` for the inspector with its resolved FormID, signature, Editor
ID and winning plugin. The same selection calls `override_chain`, which returns every record
version with its source plugin, signature, Editor ID and winner flag. The dock displays those
versions in its override-chain tree, with the last load-order version marked as the winner.
The resource is a view and does not expose record editing.

The dock uses Godot's `EditorPlugin` dock API. Its registered `EditorInspectorPlugin`
recognizes `SoeFormResource` and adds a `Winning source` row from the wrapper's
`winning_plugin` property. Local
`engine-refs/godot-docs/tutorials/plugins/editor/making_plugins.rst`,
`inspector_plugins.rst` and `classes/class_editorinspectorplugin.rst` describe these host APIs.
The plugin is enabled in `godot/project.godot` and requires the built GDExtension. It displays
record identity and winner metadata. The wrapper remains a view and does not edit records.
Typed record editing is provided by the editor document API. The inspector addition does not
provide a custom field editor for record editing.

`crates/godot-host/tests/edt_02_godot_forms_dock.rs` launches the headless editor with a
synthetic load order. It checks that the dock tree shows the case-insensitive `STAT`
matches from the winning records, that searching created zero `SoeFormResource` values, and
that selecting one row creates one resource. Run it after building the release extension
with `SOE_GODOT_BIN` set to Godot 4.4 or newer. The runner reads the editor controls without
rendering pixels, and the plugin exits the editor when the check finishes.

### Drop selected references to host collision, EDT-15

`SoeFormTable.drop_selected_reference_to_ground(space, encoded_id)` casts a vertical ray
through Godot's direct physics space at the selected REFR's horizontal position. The ray
starts 100 Skyrim units above the reference and extends 10,000 units below it. A hit is
converted from Godot metres and axes to Skyrim world coordinates. The editor updates only
the reference's vertical transform value through `EditorDocument`, preserving horizontal
position and rotation. The active-plugin edit is undoable through `SoeFormTable.undo()`.
Malformed selections, unavailable physics spaces and rays without a hit return `false` and
set `last_error()`. `query_ground_collision` exposes the same host query as a position
inspection for callers that need to preview a surface hit.

The ray query uses Godot's `PhysicsDirectSpaceState3D.intersect_ray` behavior described in
local `engine-refs/godot-docs/classes/class_physicsdirectspacestate3d.rst`. REFR `DATA` is
the six-float position and rotation layout from local xEdit's
`TES5Edit/Core/wbDefinitionsTES5.pas` and UESP's `uesp-modfileformat/REFR.wikitext`.
`godot/edt_15_drop_to_ground_test.gd` creates a server-side box collider and checks the
host query hit and Skyrim-space surface height without game data.

`DataSource` parses the five base masters through `soe_esm::read_plugin_cached`.
By default the entries live under the user cache directory
(`$XDG_CACHE_HOME/skyrim-open-engine/plugins`, or `$HOME/.cache` when the XDG
variable is unset). `SOE_PLUGIN_CACHE` selects another directory, and the value
`off` disables disk caching. The explicit cache argument on
`open_with_threads_and_cache` and `open_loadout_with_threads_and_cache` is used
by the cold and warm `soe-bench open-data` measurements.

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

For AI-65, each gameplay input frame passes active sprint intent to the shared
player actor state. The signal is true only while moving with sprint held and
is false while standing, after sprint release or while sneaking. This supplies
the transient state read by CTDA `IsSprinting` (568). The input intent test
`ai_65_sprint_signal_tracks_movement_and_sneak_state` covers these cases.

`godot/hst_12_movable_test.gd` uses the installed Bannered Mare cell, applies
an impulse to an authored movable reference, then checks World and rendering
pose agreement and the disable, despawn and unload cleanup paths.

`CellBodies::apply_reference_physics_requests` drains the transient requests
from `ObjectReference.SetMotionType` and `ApplyHavokImpulse` after placement
sync. It applies motion modes and central impulses to resident movable bodies.
Requests for references without a resident movable body are discarded. The
Papyrus request queue and this host operation do not add saved state.

`CellBodies::grab_movable_reference` changes one resident movable body to
kinematic mode. `move_grabbed_reference` updates its PhysicsServer3D transform
from a finite Skyrim-space position. `release_movable_reference` returns the
body to rigid simulation at that pose. Each `CellBodies` collection permits
one grabbed reference at a time. PlayerNode routes an Activate press during a
Telekinesis concentration cast to the resident movable body with the smallest
eye-ray offset, breaking ties by distance, whose center is within 48 Skyrim
units of the eye ray and within the 180-unit reach. It confirms the World grab
before changing the host body, moves the body to a
fixed eye-relative distance on each physics step, and releases it when World
clears the grab. A successful grab and release queue the reference's
`OnGrab` and `OnRelease` events. Losing input focus ends both hand casts and
immediately releases any host body held by Telekinesis.

`CellStreamerNode` exposes `telekinesis_grab_reference`,
`telekinesis_move_reference` and `telekinesis_release_reference` for host
integration checks. The move call accepts a position in Skyrim units. The
release call restores rigid simulation at the body's current pose.
`godot/mag_41_telekinesis_host_test.gd` uses the Bannered Mare's installed
movable-reference body to check host movement and resumed rigid simulation
after the release path.

### Grab Actor, MAG-66

After actor bodies sync from World, `PlayerNode` finds a resident actor with a
live Grab Actor effect cast by the player. The existing RID-backed actor
capsule changes to kinematic mode and follows a point 1.2 metres ahead of and
0.9 metres below the camera eye. Each held pose is written back to the actor's
existing World reference placement. When the effect ends, the body changes to
rigid mode and receives a forward and upward central impulse. World follows the
rigid body's Skyrim-space position until it settles, then the body returns to
static contact-query mode. Clearing or replacing the cell releases its bodies
and drops any hold state. This uses the actor body's existing RID and adds no
scene node or saved field.

### Papyrus Debug host requests, SCR-20

`PlayerNode::advance_game` drains the transient `World::debug_host_requests`
queue. Quit requests call Godot's `SceneTree.quit`. Cell-center requests move
the player to the destination CELL origin, run the normal World cell
transition, and pass the new placement through `sync_game_pose` so the shared
streamer replaces cell rendering and collision. A `CenterOnCellAndWait`
latent id is resolved after that sync succeeds. The Game crate owns name
resolution and rejects unknown CELL editor ids before queuing a request.

### Debug reference presentation and movement, SCR-23

`PlayerNode` displays each `Debug.ShowRefPosition` request as a small emissive
sphere at that reference's current world placement. The sphere mesh and
material are shared. The host stores one `RenderingServer` instance RID per
requested reference and frees the instances when the player node exits.
`Debug.PlayerMoveToAndWait` requests are applied to the shared World before
`sync_game_pose`. The latent Papyrus call resolves with 0 after the host has
installed the destination, or -1 if the move cannot be applied or installed.
`Debug.SetFootIK` is retained in World, but actor rendering does not yet apply
foot inverse kinematics.

`Game.ShakeController` queues a transient controller-shake request. The
player adapter sends its left and right strengths and duration to every
connected joypad through Godot's `Input.start_joy_vibration`.

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
| `detect_life` | no | active Detect Life targets as authored-shader presentation requests in Godot coordinates |
| `input` | no | the vanilla `controlmap.txt`, key bindings, and a frame of input into movement intent |
| `godot_layer` | yes | the `CellStreamer` node, `RenderingServer` instances, `array_mesh_from`, `heightmap_image_from` |
| `godot_player` | yes | the `PlayerNode`: devices in, physics character and camera out |

The Godot layer decides nothing. That is what keeps the tests engine-free, and it is the
seam a VulkanSceneGraph or other host would reuse: everything above the layer is plain data.

## Detect Life presentation, MAG-40

`detect_life::presentations` reads the selected targets from the shared game
world and converts each Skyrim position to Godot metres and axes. The request
retains the winning MGEF Hit Shader FormID and active effect identity so a host
effect renderer can identify the authored shader and remove it when the effect
ends. `PlayerNode` reconciles those targets each gameplay step with the existing
RenderingServer EffectShader path. A newly detected actor starts its MGEF Hit
Shader on that actor, and a target leaving the set stops the matching shader.
The host requests a visual only when the resolved form is a winning `EFSH`
record, so a missing or differently typed shader cannot become a default
white sphere.
The renderer reads EFSH `ICON`, color and blend settings, applies its membrane
fade timing, and uses an authored fill texture when it resolves. It draws a
server-side sphere instance and creates no per-actor scene nodes. It does not
render the EFSH particle or gradient groups. The EFSH fields follow local xEdit
`wbDefinitionsTES5.pas` and UESP `EFSH.wikitext`. The CK dump has no Detect Life
archetype page defining additional execution semantics. The focused check
`mag_40_host_presents_selected_actor_with_authored_shader_in_godot_coordinates`
also checks the start and stop requests and authored membrane settings.

`facegen::build_head_part_surface` takes one already resolved head-part NIF mesh, a parsed
`FRTRI003` file and named morph weights. It applies the deltas to a fresh mesh copy and
passes the changed geometry through the same axis conversion, bounds and surface validation
as streamed meshes. Normals are generated from the changed geometry. The caller resolves
the NPC's `PNAM` head-part links and maps `NAM9` values to morph names.

### Actor expression targets, ANM-59

Animated actor head surfaces load a companion `.tri` file when its vertex count matches the
head NIF surface. CK expression names in that file become Godot relative blend shapes
with converted vertex and normal deltas. Each actor update reads the effective expression
from the shared game world and sets the matching blend-shape weight to its strength divided
by 100, clearing the other expression targets. Clearing a Papyrus override therefore lets
the actor's current record mood drive the same head targets again. Other TRI morph names and
head surfaces without a matching TRI remain unchanged.

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
after each gameplay tick. It opens the native race editor for `ShowRaceMenu`,
shows a red screen overlay for `TriggerScreenBlood`, and emits the matching
`race_menu_requested` and `screen_blood_requested(value)` signals. Blood
intensity is the absolute native value clamped to 100 percent, and the overlay
lasts 450 milliseconds. The HUD shows a `CART` badge while the shared
`World::hud_cart_mode` flag is set and emits `hud_cart_mode_changed(enabled)`
when that value changes.

`PlayBink` preserves its file name and four playback options in the host
`bink_requested` signal. The Godot `VideoStreamPlayer` documented in local
`engine-refs/godot-docs/classes/class_videostreamplayer.rst` supports Ogg
Theora or a video GDExtension decoder. This host has no Bink decoder or video
presenter, so `.bik` playback remains unsupported. The local CK
`engine-refs/ck-wiki/Game_Script.wikitext` supplies the five Game native
signatures and describes `PlayBink` as Bink playback.

`godot/scr_29_game_ui_test.gd` checks the visible race menu and HUD presenter
controls against installed Skyrim data. Rust host tests check cart-mode state
and the screen blood overlay's intensity and lifetime.

## EffectShader playback, MAG-14

`CellStreamerNode` consumes transient EffectShader requests from the shared
game World. It keys active visuals by `(EFSH, target reference)`, so a second
Play refreshes the existing RenderingServer instance. Stop frees it, finite
durations free it after elapsed host time, and leaving the tree frees every
remaining instance. Each active effect owns one sphere mesh instance and one
material, with no scene node. The membrane uses the EFSH fill texture through
the shared data-source DDS cache, the first authored fill color, a mapped
source/destination blend mode, and the fill alpha fade and persistent ratio.
Particle emission, palette and edge texture effects, skin-only filtering, and
attachment to skinned surfaces are not implemented. The local reference
corpus includes UESP's EFSH field list and xEdit's record layout. Its CK wiki
corpus has no EffectShader page. The synthetic tests verify request data and
host fade/blend selection. Live rendered output has not been verified.

## Spell art preload, SCR-47

The player drains `Spell.Preload` and `Spell.Unload` requests from `World` on
its presentation update and passes them to the shared cell streamer. Preload
walks a spell's ordered effects, reads each winning MGEF DATA record, and finds
the casting art, hit art, projectile and explosion forms. Their `MODL` paths
are loaded through the game's VFS and the existing NIF mesh and material
pipeline. Projectile records also contribute `NAM1`, the muzzle flash model
filename from local UESP `PROJ.wikitext`. The streamer retains each generated
mesh per spell. Its material
owns the decoded textures for the same lifetime. Unload drops that spell's
retained meshes and their materials. Repeated preload requests do not create
duplicate retained entries.

The linked MGEF fields use offsets from the local UESP `MGEF.wikitext` DATA
layout. `godot/scr_47_spell_art_host_test.gd` calls the shipped Flames spell's
Papyrus methods and checks that the host retains then releases model meshes.

## Papyrus visual effects, SCR-52

`PlayerNode` advances active `World::visual_effect_instances` into its shared
`CellStreamerNode`. The streamer creates one `RenderingServer` sphere instance
per active `VisualEffect.Play` call, keys it by the game-owned call identity,
and follows the target reference's current placement. Expiry, `Stop`, target
disable and node clear release the render instance. If the linked EFSH record
has an `ICON` start texture, it is loaded through the shared DDS cache. A
shader-linked effect uses a blue tint, while an ARTO-only effect uses amber.
This is a visible host placeholder, not the authored ARTO NIF or full EFSH
animation. The placeholder follows RFCT attach-to-camera and inherit-rotation
flags, and rotates toward the optional facing reference when the RFCT flag is
set. It does not reproduce authored ARTO animation or EFSH timing.

The rendering resource uses server IDs and creates no scene node per effect.
`godot/scr_52_visual_effect_test.gd` starts an installed game, picks a winning
RFCT with a linked art or shader, captures the scene before and after playing
it on the player reference, verifies changed pixels and one renderer instance,
then stops it and checks the instance is freed. Run windowed with
`SKYRIM_DATA` and the extension loaded:

```text
SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" godot --path godot --script scr_52_visual_effect_test.gd --resolution 640x360
```

The source layouts are UESP `RFCT.wikitext`, `ARTO.wikitext` and `EFSH.wikitext`,
and xEdit `TES5Edit/Core/wbDefinitionsTES5.pas`. The local CK corpus has no
VisualEffect function page or shipped `VisualEffect.psc`. The existing game
tests cover native argument handling, errors, stacking, expiry and target
specific stop. The Godot capture verifies instance creation and release, not
fidelity to Skyrim's complete authored effect appearance.

## Invisibility rendering, MAG-34

`SkinnedActorNode` reads invisibility from the actor's shared World state and
sets visibility on every RenderingServer mesh instance. The initial appearance
bind reads that state before creating instances, and later appearance rebuilds
reuse the same cached state. Animated and ragdoll poses share these mesh
instances, so the visibility change applies in either render mode. A later
process update restores visibility after the effect expires or breaks. The
game tests cover detection and break actions, while host unit tests cover the
RenderingServer visibility mapping. `godot/mag_34_invisibility_test.gd` binds
the player appearance, casts the installed Invisibility spell, and checks the
host visibility state before and after expiry. Run it with `SOE_GODOT_BIN`,
`SKYRIM_DATA`, and the built extension:

```text
"$SOE_GODOT_BIN" --path godot --script mag_34_invisibility_test.gd --resolution 640x360
```

This live check covers the supported `SkinnedActorNode` path. Other actor
rendering paths are not implemented by the host.

## Runtime summoned lights, MAG-35

Active magic effects that place a runtime reference with an LIGH base also
create a point light through `RenderingServer` RIDs. The host reads the
winning LIGH record's 48-byte DATA, using its RGB color, radius and negative
light flag. The light follows the runtime reference position and is freed
when the effect reference expires, becomes hidden or the streamer is cleared.
It adds no scene node. Other LIGH references are not uploaded by this path.
Each positive `PlayerNode.advance` step ticks the shared `Game` before syncing
reference changes, so timed effects expire in that host path and their light
instances are released on the same synchronization pass.
`CellStreamerNode.runtime_light_count` reports the currently owned light
`PlayerNode.cast_spell_on_self` sends a self-delivered spell through the
shared World. `mag_35_runtime_light_test.gd` casts vanilla Candlelight in a
loaded cell. It checks that the RenderingServer instance appears and then
disappears after the effect's authored duration. Run it with `SOE_GODOT_BIN`,
`SKYRIM_DATA`, and the built extension:

```text
"$SOE_GODOT_BIN" --path godot --script mag_35_runtime_light_test.gd --resolution 640x360
```

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
both sequential and threaded runs. TST-02 compares threaded surfaces with
serial results for the same inputs, then hashes their arrays and bounds into
the matching game's per-tick `host.builders` partition. Worker output
therefore participates in the same divergence report as world and VM state.

## Streaming

### Vanilla save recovery sessions, SAV-61

`save_recovery::accept_vanilla_recovery_session` performs a read-only
dependency preflight before restoring a vanilla ESS. Missing plugins, missing
scripts and changed regular/light plugin kinds return the report and refuse
session start. ESS provenance that is unavailable in the format remains
reported as unknown. Unimported PMIS, PBEA, PFLA and PBAR changes and
opaque PCON remainders also refuse execution. PARW placed arrows, PHZD placed
hazards and PGRE grenade projectiles are classified as transient for
accepted recovery. They are filtered from a private copy of the import input,
omitted, with per-type counts returned in `AcceptedVanillaSession.skipped_transient`
and logged after the generation is written. This policy does not restore
unexpired arrows, grenade projectiles or active hazards. Their payloads remain
in the original ESS. Dependency checks still inspect the complete source save.
Other supported restoration errors return before a recovery
generation is written.

After restoration succeeds, the host writes a native generation to the
caller-provided recovery store. The operation does not open or replace the
source ESS path. It does not claim that every vanilla change-form type or
saved Papyrus behavior can be resumed. A caller has to opt into this operation
after reviewing its returned dependency report.

### Attaching a restored player, SAV-19

`PlayerNode::attach_restored_game` accepts a `Game` after its vanilla-save
restoration has succeeded. It requires shared data and a live cell streamer,
checks that the player has a cell and finite position and rotation, then uses
`sync_game_pose` to load the restored location, collision and camera state. It
advances one 1/60 second host step and reports success only while the game,
character, cell and camera remain attached. The caller should retain its active
`Game` until this call succeeds.

The UESP `Player_Location.wikitext` describes a world or cell, exterior cell
coordinates and a position. FallrimTools' ESS player-location handling keeps
that data separate from the player's ACHR change form. xEdit's
`wbDefinitionsTES5Saves.pas` leaves PHZD, PGRE and PARW subtype payloads empty,
and FallrimTools falls back to opaque bytes for these types. Recovery uses the
transient policy above rather than deriving an undocumented lifetime reader.
CommonLib's `GrenadeProjectile.h` supplies the PGRE projectile identity, and
`Projectile.h` separates its living time from reference placement.
No new parser, dependency or per-reference scene node is added.

`sav_19_vanilla_atomic_import_attaches_saved_cell_pose_camera_and_verified_frame`
is gated on `SKYRIM_DATA` and `SOE_TEST_SAVES`. It attempts recovery across the
external save corpus and writes each result to `target/sav_19/survey.json`.
Refused imports must preserve the prior game snapshot. Every source ESS is
checked for unchanged bytes. At least one Clean Save must recover. Its player
cell, position and rotation are compared with ESS player-location and ACHR data.
The test passes the resulting native generation to
`godot/sav_19_vanilla_attach_test.gd` when `SOE_GODOT_BIN` is set. That windowed
check uses `attach_restored_generation`, verifies the saved pose and current
camera, checks that the game clock advanced, and captures a 640 by 360 frame.
At least 100 sampled pixels must differ from the background by 0.05 in RGB
distance. The script writes `target/sav_19/frame.png` and quits. Recovery
refusals and host output remain in the survey and `target/sav_19/host.log`.

The 2026-10-08 survey attempted all 60 external saves and accepted 57,
including 18 of the 19 Clean Saves. Three refused. One Clean Save
(After The Throat of the World) carries an opaque PCON change form, which still
refuses by policy. Saves 40 and 41 of the Dawnguard and Dragonborn set refuse
because the saved body of `DLC2dunBookLevel4Bend.OnTriggerEnter` differs from
the loaded PEX, the next thing to diagnose by dumping the first differing
instruction as was done for the other mismatches.

Getting from none to 57 took these restore changes, each tied to a reader:

- PMIS, PARW, PHZD and PGRE changes are left out with per-type counts in
  `skipped_transient`, and the source ESS keeps them.
- A Papyrus instance on a created form with no change form restores as a
  standalone object (FallrimTools reports these as scripts some mods create),
  and further instances of one script on one form, saved once per
  item stack, each get their own object.
- A frame with a Null owner is a global function frame, so it runs on no
  object.
- A suspended caller whose pointer names the call instruction itself, as a
  frame blocked in `Utility.Wait` is saved, resumes after that call.
- The frame's event field is the function name, and it is looked up through
  the script's parents.
- Saved code compares names without case and a string operand stands for an
  identifier, because the ESS string table folds case. A native frame names its
  arguments `param1`, `param2` by position. A handler that changed state is
  matched against the same-named function of any state.
- A FACT change form carries a 20 byte `CRVA` plugin struct as well as the 16
  byte counter form.
- The player's cell comes from the saved `coorX` and `coorY` when present.

The survey keeps every per-save reason in `target/sav_19/survey.json` and
verifies rollback and source-byte preservation. The vanilla attachment test
`sav_19_vanilla_attach` passed on the 2026-10-08 run, 850 s, with the
extension built from this branch and imported on Godot 4.4.1. It captured the
verified frame for a Clean Save at its saved cell, pose and camera. The play
session check by the user remains open.

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
The viewport camera frames the rendered bounds after selection. The installed-data host
check `godot/edt_03_cell_scene_test.gd` opens two populated interiors in sequence and
checks that the second selection replaces the first cell's render instances.

This milestone supplies a read-only cell tree and rendered cell preview. Reference transform
editing is provided by the separate `addons/soe_reference_gizmo` dock described below. The
Godot project opens the cell view only after the author enters a valid `Data` directory or
launches the editor with `SKYRIM_DATA` set.

### Selected reference transform gizmo, EDT-14

The `addons/soe_reference_gizmo` editor dock opens plugin paths through
`SoeReferenceTransformEditor`, takes a resolved `file:object` FormID, and adds one
`SoeReferenceGizmoTarget` proxy Node3D to the edited scene. Godot's registered
`EditorNode3DGizmoPlugin` attaches a draggable handle to that selected proxy. Its commit
callback calls `commit_gizmo_transform`, which forwards position and rotation to
`SoeReferenceTransformEditor::set_reference_transform` and then to
`EditorDocument::set_reference_transform`. The record remains the source used for plugin
output. Undo calls the document edit log and reloads the proxy pose from the winning record.
The Save plugin button writes the active plugin. This creates one proxy node for the selected
reference only. Ordinary cell rendering remains RID-backed.

Godot's local `tutorials/plugins/editor/3d_gizmos.rst` and
`classes/class_editornode3dgizmoplugin.rst` specify the handle callbacks and restore value
used by this adapter. `godot/edt_14_gizmo_test.gd` exercises the gizmo callback adapter and
proxy update in Godot. The editor crate tests verify the record operation, undo, and saved
`REFR` placement. Reference selection from the cell tree and automatic replacement of the
streamed cell preview after a placement edit are not implemented by this milestone.

`PlayerNode` starts at most `soe_game::EXTERIOR_CELL_LOADS_PER_UPDATE` exterior cell loads
per update. It reports the successfully entered cell count to the shared `Game` and
Papyrus budget usage. Papyrus reads the latest count through
`Utility.GetCurrentBudget` and compares it with the configured cap through
`OverBudget`. The counter resets to zero at the start of each host update.

The direct gameplay adapter publishes exterior cell arrivals and departures
to `Game::update_streaming_cells` after `CellStreamerNode` applies the resource
delta. Initial neighboring cells use `Game::attach_streaming_cells`, and the
old exterior ring uses `Game::detach_streaming_cells` when its host resources
leave. The game derives reference 3D residency from those cell IDs, so a cell
prefetch alone does not report loaded 3D. `ObjectReference.Is3DLoaded` then
reports the residency state, including references culled by room bounds, until
their cell leaves the streamer.

`stream_cell_id` walks the cell index, which keeps a placed object whose winning
record carries the deleted flag out of the drawn list and files it under
`deleted_references` instead (`docs/esm.md`, "Dirty plugins"). A deleted
reference still resolves through the load order and the game starts it disabled,
so the host never draws one and never has to test for it.

`StreamPlan` is the unit the host schedules. `update_with_budget` caps its selected loads by
triangle and byte totals when the caller supplies a `StreamCost` for each candidate cell.
Costs may be measured or estimated. Loads stay nearest first, and a cell that does not fit
either remaining limit is deferred while later smaller cells may be selected. A zero limit
is unlimited. The ordinary `update` keeps its cell-count behavior and does not apply render
budgets. `SurfaceArrays` exposes triangle counts and its vertex and index buffers, and
`PreparedCell::stream_cost` adds payload bytes for textures its materials will use. It counts
a texture path once within a cell, including every cubemap face, while shared paths in
different candidate cells are counted again as a conservative estimate. The walking player
path calls `update_with_budget` when its exported `exterior_triangle_budget` or
`exterior_byte_budget` is positive. The limits apply to selected mesh and texture uploads.
Preparation may decode more candidates on that tick so a smaller later cell can fit. Zero on
both exported settings leaves the earlier cell-count behavior.

The nearest-first planning follows the cell ordering in OpenMW's
`apps/openmw/mwworld/scene.cpp` `sortCellsToLoad`. The host keeps its deterministic
distance ordering and adds the per-tick work caps described above.

`CellStreamerNode::stream_object_lod` reads and draws existing BTO object LOD
blocks from `meshes/terrain/<world>/Objects`. Archived paths supply available
levels and block coordinates. It uses the same capped distance streamer and
byte budget as BTR terrain LOD. Blocks inside the full resolution radius are not
requested. Each rendered block remains keyed by its worldspace and
`LodBlockCoord`, and release or budget eviction frees its instances. Use
`update_object_lod` to move the BTO streamer as the focus changes, and
`clear_object_lod` to release it. BTO surfaces retain the worldspace form as
their source identity because the merged file does not encode individual
placed-reference identities.

`terrain_lod_stats` and `object_lod_stats` report each stream's resident
block coordinates in the `blocks` field as space-separated
`level:x:y` entries, alongside drawn instance and triangle counts. The live
RND-10 renderer check streams a radius-2 exterior area and real BTR and BTO
blocks in the same scenario, then checks terrain's loaded-cell mask and that
object LOD block squares do not cover any coordinate in that near area. This exercises the handoff through
`RenderingServer` instances without treating BTO surfaces as individual
placed references.

`PlayerNode` starts terrain and object LOD when its gameplay focus enters an
exterior worldspace. It gets the asset folder from the winning worldspace
record, following PNAM's Use LOD Data bit through parent worlds. Use Land
Data does not select LOD assets. Missing parents and cycles return no LOD
identity. xEdit `Core/wbDefinitionsTES5.pas` defines the two independent flags,
and `Core/wbLOD.pas` supplies block alignment and settings. The NIF block
layouts are documented by local `nifxml/nif.xml`.

The default LOD radius is 96 cells. The exported full-resolution exclusion
radius is clamped to at least the near streamer's two-cell unload radius,
covering cells retained by hysteresis. Both radii and an approximate byte
budget per LOD stream are exported on the player. Each player update advances
both LOD streams at the current cell coordinate. Object blocks overlapping
the full-resolution exclusion are released immediately, ahead of near uploads,
while distant releases retain their per-update quota. A worldspace change
starts terrain, object and tree streams for the destination world. Leaving
exterior gameplay clears all three streams. Object and tree residency operate
on whole blocks. Terrain residency retains whole BTR blocks, while handoff
masks individual loaded LAND cells within resident blocks.
`set_terrain_lod_visible` toggles resident terrain instances without changing
near geometry or object LOD, for capture comparisons. The gate diagnostic
returns both ends of its teleport link, allowing the check to move from
authored references in each worldspace.

`godot/rnd_19_player_lod_capture.gd` enters WhiterunWorld through the Bannered
Mare door and looks south from the city gate toward the mountains. It captures
the same frozen view with terrain LOD hidden and visible, requiring at least
500 terrain-contributed pixels in the middle third of the image. Captures go
to `godot/.godot/rnd-19-whiterun-exterior-lod.png` and its `-no-terrain.png`
control. It crosses into Tamriel, checks a configured 12-cell LOD radius,
moves six cells, verifies resident blocks change and terrain retains a
loaded-cell mask while object blocks exclude retained full cells, and returns to the interior to verify both streams clear.
The Rust `rnd_19_` tests run this installed-data capture, survey WRLD flags,
and exercise independent land/LOD inheritance, nested parents, invalid
parents, focus movement, radius validation and immediate near handoff.

Landscape LOD materials with both `Model_Space_Normals` and `LOD_Landscape`
use a lit spatial shader. The LOD diffuse texture is sampled as sRGB and the
normal texture as linear RGB encoding Skyrim X, Z, Y. The shader converts
that direction to Godot X, Y, negative Z and then to view space. It uses the
same scenario lights, ambient environment and record fog pass as loaded LAND.
Shader type 18 also applies the authored landscape noise operation to linear
albedo, using the shared `textures/terrain/noise.dds` mask tiled three times
over the block. Ordinary type 9 landscape LOD does not apply that mask.
A missing normal texture
leaves the converted mesh normal in use. Material cache identity includes
the normal-space and noise choices, so an ordinary mesh using the same texture paths
does not reuse the LOD shader. Geometry remains in RenderingServer instances.

The normal swizzle follows the local Community Shaders
`package/Shaders/Lighting.hlsl` `MODELSPACENORMALS`, `LODLANDSCAPE` and
`LODLANDNOISE` branches, including `GetLodLandBlendParameter` and
`GetLodLandBlendMultiplier`. CommonLibSSE-NG's
`BSLightingShaderMaterialLODLandscape.h` identifies feature 18 and its separate
noise texture. The installed Tamriel BTRs select that feature, and the installed
texture archive supplies `textures/terrain/noise.dds`. Missing noise leaves
the diffuse unmodulated. The shared mask participates in the prepared-cell
upload and interior-prefetch texture byte budgets.
`nifxml/nif.xml` defines the two flags, and xEdit's `Worldspace browser.pas`
`LODTextureFileName` supplies Skyrim's diffuse and `_n.dds` naming. OpenMW's
`components/nifosg/nifloader.cpp` leaves this landscape shader unhandled, and
Godotwind's Terrain3D shader consumes a different terrain format. The host
therefore applies these semantics in its own Godot material.

`set_loaded_terrain_visible` toggles loaded LAND batches for capture masks,
retaining placed objects and collision. `loaded_terrain_meshes` exposes those
mesh resources for edge and material inspection. `godot/rnd_26_lod_lighting.gd` compares
the shipped BTR material against a standard material under six normal
directions, ambient light, fog, rotated geometry and a missing normal map.
Dark and bright albedos with three constant noise masks verify the linear
noise operation before lighting.
It then looks south from Whiterun's gate wall. The capture loads a complete
radius-two LAND ring through `stream_exterior` and the adjoining BTR ring,
because the gameplay one-cell load ring does not fill its two-cell LOD
exclusion. Independent LAND and BTR visibility captures identify the two
sides at the projected shared cell boundary, excluding foreground hill crests.
At least 40 columns must contribute,
with mean linear luminance ratio at most 1.35 and difference at most 0.08.
The captures and process log are stored under `godot/.godot/rnd_26-*`.
This material change does not change the gameplay ring's geometry coverage.

## Whiterun and small worldspaces, RND-27

Gameplay destination selection uses the shared CellIndex. WRLD DATA Small World
selects every authored grid cell in that worldspace, sorted south to north then
west to east. Destination loading adds the world's persistent cell once, separately
from grid cells. This matters for Whiterun's persistent CELL, whose XCLC does
not make it a grid cell. Parent LAND is requested at each selected coordinate
when PNAM Use Land Data is set. Child LAND takes precedence as before.
The `stream_exterior` inspection API retains its explicit radius square.

PlayerNode retains a small world's complete renderer and collision resources
while walking inside it and updates the logical focus cell without removing
city cells. Leaving the worldspace replaces those resources. Ordinary exterior
worlds load a 5 by 5 square, with a two-cell load and unload radius, so the
default LOD exclusion radius no longer hides a band of cells that was never
loaded. Tamriel's authored Whiterun LOD placements use the near renderer in
that square and BTO object LOD beyond it. Parent terrain and object LOD continue
to follow the independent PNAM Use LOD Data chain.

Every installed grid cell is registered for `exterior_cell_batches` and
`release_exterior_cell`, including cells installed through gameplay location
replacement. Departed cells leave that grid lookup. Bulk geometry still uses
RenderingServer RIDs and this change adds no reference nodes or saved state.

Sources are local UESP `uesp-modfileformat/WRLD.wikitext`, xEdit
`TES5Edit/Core/wbDefinitionsTES5.pas` WRLD DATA and PNAM definitions, and the
existing RND-19 parent-LOD and block consumer. The renderer uses that consumer
rather than introducing a separate city mesh loader. RND-27 fixtures cover
complete small-world selection, persistent-cell separation, ordinary radii,
invalid destinations and independent parent land and LOD flags. The installed
record check surveys Whiterun's grid cells and Tamriel's city LOD placements.
`godot/rnd_27_whiterun_capture.gd` checks gameplay residency, retained city cells
while moving to the gate, parent LOD instances and replacement on gate crossing.
It captures the plains from Dragonsreach with and without terrain LOD, requiring
500 changed pixels, then the city from outside the gate against a control
without district cells and distant objects, requiring 1,000 changed pixels
across 120 columns. Captures and the Rust runner's log go under
`godot/.godot/rnd_27`.
The capture freezes gameplay and hides CanvasLayer overlays so a cold-load fade
does not cover the world view.

## Loaded terrain and LOD join, RND-29

Terrain LOD retains BTR blocks touching the near grid. Its coverage follows
LAND surfaces actually uploaded, including hysteresis-retained cells. Pending
cells keep their LOD coverage. `terrain_lod_join::join_surface` clips archived
triangles to cell squares and omits loaded LAND squares. It subdivides exposed
edges at the loaded 33 by 33 sample spacing and matches their heights, retaining
BTR texture coordinates, colors, tangents and material. The triangle fan's
centre keeps its BTR height. Distant blocks outside the near boundary retain
their original geometry. Changed LAND coverage rebuilds only touching blocks
and their neighbours from retained source surfaces. Departed cells regain LOD
coverage in the same cell-delta application. The recorded byte budget includes
retained source arrays and joined upload arrays, and releases distant blocks
when their combined cost exceeds the cap. Bulk geometry uses server RIDs.

`terrain_lod_stats.covered_cells` lists masked coordinates as `x:y` entries.
`set_terrain_lod_debug_color` gives resident LOD a flat magenta override for
capture checks. `set_terrain_lod_visible` retains its existing visibility toggle.

Distant water is built as cell-sized planes at the same resolved XCLW or
inherited DNAM height as loaded water. CELL and WRLD No LOD Water flags omit
planes. WRLD NAM3 selects the distant material when it resolves. The renderer
masks these planes inside loaded LAND cells without moving their water heights
to LAND edge heights. It does not use one uniform NAM4 plane across cells with
different recorded water heights.

The layout source is xEdit `Core/wbLOD.pas`, `BlockForCell` and `LoadFromData`,
and `Core/wbDefinitionsTES5.pas`, CELL and WRLD water fields and flags.
`Core/wbDefinitionsCommon.pas` defines NAM3 and NAM4. OpenMW's
`components/terrain/buffercache.cpp` connects different edge resolutions with
triangle fans. Its regular-grid index buffers do not fit archived irregular
BTR triangles, so the host clips those triangles before joining their edges.

`rnd_29_terrain_join` tests cell coverage, all four height seams, unload
restoration, preserved attributes, pending cells, water heights and malformed
inputs. Synthetic records cover inherited water heights and both No LOD Water
flags. The installed-data survey reads cell water heights around Whiterun and
the western coast. `rnd_29_terrain_join_capture` runs a windowed plains view
with an orthographic camera and a magenta coverage control. It requires zero
LOD pixels inside loaded cells, zero background pixels in the five-cell
boundary area, and at least 1000 LOD pixels outside. Clearing the near grid
must restore at least 1000 LOD pixels inside its former coverage. A second
normal view stands 1.7 metres above the installed LAND centre sample and looks
south across the plains. Captures and logs are written under
`godot/.godot/rnd-29-*`.

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

`DataSource` excludes bit 23 editor markers from visual model selection.
For models selected through STAT, CONT, FURN and DOOR, it also removes
TriShape and BSTriShape blocks reached through an `EditorMarker` node.
RND-20 extends this to hidden and shaderless helper geometry, described
below. xEdit's
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
Godot materials. Slot two is loaded as a glow texture when `SLSF2_Glow_Map` is
set. The slot has other uses on skin and hair shaders, so its presence alone
does not enable glow. Authored emission without a glow map remains a constant
color, except for the `_g.dds` `Own_Emit` convention documented under RND-24.

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

## Emission flag and glow-map mask, RND-24

`MaterialDesc::emits_light` requires positive authored emission and either
`SLSF1_Own_Emit` or `SLSF2_Glow_Map`. The standard and decal materials
multiply enabled emission by the glow texture when one resolves.
`SLSF2_Glow_Map` activates slot two directly. The installed Bannered Mare
`CandleHornChandelier01.nif` instead has `SLSF1_Own_Emit`, a clear glow-map
flag, and `HornCandles01_g.dds` in slot two. For this authored convention,
`Own_Emit` plus a slot-two path ending in `_g.dds` also activates the texture
mask. Other slot-two paths remain unused for emission. `Own_Emit` without a
glow texture emits its constant authored color. Materials with emissive
values but neither flag do not emit. The NIF reader already preserves both
flags and the slot. Their bit definitions were checked against
`engine-refs/nifly/include/Shaders.hpp` and the local `uesp-modfileformat`
`TXST.wikitext` slot table, so no parser changes were needed.

`rnd_24_emission_requires_own_emit_or_glow_map_and_glow_uses_slot_two` checks
both enabling flags, the `_g.dds` Own_Emit convention and that an unrelated
use of slot two is not treated as a glow map. The installed-data test checks
that the Bannered Mare cell places the chandelier and that its emissive
surfaces have a usable mask texture. `godot/rnd_24_emission_capture_test.gd`
renders the installed Bannered Mare chandelier with Godot's production
material path. It checks the glow map leaves most chandelier surface pixels
dark while candle pixels remain bright.

## Tree materials and texture paths, RND-31

The NIF mesh material reader normalizes texture-set and effect-shader paths
before the host resolves them. It lowercases and changes backslashes to
forward slashes, discards any build or data prefix before the first
`textures/` component, and prepends `textures/` when that component is absent.
Texture misses keep the checker fallback. The first warning for each
mesh-and-texture pair names both paths.

Materials carrying `SLSF2_Tree_Anim` use a shader material that samples the
authored diffuse and normal textures. Its vertex stage uses the red vertex
color as the sway weight. Its fragment stage applies `NiAlphaProperty` cutout
thresholds, and its culling mode follows the material's double-sided flag.
Other tree material fields continue through the ordinary NIF material path.
Placed `TREE` records resolve their `MODL` path through the same cell model
pipeline used by static references.

The installed RND-31 survey scans NIFs visible through the install's BSA
archives and loose data directory, checks normalized referenced paths against
that view, reports references unavailable in that view, and verifies named
tree roots and animated cutout, two-sided surfaces across the mesh corpus. It
also checks that rooted animated tree surfaces and the Tundra Driftwood tree
have resolved diffuse and normal maps. The NIF normalization follows nifly's
`TrimTexturePaths` and its path-trimming test. Nifly's
`Nodes.hpp` names the tree node classes. OpenMW's local NIF loader was checked
for scene handling but does not supply Skyrim tree shader semantics.

## Materials (2026-09-17)

`soe-dds` is now a `godot-host` dependency. `data_source.rs` resolves each surface's
`MaterialDesc` into decoded textures. NIF material `diffuse` and `normal` fields carry
normalized `textures/...` paths. `Vfs::read` handles case and separator normalization as
well. The read goes through the same `Vfs` the meshes use, so a loose file
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

PRF-08 attaches each bulk material to its mesh surface before creating the
RenderingServer instance. Batched, unbatched, MultiMesh and distant LOD meshes
use this path. A placement rebake carries the old surface material to the new
mesh. This exposes the mesh/material combination to Godot's pipeline
precompilation without an instance material override. `surface_material_count`
reports how many full-resolution instances have a surface material.

`godot/render_warmup.gd` renders the configured scene under a ColorRect during
benchmark and walkthrough loading. It synchronizes submitted GPU work there
and waits for at least three drawn frames and 500 ms without changes to any
pipeline compilation counter. It fails after 30 seconds without settling.
Camera, lights and materials stay active. No per-reference nodes are added.
Loading durations, worst loading frame and pipeline counts are reported
separately. This is a quiet-counter predicate, not an API for pending compiler
jobs. It does not warm views or effects introduced later.

The render warm-up timeout begins at the first `frame_post_draw`, so time
before the first drawn frame does not use the 30 second settle limit. The
successful load report records that delay as `load_first_frame_wait_ms` and
records the subsequent pipeline settling time as `load_render_ms`.
`godot/prf_08_test.gd` checks the timeout behavior after a 77 minute simulated
undrawn wait and checks both report fields with a windowed renderer.

Sources read for PRF-08: local `godot-docs/tutorials/performance/pipeline_compilations.rst`,
`classes/class_renderingserver.rst` and `classes/class_projectsettings.rst`.
Godot documents deferred compilation for instance overrides, covered rendering
during loading, the pipeline counters and disk shader caching. The implementation
uses these stock Godot 4.4 APIs and retains the default enabled shader cache.
The headless `prf_08_whiterun_mesh_materials_are_bound_before_first_frame` check
loads nine installed Whiterun cells in batched and unbatched modes and checks
attached materials before drawing.
It requires `SKYRIM_DATA` and a Godot executable. The deterministic GDScript
contracts run through `prf_08_loading_reporting_and_repetition_contracts_in_stock_godot`.
See [performance.md](performance.md#prf-08-first-use-rendering) for windowed acceptance.

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
RenderingServer instances and albedo resources, then renders a terrain instance in the main
viewport and checks the captured frame for visible, varied, non-white terrain pixels. The
inspection rows include each terrain instance's bounds center so the camera can frame that
surface without using unrelated cell geometry.
`godot/pan_capture.gd` orbits and pans that block through 25 shots for a quick visual sweep.
Distant geometry is Bethesda's precomputed
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

An appearance-bound `SkinnedActorNode` keeps its target `GlobalFormId` and a
cursor into the shared world's animation request log. On each `advance`, it
passes new requests for that target to its HKX interpreter before updating the
graph. Requests for other forms and requests already present when the visual
was bound are skipped. The cursor advances across the whole log, so one
request reaches the matching visual at most once. Graph events are queued back
to the shared world after their pose is uploaded. `Game` delivers them to
`OnAnimationEvent` listeners and resolves matching `PlayAnimationAndWait`
latent calls once. Removing the visual fails any waits for its target.

The CK `PlayAnimationAndWait` contract comes from
`engine-refs/ck-wiki/PlayAnimationAndWait_-_ObjectReference.wikitext`: send
the named animation to the target graph, return true on the requested event,
and false if the graph unloads first. This bridge only drives visuals
explicitly bound through `PlayerNode.bind_actor_appearance`. General reference
animation graphs are not instantiated by this bridge.

`PlayerNode.gameplay_animation_events` returns the source file and object IDs
and event name for each graph event delivered to the active Game. The ANM-58
Godot integration test starts a Papyrus `WaitForAnimationEvent` through the
host diagnostic methods, emits the bound rig's signal, and checks that the wait
returns true. It starts a second wait and checks that the graph unload signal
returns false. The diagnostic methods expose the VM thread result to that
integration check.

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
walking.

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
inside a FUZ and use the in-process WMA v2 decoder in the default build.

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

### Live dialogue mixer check

`godot/dlg_11_audio_test.gd`, launched by
`crates/godot-host/tests/dlg_11_godot_audio.rs`, opens a generated voice BSA
through `SoundBank::open_data`. The fixture sets `sLanguage=ENGLISH` in
`Skyrim_Default.ini` to select its English voice archive.
The script creates a current `AudioListener3D`, starts a response at the speaker
position, captures nonzero samples from Godot's master mixer, stops that
player through the exposed `stop_voice` method, starts a replacement at a
second position, and captures its output.
Capture polls for signal until the stream duration plus half a second, since
voice assets can start with silence. The generated WAV fixture includes a
quarter second of leading silence. The capture buffer is cleared between
responses using Godot's `AudioEffectCapture` API, documented in local
`engine-refs/godot-docs/classes/class_audioeffectcapture.rst`.
This exercises the decoded `AudioStreamWav` and `AudioStreamPlayer3D` path in a
headless Godot process. The Rust test runs when `SOE_GODOT_BIN`
names a Godot 4.4 or newer executable and the release extension exists at
`target/release/libsoe_godot_host.so`. Otherwise it reports a skip. Build the
extension with `cargo build -p soe-godot-host --release` before running it.
The script rejects an extension missing the voice methods before playback.
The script exits on each checked failure and success path. Both the generated
WAV speaker check and the installed xWMA check run headless. The Rust
test gives each Godot child 110 seconds, kills and reaps it at the deadline,
and includes captured standard output and error in a failure message. A
missing completion marker fails the test. The script watchdog exits after 90
seconds, and the headless launch also quits after 3,600 frames as a secondary
bound.
The script explicitly loads `soe.gdextension` when needed, so a fresh checkout
does not require an editor import or a cached extension list. This uses
`GDExtensionManager.load_extension` from the local
`engine-refs/godot-docs/classes/class_gdextensionmanager.rst` reference.

`dlg_11_installed_xwma_voice_plays_in_headless_godot_mixer`, gated on
`SKYRIM_DATA`, reads the surveyed `maleelfhaughty/voicepowers_shout01b` xWMA
FUZ from the installed English voice archive, then runs the same mixer check
with that voice. If that path is unavailable, the test samples up to 60 sorted
vanilla FUZ paths and uses the first decodable xWMA voice with signal. The
generic mixer test uses generated WAV data with 250 milliseconds of leading
silence. Capture polls for signal through the clip duration plus one second,
retains the 0.0001 peak threshold and clears captured samples after the
interrupted player drains. The capture uses the ring-buffer API described in
local `godot-docs/classes/class_audioeffectcapture.rst`. Both checks prove
Godot's mixer received both voice buffers at the requested speaker positions.
They do not prove that a physical speaker or headset produced audible output.
That still needs a windowed run with installed data and a `SoundBank` in the
tree.

### Physical output check, HST-05

`godot/hst_05_audio_output.gd` lists every output returned by
`AudioServer.get_output_device_list`, selects the requested device, and plays
the requested SoundBank effect three times. Its defaults are the system
`Default` output and the 0.477 second vanilla footstep
`sound/fx/fst/dirt/jumpdown/fst_dirt_jumpdown_01.wav`, read from the installed
`Skyrim - Sounds.bsa`. Each successful start prints an `AUDIO_CHECK` line with
the selected device, sound path and play number. A load or playback failure
prints its reason and exits with status 1.

Run the script without `--headless` while listening through the selected
physical output. Pass a device name and sound path after `--`, or omit either
argument to use its default:

```text
SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" godot --path godot --script hst_05_audio_output.gd -- "Default" "sound/fx/fst/dirt/jumpdown/fst_dirt_jumpdown_01.wav"
```

Record the date, selected device, sound, what the person heard and process
exit status in [hst-05-audio-observation.md](hst-05-audio-observation.md).
The listening field stays blank until a person reports what they heard.
Set `SOE_HST_05_LISTENING_CHECK=1` to validate that field with
`cargo test -p soe-godot-host --test hst_05_breaker_adversarial` after recording
the observation. Automated gates skip that human listening check. The
`hst_05_audio_output` integration test runs the same script with Godot's dummy
audio driver and installed game data. It checks device listing, selected
device reporting and all three SoundBank starts. The dummy driver does not
establish that a physical output made the sound audible.
Footsteps are driven by the character controller. Dialogue response audio is
now driven by the dialogue menu.
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
map. PlayerNode passes its press edge to `MeleeContact::begin_from_attack_press`,
which arms the player's right-hand contact once and ignores held frames. The
authored animation's `HitFrame` marker still supplies the contact pose and
reaches the same shared Game path as other melee contacts. The CMB-04 test
injects the mapped keyboard press and verifies one hit in that Game. A
single-key or single-button action can also be pressed through Godot's Input
action API, while chorded bindings continue to require every device input.

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
When shared gameplay is active, `PlayerNode` copies this full Skyrim-space
look direction into `World` before script ticks so `Game.IsPlayerSungazing`
uses the active camera orientation. `Game.ShakeCamera` requests are drained
after each game tick and applied through the player `Camera3D` horizontal and
vertical offsets until their requested duration expires.

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
`SkinnedActorNode.set_first_person_geometry_visible` applies the same visibility
change to every shape instance in its rig. The windowed
`godot/hst_17_geometry_visibility_test.gd` check loads the installed vanilla
body, hides its shape instances, then shows them and compares rendered pixel
counts.

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

## Ordered and persistent workers and data-source split (2026-09-19, 2026-09-29)

The data source now uses a cached bounded Rayon pool for master parsing,
archive opening and mesh construction. The pool is reused across batches with
the same worker count, and changing the count replaces the cached pool.
Indexed collection preserves input order, so archives still enter the VFS in
load order and retain last-archive precedence. A worker panic returns
`DataSourceError::WorkerPanicked`, rather than shortening the mesh result list
and potentially attaching later geometry to the wrong reference. Pool
construction errors are returned as `DataSourceError::WorkerPoolBuild`.

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

Convex point clouds are reduced to the boundary vertices using
`soe_physics::hull::triangles` before `PhysicsServer3D` receives them. If that
cannot produce a 3D hull, the host removes duplicate and non-finite points and
submits a slightly padded bounds hull. A repaired hull reports its source mesh
path and NIF collision node name once per process. Godot's `shape_set_data`
returns no hull failure result, so this check prepares the point set before
the call rather than retrying after Jolt reports an error. The PRF-32 checks a
44-point cloud with interior points and a flat cloud, then sends both repaired
hulls to Godot's Jolt server. Its installed-data check streams Bannered Mare
through the host and runs `physics_test.gd`, requiring collision bodies,
contact and no Jolt shape error.
The installed collision helper builds and registers the release extension.
Clippy can leave metadata at the debug library path, so existence alone does
not establish a loadable library.

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
also releases the reservation. The strict distance boundary and cell-change
release are covered by `hst_09_host_stands_after_movement_or_cell_change`.
The marker selection and saved actor state are described in
[game.md](game.md#furniture-activation-hst-09).

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
`gameplay_swing_doors` reports each candidate's current `opened` state so the
harness tries closed doors and reports failure if none creates an animated
door node.
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
matching key, `PlayerNode` opens its native lockpick panel if the player carries
a `Lockpick`. The angle control submits attempts to the shared `Game`. A
successful pick unlocks the reference and resumes ordinary activation. A
failed attempt consumes a pick and leaves the panel available. Keyed activation
remains on the regular game path.

PlayerNode presents the current native lockpick panel when activation reaches
this path. Its angle control and Turn lock button apply the attempt to the
shared Game, then successful picks resume ordinary activation. The
`hst_03_lockpick_handoff.gd` runtime test uses the synthetic
`inventory_fixture` records to activate a keyless locked door, submit the
record-derived successful angle, activate the unlocked door and reject a
level 255 key-only door. `ui_08_lockpicking.gd` drives the same Godot controls
through a failed turn, checks the remaining pick in the native inventory, then
completes a successful turn and activates the door. The fixture starts with two
lockpicks so both outcomes run in one session. Generate the fixture with
`cargo run -p soe-godot-host --example inventory_fixture -- /tmp/soe-hst-03
"$SKYRIM_DATA"`, then run the script with the fixture path:

```sh
rtk env SOE_UI_TEST_DATA=/tmp/soe-hst-03 "$SOE_GODOT_BIN" --path godot --script \
  "$PWD/crates/godot-host/tests/ui_08_lockpicking.gd" --resolution 640x360
```

The PlayerNode handoff passed on Godot 4.4.1 at 640 by 360 with llvmpipe.
The script activated fixture reference `0:000511`, opened the lockpick panel,
submitted angle 23.6, then activated the now-unlocked door. It also confirmed
that key-only reference `0:000513` did not open the lockpick panel. The fixture
used installed Skyrim assets in place and was generated under `/tmp`.

The integration gate passed with 915 Rust tests and 11 Python runner tests.
All three clippy passes and every lint tool ran. The cell-index API change
also removed unused mutable bindings in the benchmark and navmesh test,
without changing their behavior.

An initial test accepted the cell selected by startup scripts and therefore
tested a different location. The corrected test asserts the requested
Bannered Mare identity after startup. The first import aborted when sandbox
restrictions denied an editor socket. Direct runtime execution succeeded.

### Animated static collision, HST-01

`AnimatedStaticNode` can build an `AnimatableBody3D` with a concave trimesh
shape below each generated mesh when `collision_enabled` is set before
`load_nif`. The body synchronizes to physics frames, and both body and shape
remain descendants of the animated mesh, so Godot applies the same scene
transform to the visual and collision geometry. `open()` resets controller
time and starts its per-frame evaluation. The collision option defaults off.
This path is for small animated references and does not change bulk cell
batching. Godot documents `AnimatableBody3D` for manually moved doors and its
`sync_to_physics` property for physics-frame updates. The host advances the
controller from its process callback.

The standalone `hst_01_door_swing_test.gd` loads a synthetic NIF controller
through the byte loader. It advances the rotation track and confirms that the
animated physics body and shape follow the mesh transform.

### Resident doors, HST-21

Cell installation separates DOOR surfaces from ordinary material batches.
Each built door has one control `Node3D` and RenderingServer mesh instances,
with the winning reference transform and the normal cached materials. Both
swing doors and XTEL load doors use this path. World owns the open and enable
states. Opening and closing retain the node, meshes and render RIDs until
cell detach. A disabled door hides its render instances and control node
without destroying them. Re-enabling restores the same resources.

The old activation path created an AnimatedStaticNode on open and queued its
removal on close. It skipped load doors, while the batch sync collapsed every
opened door's geometry. Movement collision only followed enable state, so
SimpleWorld retained a closed panel after the render path hid it. The resident
path replaces that activation-time node creation. SimpleWorld now removes
opened door bodies and restores them on close. Both player query collision
and optional streamer PhysicsServer collision detach open or disabled door
bodies from their space. Closing a disabled door keeps every collider absent.

The resident mesh swings reversibly through a quarter-turn about the authored
placement origin at one quarter-turn per second and holds each endpoint.
This fallback follows local OpenMW `apps/openmw/mwworld/worldimp.cpp`,
`World::rotateDoor`. It is not the NIF controller-manager Open/Close sequence
player. The existing HST-01 direct-controller utility remains available.
No moving door is included in a static cell occluder. The source for the
open/close destination is local CK `SetOpen_-_ObjectReference.wikitext`, and
REFR placement and XTEL identity use the existing record reader. The local
godotwind `door_portal.gd` hides door meshes during portal previews. That path
does not fit ordinary door presentation here, which retains visible geometry
and disables the original panel collision while open.

`door_state` exposes the stable node identity, render RID identities, local
bounds, placement, visibility and swing fraction. `advance_doors` advances
presentation without a gameplay tick. `swing_door_count` reports resident
control nodes rather than activation-time nodes. The `hst_21_` tests cover
reversal, endpoint holding, invalid elapsed time and the SetOpen native's
unlocking behavior. The SKYRIM_DATA-gated scene uses the installed Bannered
Mare and Breezehome front doors, opens and closes each ten times, checks node
and render identity, and moves a CharacterBody3D capsule through each open
doorway with a blocked closed-door control. Each sweep ends with the capsule wholly
beyond the panel plane and before the interior end wall. It checks disable, close while
disabled, re-enable and cell detach across movement, query and streamer
collision. Windowed captures are external under `target/hst_21`.

This path adds one control node per built door, with no child mesh or physics
nodes. The windowed Godot 4.4.1 llvmpipe scene passed all twenty cycles and
exited zero. Bannered Mare had six resident door nodes and the front door
had five render surfaces. Breezehome had one node and three surfaces.
Across 1,000 zero-time `advance_doors` calls, the whole API call averaged
0.086 microseconds for the six doors and 0.081 microseconds for the single
door. These measure held-pose presentation calls with GDScript dispatch,
not the full host frame or moving-mesh server submission cost. Door surfaces
remain server resources. The path adds no dependencies or saved fields.

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

`aud_05_godot_interrupted_transition_replaces_stale_player_stream` runs the
SoundBank in Godot with three synthetic archive tracks. It checks both players
during the first fade, verifies each configured fade changes player volume,
then interrupts the fade and checks that the reused slot no longer has the
first track assigned. AudioEffectCapture confirms mixer output during playback.

### Asynchronous music and ambience (AUD-12)

Music and ambience requests decode on a worker. The main thread creates the
`AudioStreamWav` only after PCM is ready, then starts playback. A newer request
invalidates an older decode result, and the current music continues playing
until its replacement is ready. Decoded music and ambience paths share a
cache with a 64 MiB audio-data budget. Effects and voices
keep their existing path, which decodes synchronously.

SoundBank retains decode workers and joins them before destruction, so no
worker executes extension code after the library unloads. Finished workers
are joined during polling. This follows the local
`godot-docs/tutorials/performance/using_multiple_threads.rst` shutdown example.
An accepted request can still fail during decoding, which updates `last_error`
without incrementing the playback start count. The AUD-11 counter check waits
for decode completion before checking missing assets and restarts.

`aud_12_music_and_ambience_requests_do_not_block_and_reuse_decodes` checks
request latency, replacement cancellation, continued current playback, cache
reuse and the cache budget using generated WAV files. It also checks the fast
effect and voice path. The installed PRF-12 test exercises the real walkthrough
and checks the audio system's worst recorded frame time.

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
exposes `stop_voice(slot)` to Godot scripts and refuses to stop a slot that has
since been released or reused for an effect.
The default host build decodes vanilla xWMA in process through the audio
crate's native WMA v2 decoder. The optional `xwma-ffmpeg` feature selects
ffmpeg as a comparison decoder. The dialogue tests exercise response selection
and replacement with mock output. The Godot mixer test checks speaker playback
with generated WAV data, and its installed-data case checks xWMA decoding
through Godot's mixer.

The selected FUZ's LIP block is decoded with the voice buffer. SoundBank
tracks the active dialogue slot and samples the native mouth targets against
that player's playback position, spreading the stored frames over the decoded
voice duration. The `dialogue_mouth_weights` method returns the current weights
in LIP target order. Stopping or finishing the voice clears the track and
returns its weights to zero. Lines without a decodable LIP block keep the mouth
weights empty or neutral. `tests/dlg_20_lip_playback.rs` exercises decoded
frames through the same slot lifecycle used by SoundBank, including playback
updates and interruption. `tests/dlg_20_godot_lip.rs` packages a generated WAV
and LIP block into a voice archive, then launches Godot with the extension and
opens it through a live SoundBank. It checks that the exposed mouth weights
match the frame selected by the player's playback position and return to zero
when `stop_voice` interrupts playback. A nonzero player start offset makes the
time sample deterministic in headless or windowed runs.

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

`cast_touch_spell(spell_file, spell_object)` casts a touch spell at the first
eligible actor intersected by the camera's forward physics ray. The ray length
comes from the winning SPEL's SPIT range converted to Godot metres. The player's
own bodies are excluded, and the nearest static obstruction or unmapped body
blocks actors behind it. The returned actor must still be enabled, undeleted
and alive in the shared World. `World::cast_spell` performs its authored reach
check before applying effects. Invalid spells, unavailable physics space,
misses and rejected targets return false.

`aimed_spell_projectile_contact(spell_file, spell_object,
projectile_instance_id, contact_body)` maps the contacted actor body RID
through that same owner table and applies the aimed spell through
`World::cast_spell_on_projectile_contact`. The first contact consumes the
projectile's instance ID before applying effects, so duplicate Godot contact
signals cannot repeat the hit. Unknown bodies and rejected casts return
false. The projectile owner calls `aimed_spell_projectile_finished` when it
frees the projectile, which releases the one-shot guard. This callback does
not create or advance projectile flight.

xEdit's `wbDefinitionsTES5.pas` defines SPIT delivery value 2 as Aimed. The
CK `OnHit_-_ObjectReference.wikitext` describes spell projectiles as contact
hits and notes that each associated magic effect can produce its own hit
event. The world contact API applies the spell effect list once for the
reported actor contact. `mag_02_projectile_contact.rs` checks that repeated
contact signal delivery for one projectile dispatches only once and that
freeing it releases the guard. Flight creation and movement are not covered.

`selected_location_position()` returns the first static geometry hit along the
camera look ray in Godot metres. `cast_location_spell_at_look(spell_file,
spell_object)` sends that same hit point to the shared cast path. The explicit
`cast_location_spell(spell_file, spell_object, x, y, z)` entry point accepts a
Skyrim world position. Both check the segment from the player's eye to the
point, allowing a hit at the endpoint and rejecting an earlier obstruction.
Missing physics data rejects the cast. The world checks the authored SPIT
range and centers each effect's area at the selected point.

`godot/mag_17_location_selection_test.gd` loads the installed Bannered Mare,
checks that the live camera ray selects loaded geometry, and casts the vanilla
Fire Rune at that selected surface through the host callback.

Pose-derived segments come from two new entry points.
`melee_pose_contact(name, skeleton)` finds the armed hand's weapon node in a
Skeleton3D (`WEAPON` then `Weapon` then `NPC R Hand [RHnd]` for the right hand,
`SHIELD` then `Shield` then `NPC L Hand [LHnd]` for the left) and takes its
global pose. `melee_bone_contact(name, transform)` takes that world transform
directly. Both run the segment from the node origin along its negated basis Z
column, which is Skyrim local +Y after the axis map in `units.rs`. The length
is `World::melee_reach` converted to metres. `sync_contact_actors` and
`contact_actor_body` expose the store for diagnostics, `contact_actor_bodies`
lists every body with its owner and World position in Godot space, cell,
coordinate space, dead flag and detection range,
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
stagger behavior. When a new active deadline appears, the rig writes the
actor's latest `stagger_magnitude` to the graph's `staggerMagnitude` input and
sends `staggerStart` once. At expiry it sends
`staggerStop` once.
The graph selects and plays its authored hit reaction, then follows its own
transitions back to the locomotion state. This rendering step only reads World
state and advances the animation interpreter. It never resolves the hit or
changes health, so the combat event cannot apply damage twice.
`cmb_27_shared_game_applies_stagger_damage_once_per_hit_frame` checks the
shared Game applies damage once and consumes duplicate hit-frame events while
the target is staggered. `gameplay_actor_value` and
`gameplay_actor_is_staggered` inspect that same shared World. The installed
`godot/cmb_27_live_reaction_test.gd` hits a resident actor with a vanilla
Iron Warhammer, checks the bound graph enters `Stagger_State`, advances the
Game through expiry, and confirms health stays unchanged after the hit.

`activeragdoll/src/main.cpp`'s `StaggerActor` and the local Nemesis master,
stagger and one-handed behavior graphs establish the variable and event names.
CMB-14 stores the stagger deadline, computed hit magnitude and source
direction. The master graph does not expose
`staggerDirection` from its referenced stagger graph, so the host cannot select
a hit direction through the outer graph's variable table. The gated
`cmb_27_vanilla_hit_reaction_plays_and_returns_to_locomotion_without_damage`
test checks the installed master graph plays its authored stagger clip and
returns to the original active state. Exact hit direction and same-frame
repeated deadlines are not represented by the current World event state.

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

`godot/cmb_38_ragdoll_test.gd` loads a vanilla Bannered Mare actor, explicitly
adds its ragdoll, queues `PushActorAway` through the shared `World`, and checks
that Jolt changes the root body's velocity. It then removes the explicit
ragdoll and checks that the bodies are freed and animated rendering resumes.
The host bridge calls the same `World` methods used by the Papyrus natives.

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

Papyrus `Game.RequestSave` and `Game.RequestAutoSave` write verified native
generations below `user://saves/manual` and `user://saves/automatic`, retaining
up to three per directory. The host creates the selected directory when the
request runs and reports write failures to Godot's error log. `Game.ForceFirstPerson`
and `Game.ForceThirdPerson` set the live player rig and refresh the camera.
`Game.FadeOutGame` uses the existing screen overlay, applies the requested
delay and duration, and chooses black or white from its black-fade flag.
Negative or non-finite timing values are rejected by the Game native and
logged. These operations run after the VM tick that enqueued them.
Camera requests share the rig mode helper used by the camera tests. The fade
overlay uses one presentation value for its color, opacity and visibility,
including when a door fallback fade overlaps a Papyrus fade. Save requests use
the same generation writer as explicit host saves.

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
The controls compile with gdext, and `godot/ui_06_hud_render_test.gd` checks
their appearance in a running Godot window. It starts gameplay in the
installed Bannered Mare cell, checks viewport pixels in the three meter
labels, bars and compass, damages each player actor value through
`PlayerNode::damage_player_actor_value`, then checks that the visible labels
and bar fills update after Godot processes frames. The method applies damage
to the active shared game and returns false if gameplay is not active or the
actor value name is invalid. Synthetic Rust tests also change all three
shared actor values and check their updated fractions.

Run the host check with the release extension loaded and installed data set:

```sh
SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" godot --display-driver wayland --path godot --script ui_06_hud_render_test.gd
```

The local `engine-refs/skyui/src/HUDWidgets/skyui/widgets/meter/MeterWidget.as`
and `engine-refs/skyui/src/Common/skyui/components/Meter.as` informed the
0-to-1 bounded fill. SkyUI keeps a meter hidden until `initCommit` and permits
fill-direction, color, flash and animation settings. This native HUD displays
the three meters when gameplay starts and does not implement those Scaleform
widget calls. `MiniMapWidget.as` reads a player angle separately from a
compass angle. The native compass currently centers `PlayerNode` camera yaw,
so it does not apply cell NorthMarker orientation. The CK
`GetActorValue - Actor` and `GetActorValueMax - Actor` pages define current and
modified-maximum stat inputs. Compass direction is not an actor value.

UI-01 adds a modal Control tree to PlayerNode. One CanvasLayer, one
PanelContainer, two VBoxContainers, one Label and one Button form the fixed
shell. A Theme supplies shared text and panel styling. Opening sets keyboard
focus and releases the mouse. Escape and the Close button close the menu.
Closing frees presenter controls, clears held gameplay actions and restores
prior focus and mouse mode. Mouse restoration waits for window focus when
closing while unfocused. Repeated opens and closes do not add shell nodes.

UI-21 documents the public API and license boundary for SKSE Menu Framework
and dMenu style ImGui pages, and compares host rendering options. Those
frameworks are not implemented by the native menu framework described here.
See [ui-frameworks.md](ui-frameworks.md) for the research and proposed rows.

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
presentation under the project intent. The UI-11 VM fixture adds text and
toggle rows, changes toggle enabled state and checks that one enabled selection
delivers one `OnOptionSelect` callback. The
`ui_11_runs_unmodified_skyui_manager_and_base_pex_fixture` test loads both
unmodified scripts from `SOE_SKYUI_CONFIG_BASE_PEX` and
`SOE_SKYUI_CONFIG_MANAGER_PEX`. It runs the base script's page callback and
option constructors, updates its option flags, rejects a disabled toggle, then
sends the manager's option event and checks that the base script reaches the
config callback once with its page-strided option ID. The execution check skips
when either compiled PEX is unavailable. A separate source fixture reads the
unmodified `SKI_ConfigBase.psc` and `SKI_ConfigManager.psc` files, checking the
text and toggle constructors, flag packing, page-strided selection callback
and manager forwarding. Set `SOE_SKYUI_CONFIG_BASE` and
`SOE_SKYUI_CONFIG_MANAGER` to use different source paths. Script execution
remains unverified when the compiled PEX files are absent. The host menu
manager and Scaleform APIs remain outside this path.

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
value with `set_color_value`. A color dialog starts with SkyUI's `-1` start and
default colors. Its active dialog setters model `SetColorDialogStartColor` and
`SetColorDialogDefaultColor`. Cancel clears the active dialog without sending
an accept callback or changing the row. Disabled and hidden options reject
these callbacks. The callback names and argument order follow the local
SkyUI `SKI_ConfigBase.psc` header and source, while conflict details follow
`SKI_ConfigManager.psc`. `KeyConflictScanner::from_bindings` derives vanilla
conflicts from the active gameplay `Bindings`, including every key in a chord
and the keyboard, mouse and gamepad columns. `remap_key_live` checks those
bindings first, then asks each registered MCM config to run its
`GetCustomControl(keyCode)` method in registration order. It reports the first
nonempty custom control with that config's `ModName`. The older scanner
constructor and `remap_key_scanned` remain available for focused callers and
fixtures. Color dialog parameters are exposed by the host model. They are not
yet wired to Papyrus calls to the two SkyUI setter functions. Synthetic tests
cover active binding discovery, registered script calls, callback eligibility,
conflict precedence, color dialog parameters, acceptance and cancellation
without Skyrim data.

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

The item card and inventory row share this category reader, including the food
icon. Its flag follows local xEdit `Core/wbDefinitionsTES5.pas`, and SkyUI's
`ItemMenus/InventoryDataSetter.as` supplies the food classification behavior.
SkyUI cache scene checks require `SOE_TEST_SKYUI` and either `SOE_FFDEC_JAR`
or `SOE_FFDEC`. Without those inputs they report a skip. JPEXS FFDec's local
CLI help lists `yes`, `ask` and `local` as import options. The converter uses
`local` alone, leaving both import and prompting disabled, as confirmed by
the bundled `CommandLineArgumentParser` and `ConsoleUrlResolver` classes.
The archive survey checks bounded sprite definitions when Ruffle's parser
rejects SkyUI's omitted sprite End tags.

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
It lists positive inventory totals after applying the optional FormList's form
and keyword matches. Stolen stacks are hidden unless the request asks to show
them. Selecting a row transfers one item through `Game::gift_menu_transfer`
and refreshes the rows. Closing the modal menu resolves the latent Papyrus call
with the accumulated nonnegative gift value. Both giving and taking use the
same source and target transaction path. The `ui_15_gift_menu_lists_only_filtered_forms_and_keywords`
test checks that a direct form match and a keyword match appear while an
unmatched inventory item does not. A second UI-15 test checks that a stolen
row is hidden by default and shown when requested.

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

`PlayerNode::gameplay_creation_benches` exposes enabled creation bench
references in the loaded logical cell for runtime tools. The CRF-06 installed
runtime check activates these references through `activate_reference`, then
checks the recipe Control buttons and their confirmation callback in Godot.
With the fresh live player inventory, the test exercises the insufficient
ingredient rejection. The Rust presenter tests cover successful transaction
confirmation.
It runs with `SKYRIM_DATA` and the host extension loaded:

```text
SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" godot --path godot --resolution 640x360 --script crf_06_crafting_menu_test.gd
```

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

`crates/godot-host/tests/ui_04_quest_journal.rs` checks the presenter against
synthetic quest records, including a stage change that replaces the displayed
objective. `godot/ui_04_quest_journal_test.gd` loads installed data, starts the
shared Game in Bannered Mare and checks that the native menu panel and quest
text controls are populated. It then changes MQ101 to stage 10 through the
PlayerNode host bridge and checks that the open menu rows change on a later
process frame. Run it after building the extension and importing
the Godot project with
`SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" godot --headless --path godot --script ui_04_quest_journal_test.gd`.
This verifies the Godot control tree and its text, without asserting a rendered
image.

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
cannot be evaluated. The native map surface is opened from the quest journal.
It plots marker XY positions north-up on a Godot Control and hides markers
whose presenter visibility is false. The surface refreshes when its resolved
marker data changes. The presenter reads the shared world and adds no saved
state. QUST target rules follow local xEdit
`TES5Edit/Core/wbDefinitionsTES5.pas`, `uesp-modfileformat/QUST.wikitext` and
the CK `Quest_Script.wikitext` objective display semantics. Location marker
resolution follows `uesp-modfileformat/LCTN.wikitext`, where `MNAM` identifies
the world-location marker reference.

The lockpicking presenter opens when player activation reaches a locked,
non-key-only reference and the player has a lockpick but no matching key. Its
angle slider and Turn lock button submit attempts to `Game::attempt_lockpick`.
A miss consumes a pick and leaves the menu available for another angle. A
successful turn closes the menu and activates the same reference through the
ordinary Game path. The menu is presenter state only. The
`hst_03_lockpick_handoff.gd` and `ui_08_lockpicking.gd` exercise this PlayerNode
path against a synthetic keyless door and the shared Game. CK Lock and
GetLockLevel pages plus xEdit's XLOC definition establish lock state, level
bands and the key-only level 255 case. The angle rule follows the game crate
policy documented in [game.md](game.md#authored-locks-and-keys-2026-09-19).
Synthetic `ui_08` game tests cover both outcomes. The live Godot interaction
test checks the failed and successful turns. Visual presentation and minigame
feel remain unverified.

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

## Effect shader playback snapshot, SCR-24

`effect_shader::EffectShaderStack` copies active EFSH and target identities
with each remaining duration from the shared World. PlayerNode syncs this
snapshot after gameplay startup and each gameplay tick, clears it when the
data source changes, and exposes `gameplay_effect_shaders` as a diagnostic
array. It allocates no scene node or server resource. The host does not yet
render EFSH authored textures, color keys or particles on reference geometry.
An opt-in `scr-24-test` build adds a synthetic request driver to
`CellStreamerNode`. `godot/scr_24_effect_shader_test.gd` drives the actual
RenderingServer path and checks keyed refresh, Stop, finite expiry, clear and
tree-exit cleanup. Build with
`cargo build -p soe-godot-host --release --features scr-24-test`, then run
`godot --path godot --script scr_24_effect_shader_test.gd`. The regression
checks temporary server-instance ownership, not authored EFSH appearance.
See [game.md](game.md#effect-shader-playback-scr-24) for the Papyrus contract.

## Image-space modifier rendering, MAG-71

PlayerNode synchronizes active modifier identity, age and strength with the
winning base IMGS for the loaded cell or selected weather. Base IMGS values
continue through the Environment and CameraAttributesPractical controls. The
camera Environment starts as a copy of the WorldEnvironment when present.
Clearing the base and modifier sources removes the camera resources. The host weather
selection method accepts a WTHR editor id, two slots and a blend. An empty id
returns to CELL/XCIM. `godot/walkthrough.gd` supplies the RND-18 record sky
and interior lighting.
The camera environment refreshes its sky and fog inputs from the current
WorldEnvironment before applying image-space adjustments.

`ImageSpaceStack::render_layers` samples active authored IMAD curves through
the shared World. `RenderedModifiers::from_layers` composes tint, blur radius,
double vision, depth strength and distance, HDR scale, bloom threshold, and
cinematic saturation, brightness and contrast using each layer's active
strength. This is the host composition policy, since local xEdit and CommonLib
sources do not define exact weighted stacking.

`ModifierRenderer` applies tint, blur sampling, double vision, bloom threshold
and scale, and the selected cinematic channels in
`godot/mag_71_image_space.gdshader`. DOF
strength, distance and range update the camera's `CameraAttributesPractical`.
The pass is attached to the existing player CanvasLayer and hides when no
visible color channels are active. It creates no per-reference node or server
resource.

The base reader retains all authored HNAM, CNAM, TNAM and DNAM values in its
snapshot. The host maps the supported subset to stock Godot controls. The
`mag_71_rendered_image_space_test.gd` pixel fixture loads the production shader
and checks tint, blur, depth-of-field and bloom threshold against a checkerboard
scene. The Rust test
`mag_71_composes_authored_render_channels_with_active_stack_weights` checks
weighted composition for synthetic curve samples. The pixel fixture still sets
shader parameters directly, so an end-to-end rendered authored IMAD fixture
through the active stack remains unverified. These fixtures do not claim pixel
parity with Skyrim or implement target-based radial blur.

`mag_72_windowed_fixture_reads_hdr_color_and_depth_pixels` launches Godot with
a 640 by 360 window and captures a checkerboard SubViewport. Its Rust runner
resolves synthetic CELL/XCIM and selected WTHR/IMSP records through World and
ImageSpaceStack, then passes their render values to the Godot script. The script
applies those values to Environment, CameraAttributesPractical and a tint
overlay. It compares rendered pixels for the clear, cell, weather and
restored-cell states, and compares depth-of-field enabled and disabled output.

Sources read under `engine-refs`:

- xEdit `Core/wbDefinitionsTES5.pas` defines IMAD channel order, DNAM counts,
  IMGS base values and depth fields. CommonLibSSE-NG
  `include/RE/T/TESImageSpaceModifier.h` distinguishes base data, animated
  curves and modifier-only effects.
- Godot `classes/class_environment.rst` and
  `classes/class_cameraattributespractical.rst` define the Environment and
  practical depth-of-field controls used by the host. Its
  `tutorials/shaders/screen-reading_shaders.rst` defines screen-texture access
  for the color pass.

The game test `mag_71_samples_authored_color_blur_depth_and_hdr_curves` checks
synthetic animated keys. The installed test
`scr_33_installed_imad_lifetimes_and_curves_sample_winning_records` is gated on
`SKYRIM_DATA` and samples winning IMAD records at their active normalized ages.
The rendered Godot fixture uses llvmpipe. It does not measure frame time or
verify Skyrim shader parity.

The installed layout survey and counts are recorded in
[game.md](game.md#image-space-rendering-prerequisite-audit-mag-71). The pixel
fixture uses a synthetic scene rather than a vanilla gameplay capture. Frame
time and exact Skyrim shader parity remain unmeasured.

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
expression names only. `bound_actor_expression_weights` reads the effective
expression from the bound actor in the shared world and resolves those weights
for the head's available targets. `SkinnedActorNode` submits the selected
weight and clears competing expression targets on each pose update, while
preserving unrelated morphs. The actor NIF pipeline loads matching `.tri`
expression deltas and uploads them as Godot blend shapes.
`SkinnedActorNode.debug_expression_weights` reports the latest submitted
target values. `PlayerNode` exposes debug setters used by the bound-head
integration check to change and clear the player's override through the shared
world. `bind_actor_expression_head` connects a loaded head mesh to the actor's
effective expression without replacing the mesh.
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
resolves to image data. The windowed check
`godot/rnd_05_vanilla_npc_material_check.gd` renders that body through
RenderingServer, captures the 640 by 360 viewport and checks for visible
character pixels with diffuse color variation. On Godot 4.4.1 with installed
data, it passed with 4,874 character pixels, 3,093 colored pixels and 17
quantized color bins. The capture was saved to
`/tmp/rnd_05_vanilla_npc_material_check.png`. This exercises the vanilla body
mesh and its authored skin and clothing textures, not a complete NPC head or
full race appearance assembly.

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

## LOD rebuild support, RND-16

`lod_rebuild::LoadOrderHash` hashes ordered plugin names, enabled flags and
resolved slots. `LodBuildCache` stores completed values in memory by that
hash. A cache miss starts the supplied builder on a worker thread, and
`LodBuildRequest::finish` publishes its complete result under the key. A
builder can return object and terrain products together. This helper is not
connected to `DataSource::open_loadout`, the object or terrain generators, or
the Godot streamer, and its cache is not persisted between processes.

`silhouette_error_pixels` measures the maximum edge displacement between two
axis-aligned projected bounds. `select_far_level` chooses the coarsest level
whose caller-supplied projected error is within its pixel limit. These helpers
do not project meshes or measure their rendered silhouettes. The streamer
still uses one configured level, and generated terrain LOD is not implemented
here. Automatic load-order monitoring, terrain generation, runtime far-level
switching and a rendered under-one-pixel silhouette survey remain unverified.

The named DynDOLOD and xLODGen documentation was not present in the local
reference corpus. The available Wrye Bash distant-LOD note explicitly covers
Oblivion, so its format claims are not used for Skyrim. Object LOD generation
continues to follow the local TES5Edit `Core/wbLOD.pas` source described above.

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
wanted, nearer or farther blocks are stale, loads and distant unloads are
capped per tick and sorted nearest first or farthest first. Near overlaps
are released immediately. `full_res_radius` covers retained full cells,
including the unload radius when the near streamer uses hysteresis.
One streamer covers one LOD level, a caller that wants
more than one level resident at once runs one streamer per level with
disjoint radius bands, finer levels nearer the focus. Picking a level by
distance can use `lod_rebuild::select_far_level`, which chooses the coarsest
level whose measured projected silhouette edge error is below its pixel
limit. The host does not yet feed rendered reference measurements into this
selector.

### Background LOD products, RND-16

`lod_rebuild::LoadOrderHash` keys generated output by ordered profile plugin
names, enabled state and slot assignment. Callers mix in
`soe_esm::LoadOrder::fingerprint()` so edits to plugin contents also
invalidate the cache. `LodBuildCache::request` returns a cached product when
the key matches, shares a pending worker for the same key, or starts a worker
for a cache miss. `GeneratedLod` carries
object and terrain output together, and `finish` publishes only after the
complete worker result is available. A failed worker leaves the cache
unchanged.

`silhouette_error_pixels` measures the maximum displacement of the four
projected bounds edges in pixels. `select_far_level` chooses the coarsest
candidate below the configured error limit and ignores non-finite
measurements. The cache and selector are host APIs. The cell streamer does
not yet schedule rebuilds when a profile is reloaded or apply selected levels
to rendered references.

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
`returned_to_main_menu`. `is_main_menu` reports this state. A title sequence
starts only when a Godot `Animation` has been registered under its name with
`register_title_sequence`. PlayerNode plays the matching animation through its
child `AnimationPlayer`, stores the active name and emits
`title_sequence_started(name)`. `is_title_sequence_playing` and
`title_sequence_playback_position` report playback state. A plain return and
starting a new game stop playback and clear the active name.

The Creation Kit `engine-refs/ck-wiki/Game_Script.wikitext` says
`QuitToMainMenu` forces the game back to the main menu and
`StartTitleSequence` plays the specified sequence in the title menu. The
SCR-31 host tests call both globals through Papyrus and check their transient
requests. `QuitToMainMenu` clears a current sequence, while
`StartTitleSequence` preserves the requested sequence name.

`PlayerNode.debug_invoke_main_menu_native` invokes either of those two Game
globals through the active VM and consumes the resulting host request. The
Godot check `godot/ui_18_main_menu_test.gd` registers a two second animation,
starts it through the Papyrus native, verifies its playhead and animated
PlayerNode property advance after gameplay is released, then checks that
quitting clears the active sequence and stops playback. Run it with installed
game data and the built extension:

```text
SOE_GODOT_BIN --path godot --script ui_18_main_menu_test.gd --resolution 640x360
```

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

Player cell changes and streaming arrivals call the Game transition methods.
Those methods bind attached references and resolve queued cell events before
returning. This includes startup placement, coc, restored-game placement and
neighbor resource arrivals. The SCR-01 repair adds no per-reference host nodes.

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
its request immediately. `godot/hst_19_screenshot_capture_test.gd` checks the
resulting PNG in a 640 by 360 windowed Godot run with installed data. The
`hst_19_windowed_godot_saves_the_papyrus_requested_frame` host test launches
that check and requires it to report success.

## Per-cell occluders, RND-07

`CellStreamerNode.cell_occluders` optionally creates one RenderingServer
occluder mesh and instance for each streamed cell. `batch::cell_occluder_mesh`
selects at most four nonempty opaque static surfaces by descending triangle
count, keeps source order for equal counts, and applies a `MultiMesh` surface's
instance transform before combining its vertices. Skinned, blend and cutout
surfaces are excluded. Cell release and node cleanup free both occluder RIDs.
The export defaults to off.

Godot 4.4's `tutorials/3d/occlusion_culling.rst` requires a simplified
occluder representation made from static objects. Skinned actor surfaces
change with their skeleton, so including them in the generated stationary
mesh would leave stale occlusion as the actor moves.

The candidate is based on Godot's local `engine-refs/godot-docs` sources:
`classes/class_arrayoccluder3d.rst` documents the procedural vertex and index
mesh, and `tutorials/3d/occlusion_culling.rst` explains that Godot CPU
rasterizes occluders and that scenes with few occlusion opportunities can
lose performance.

The 2026-10-02 comparison used the installed Whiterun Bannered Mare cell,
Godot 4.4.1 Forward+, llvmpipe, a 640 by 360 window with the benchmark's fixed
1600 by 900 render target, and three 600-frame samples per mode. Both modes
drew 189,506 triangles with 145 RenderingServer instances. With occluders off,
the p99 samples were 40.514, 40.018 and 44.655 ms, median 40.514 ms. With
occluders on, they were 39.019, 41.130 and 46.134 ms, median 41.130 ms. The
enabled median was 0.616 ms higher, so the export remains off. These results
are specific to this scene, machine and renderer.

`crates/godot-host/tests/rnd_07_adversarial.rs` checks empty and nonopaque
input, deterministic source ordering for ties, top-four opaque selection,
valid combined indices and transformed `MultiMesh` placement. The frame-time
comparison was run through `godot/bench.gd`. An exterior attempt was rejected
because its large world coordinates produced zero rendered primitives.

## Restored player attachment, SAV-19

`PlayerNode::attach_restored_generation` reads a verified native generation
into a fresh `Game`, installs the restored player cell, collision and camera,
and advances one frame before reporting success. A failed generation read
leaves the active game untouched. The destination cell and all six pose
components must be present and finite before the host sync starts. The Godot
acceptance check saves a live player's generation and attaches it to a fresh
player using the same installed cell data.

## Deterministic host work, PRF-14

`PlayerNode.work_counts` returns all ten operation counters from the last
`advance`, including its nested game tick, rays, headless capsule queries and
host render instance creation. `CellStreamerNode.begin_work_frame` and
`end_work_frame` bracket synchronous scripted work and return the same counter
names, including zeros. End a window before awaiting another frame.
RenderingServer `instance_create2` calls charge one instance, including
MultiMeshes, actor surfaces, particles, lights and occluders. Host physics ray
calls charge one query. Counts describe engine submissions, excluding Godot's
internal renderer and physics work. Data-source workers enter the caller's
recorder for each ordered chunk.

`godot/prf_14_work_test.gd` loads the installed Bannered Mare, compares actual
instance creation with the streamed count and verifies empty-window reset.
The Rust wrapper runs it in an isolated project with an explicit extension list,
loading Cargo's current test-built library rather than a cached release build.
It is a headless correctness check. The counter paths add no per-reference
nodes and no saved state. The row uses the existing RenderingServer and
PhysicsDirectSpaceState3D APIs described in the local Godot class docs.

## AI planning measurement, PRF-11

`PlayerNode.prepare_ai_planning_check` prepares the active world's NAVM graph
and query caches. `ai_planning_microseconds` reports the last shared AI driver
tick, including door refresh, package evaluation, navigation slices and movement.
`ai_package_evaluations` reports the active world's evaluation count.
`ai_planning_details` reports door refresh, evaluation scheduling, resumed
navigation, actor processing, automatic evaluations, package selection and
switching, conditions, combat flags and target checks, combat acquisition,
procedure movement, fleeing, pursuit and path requests, plus instantiated actor
count. Navigation details include request setup, search advancement, route
result conversion and fallback containment, and flee search. Procedure details
include tree entry, marker selection and Turn Undead checks. Marker details
include cell lookup, base filtering, bed classification, reference initialization
and resumed scans.
Actor details also separate state checks, Calm checks and evaluation eligibility.
Phase durations include their nested calls. For example, conditions are
also included in package selection, and new path requests in movement.
These measurement hooks add no actor scene nodes or saved fields.

`godot/prf_11_ai_planning_check.gd` starts the installed Bannered Mare through
the normal gameplay entry point and manually advances 3,600 frames at 60 Hz.
It requires at least five contact actors and executed package evaluations,
and rejects any AI driver tick above 8 ms. Graph preparation and gameplay
startup occur before the 60 simulated seconds of measurement. The script
frees its host nodes and quits with its check result. It skips when
`SKYRIM_DATA` is unset.
The result also prints the worst frame's index and inclusive phase timings.

## Shared logging and controls, TOOL-06

The extension installs the shared tracing subscriber at Scene initialization,
using `SOE_LOG` and `user://logs/engine.log`. Host diagnostics use the facade
with subsystem targets such as `host::melee` and `host::data_source`.
`SoeLogBridge` flushes completed one-second message buckets each frame and
queues worker events for delivery on the main thread. Errors use Godot's
`push_error` path and warnings use `push_warning`, reaching the debugger.
Other levels use Godot output. Every enabled level reaches size-rotated files.
Exit-tree and extension shutdown flush the current bucket.

`godot/diagnostics.gd` is an autoload with F3 verbosity controls and a backtick
logging console. Both reload the same filter. F3 shows five seconds of recent
messages. `log` reads the directives and `log <directives>` changes them.
Invalid filters retain the previous setting. The panel releases the captured
mouse while open and restores its prior mode on closure. It adds one control
node for the host, with no node per placed reference.

`SoeLogBridge::log_window` and `dump_log_window` expose inclusive logging-clock
windows, including the current pending bucket, for hitch captures. PRF-18's
automatic recorder is absent in this checkout. The host regression checks the
window export directly rather than claiming automatic hitch capture.
The `tool_06_godot_routing_and_editor_debugger` test runs a windowed scene and
an editor scene with `SOE_GODOT_BIN`, asserting error/warning routing, output,
file context, repeated counts, release-level elision, console reload and F3
controls. Both scenes quit when their checks end.
The tests build the release extension and import the project before running
the scenes so fresh checkouts register the extension's classes.

The routing source is the local Godot documentation's GlobalScope
`push_error`, `push_warning` and `print_rich` contracts. Lifecycle callbacks
follow local gdext `godot-core/src/init/mod.rs`. Details of the subscriber,
retention bounds and dependencies are in [trace.md](trace.md).

## Streamed reference enable state, RND-17

Cell mesh and collision preparation omit references whose authored effective
enable state is off, using the game's winning-record XESP chain query.
When installing prepared gameplay cells, the streamer also filters surfaces
against runtime World state. Undrawn model references retain their cell
ownership as deferred identities. The next gameplay sync builds a deferred
reference only when World reports it enabled. It uses the normal VFS, NIF,
material and collision paths and adds RenderingServer and PhysicsServer3D
resources. Unloading its cell removes the deferred identity.

Built geometry reads effective visibility through `ref_placement` for
notified presentation changes. Disabling collapses batched triangles or sets a MultiMesh member's scale
to zero, and removes the reference's host collision from its physics space.
Enabling restores those resources without reloading the cell. Player movement
and query collision also follow effective enable state. Static movement bodies
are removed on disable and rebuilt on enable, and enabled query bodies use
the same placed colliders. The host's applied-state cache does not own enable
state. No per-reference scene nodes are added.

`rendered_reference_pose` and `render_alignment_error` inspect the uploaded
geometry. `reference_collision_bodies` reports bodies actually attached to a
Godot physics space on the streamer, and movement and query counts on the
player. These diagnostics support the RND-17 acceptance script.

The record rules come from local `uesp-modfileformat/REFR.wikitext` and
xEdit's `wbXESP` in `TES5Edit/Core/wbDefinitionsTES5.pas`. The CK Enable and
Disable ObjectReference pages describe parent children popping together.
The host uses the game's query because it already owns runtime reference
state and resolves winning links.

The shared model selector includes non-marker ACTI bases. This lets the
Whiterun `StockadeBarricade01Activator` use its MODL mesh and authored
collision when its enable parent turns on. ACTI marker bases stay omitted.
Local `uesp-modfileformat/ACTI.wikitext` and xEdit's `wbRecord(ACTI)` supply
the generic model field and bit 23 marker rule.

Streaming statistics count activator models separately from containers.

The stockade barricade NIF stores every BSDamageStage as hidden. Placed-model
preparation selects the intact range containing stage zero before filtering
hidden subtrees. The three trailing range bytes follow local `nifxml/nif.xml`
and `nifly/include/Nodes.hpp`, using the existing NIF header and stream reader.
Other damage ranges remain hidden, and ordinary hidden helpers stay excluded.

`rnd_17_hidden_barricade_damage_stages_build_only_intact_surfaces` exercises
that selection through the VFS and surface builder with a synthetic NIF. The
intact stage builds two triangles, damaged stages contribute no surfaces, and
an ordinary hidden ancestor still hides the intact geometry.
`rnd_17_whiterun_barricade_has_no_surface_or_collision_until_parent_enabled`
surveys all nine disabled-parent ACTI barricades in installed WhiterunWorld.
Each is absent from authored cell geometry and collision, then builds two
nonempty surfaces and static collision after marker `0:0376E0` is enabled.
The survey restores the marker's disabled state between references.

With SKYRIM_DATA and SOE_GODOT_BIN set, the survey also runs
`godot/rnd_17_enable_parent_test.gd` in a 640 by 360 window. Build the release
extension and import the Godot project before running it. The script toggles
the marker four times without reloading the cell, checks uploaded geometry
alignment, host, movement and query collision, and unchanged streamer child
count. It writes disabled and toggled captures under `target/rnd_17` and
requires at least 100 pixels to change by more than 0.05 in one color channel
when the marker first turns on. The script quits and the Rust test checks its
exit status and result marker.

The installed survey passed for all nine barricades. The windowed check on
Godot 4.4.1 with software Vulkan changed 4,949 pixels on enable, measured zero
uploaded geometry alignment error through all four toggles, and switched
movement and query collision between one body and none. The streamer child
count stayed unchanged and the process exited with status 0. The capture
fixture advances 200 ms before drawing to finish the cell entry fade.

## Whiterun white geometry, RND-20

The pre-fix installed-data survey streams the nine grid cells around
`WhiterunOrigin` in `WhiterunWorld` and the nine around `WhiterunExterior04`
in `Tamriel`. Both origins are grid (4, -2). It also inspects persistent
references placed within those blocks. Every surface lacking resolved albedo
was logged with its reference, resolved base, EditorID, record type, model
and complete material texture set. No named DDS path failed in this sample.
The following is the complete base-form list from that survey. Counts are
surfaces, so the forge appears six times for one reference. All listed
texture sets have empty diffuse, normal, glow, environment and mask slots.

| Base | EditorID | Type | Model relative to meshes/ | Surfaces |
|---|---|---|---|---:|
| `0:000010` | `MapMarker` | STAT | `Marker_Map.NIF` | 2 |
| `0:00003B` | `XMarker` | STAT | `MarkerX.nif` | 73 |
| `0:000032` | `COCMarkerHeading` | STAT | `MarkerCOCHeading.nif` | 3 |
| `0:000034` | `XMarkerHeading` | STAT | `MarkerXHeading.nif` | 117 |
| `0:0BF9E1` | `CraftingBlackSmithForgeWR` | FURN | `Furniture/BlacksmithForgeMarkerWR.nif` | 6 |
| `0:09C6CE` | `CraftingSmelterMarker1` | FURN | `Furniture/SmelterMarker.nif` | 1 |
| `0:06E9C2` | `CraftingBlacksmithSharpeningWheel` | FURN | `Furniture/Clutter/BlacksmithSharpeningWheelAnimating.nif` | 1 |
| `0:03DE33` | `WRHallOftheDead01` | STAT | `Architecture/Whiterun/WRBuildings/WRHallOftheDead01.nif` | 1 |

The STAT markers carry bit 23. The crafting models contain hidden
bounds/trigger shapes and shaderless emitter helper shapes. They have no
render material and are excluded from drawing. The Hall of the Dead `glow`
shape, block 11 using BSEffectShaderProperty block 12, has an authored empty
source texture and greyscale texture. Its blended vertex-color glow is the
one intended textureless reference in this survey, `0:03DE34`. Water is
also intentionally colored from WATR instead of an albedo map.

The surveyed reference identities, grouped by base, are:

MapMarker (`0:000010`): `0:0162CE`, `0:072879`.

XMarker (`0:00003B`): `0:01F24F`, `0:02884B`, `0:029AAF`, `0:02C905`, `0:02C915`,
`0:0370B4`, `0:0370D6`, `0:0376E0`, `0:04E4B4`, `0:05411D`, `0:05411F`, `0:054120`,
`0:054128`, `0:054129`, `0:05412A`, `0:0559D8`, `0:0559D9`, `0:05EA41`, `0:05EA42`,
`0:05EA43`, `0:05EA48`, `0:05EA4A`, `0:05EA55`, `0:06BBFD`, `0:06BBFF`, `0:070F35`,
`0:072FDF`, `0:073FFB`, `0:073FFE`, `0:074311`, `0:074DED`, `0:074F31`, `0:074F32`,
`0:074F33`, `0:074F36`, `0:077058`, `0:077059`, `0:078092`, `0:08AF2E`, `0:08CF87`,
`0:0914DE`, `0:091915`, `0:096575`, `0:0A804F`, `0:0BBCF2`, `0:0BBCF3`, `0:0D198C`,
`0:0DDDB3`, `0:0FC87D`, `0:0FC8AB`, `0:0FC8F1`, `0:0FC90B`, `0:0FC90C`, `0:0FC91B`,
`0:0FE2CF`, `0:0FF21C`, `0:10372F`, `0:10483D`, `0:105571`, `0:105572`, `0:105573`,
`0:105574`, `0:10810E`, `0:10810F`, `0:108117`, `0:1095FC`, `0:109600`, `0:10DE9E`,
`0:10EBAE`, `0:10F64C`, `1:002F2E`, `2:00E688`, `3:018B8A`.

COCMarkerHeading (`0:000032`): `0:0215D4`, `0:02FFA4`, `0:0D520D`.

XMarkerHeading (`0:000034`): `0:024049`, `0:02D01C`, `0:02D82B`, `0:02D82D`, `0:02D842`,
`0:02FFAD`, `0:039E76`, `0:039F36`, `0:039F38`, `0:04222E`, `0:04222F`, `0:04401C`,
`0:04D0C4`, `0:04E4A6`, `0:055FAF`, `0:055FB0`, `0:055FB1`, `0:0567DB`, `0:0567DD`,
`0:0567DE`, `0:0567DF`, `0:0567E0`, `0:0567E5`, `0:0567EA`, `0:0567F7`, `0:057FA6`,
`0:057FA7`, `0:057FA8`, `0:057FA9`, `0:05C9EC`, `0:05C9ED`, `0:05C9EE`, `0:05C9EF`,
`0:05C9F0`, `0:062D88`, `0:062D89`, `0:06762C`, `0:07287B`, `0:074F3A`, `0:074F3B`,
`0:076EBE`, `0:076EE4`, `0:0847A4`, `0:0847A6`, `0:088962`, `0:088963`, `0:088964`,
`0:08AD66`, `0:08AD67`, `0:08AD68`, `0:08AD69`, `0:08AD6E`, `0:08AD6F`, `0:08AD70`,
`0:08AD71`, `0:0914E1`, `0:0914F2`, `0:0914F3`, `0:092A65`, `0:092A66`, `0:092B24`,
`0:092B25`, `0:098465`, `0:09E0B5`, `0:09E0B6`, `0:09E0E0`, `0:0A202A`, `0:0A202B`,
`0:0A202C`, `0:0A8050`, `0:0B96D7`, `0:0C07EF`, `0:0CD1A1`, `0:0CD25A`, `0:0D1986`,
`0:0D5321`, `0:0D5322`, `0:0D5327`, `0:0D750B`, `0:0D8F0A`, `0:0DDDA6`, `0:0DF1BB`,
`0:0E732C`, `0:0F1469`, `0:0FB8BD`, `0:0FB8BE`, `0:0FB8C0`, `0:0FF146`, `0:0FF147`,
`0:0FF148`, `0:0FF149`, `0:0FF14A`, `0:0FF14B`, `0:0FF14C`, `0:0FF14D`, `0:0FF14E`,
`0:0FF14F`, `0:104752`, `0:104755`, `0:104756`, `0:104757`, `0:104758`, `0:104759`,
`0:10475A`, `0:10475B`, `0:10475C`, `0:1052F4`, `0:1052F6`, `0:105947`, `0:106CB1`,
`0:108D8D`, `0:109826`, `0:109E3C`, `0:10DEBA`, `0:10DEBD`, `0:10DEC0`, `3:004287`.

CraftingBlackSmithForgeWR (`0:0BF9E1`): `0:0C430A`.

CraftingSmelterMarker1 (`0:09C6CE`): `0:10E391`.

CraftingBlacksmithSharpeningWheel (`0:06E9C2`): `0:016D07`.

WRHallOftheDead01 (`0:03DE33`): `0:03DE34`.

Model selection rejects the editor-marker bit for every supported base
record. REFR occlusion-plane, room and portal fields (`XOCP`, `XORD`, `XRMR`, `XPOD`,
`XPTL`) and XPRM portal boxes exclude a reference from visual streaming. Named NIF EditorMarker
subtrees, hidden subtrees and shaderless Skyrim geometry
are removed before surface building. Collision continues to read the model
through its existing selection path. No per-reference scene nodes are added.

The ACTI models selected by RND-17 also expose `WRStairsStreamLACT` water
geometry. Its `BSWaterShaderProperty` is retained as an unknown NIF block and
has no host material binding. Visual streaming excludes that surface instead
of drawing a default opaque white material. Its effect-shader foam remains
visible. CELL/WATR water planes keep their existing rendering path. Binding
NIF water shaders remains outside this correction.

The expanded ACTI survey also retains reference `0:04221F`, base `0:021513`
(`NorLever01`, `Dungeons/Nordic/Levers/RuinsLever01/NorLever01.nif`). Its
`RuinsLever:3` shape uses a blended BSEffectShaderProperty with empty source
and greyscale textures, flags `0x80000000` and `0x21`, and grayscale vertex
colors. This authored detail is a second textureless exception. The NIF's
separate named EditorMarker shape stays filtered. The strict survey verifies
both identities and shader flags, plus the lever's vertex colors. It covers
5,363 grid surfaces and 226 nearby persistent surfaces after the corrections.

Named albedo paths that fail VFS lookup or DDS decoding resolve to a shared
8 by 8 opaque magenta/black checker. Each failed canonical path logs once
per DataSource, including after prepared textures are cleared. Authored
empty texture slots stay empty. Both StandardMaterial3D and the
shader-material path bind the checker, including if an albedo upload fails.
`CellStreamerNode.material_albedo_stats` inspects their uploaded bindings.
The cell loader logs any retained surface without source albedo with its
reference, base form, EditorID, record type, model and texture set.

Sources are local `uesp-modfileformat/STAT.wikitext` and `REFR.wikitext`,
TES5Edit `Core/wbDefinitionsTES5.pas` and `Core/wbDataFormatNif.pas`, OpenMW
`components/nif/node.hpp` and `components/nifosg/nifloader.cpp`, and
`nifxml/nif.xml`'s NiPSysMeshEmitter definition. The existing NIF reader,
VFS and texture cache are reused. No new parser or dependency is added.

The `rnd_20_` Rust checks exercise form and reference filtering, hidden
hierarchies, helper shapes, missing/malformed textures, path aliases,
concurrent loads and warning deduplication across cache clears. The
SKYRIM_DATA scene survey checks both gate blocks and nearby persistent
geometry against the one documented glow exception. The windowed
`godot/rnd_20_albedo_check.gd` checks production albedo bindings, standard
and decal checker pixels, and installed gate rendering, then quits.

The installed scene check passed with 5,554 surfaces across 18 grid cells
and 126 nearby persistent surfaces. The Hall of the Dead glow was the only
retained textureless reference. The windowed check passed on Godot 4.4.1
with llvmpipe at 640 by 360. Each standard/decal fixture capture contained
7,406 magenta samples and 1,224 black samples, above the predeclared
500-sample minimum for each color. The uploaded material cache contained
108 albedo bindings after WhiterunOrigin and 149 after WhiterunExterior04,
with zero missing named albedo bindings and zero checker fallbacks in the
installed blocks. The script exited with status 0. Captures stay under
`target/rnd-20-fixture`.

## Cell lighting, RND-21

`CellStreamerNode.cell_lighting(editor_id)` returns render values for interior
cells from the winning CELL record. When XCLL is present, its inheritance flags
select ambient and directional colors, fog, directional rotation and fade,
clip distance, fog power and maximum, and light fade distances from the
referenced LGTM. A cell without XCLL uses its LGTM values directly. Exterior
cells return an empty dictionary so callers do not add an interior light to
them. LGTM DATA and DALC follow the local UESP CELL and LGTM layouts and xEdit
XCLL definition in the engine reference corpus.

The walkthrough and benchmark attach the same `record_sky.gd` controller and
create no additional Environment or directional lights. The controller owns
one WorldEnvironment, one weather sun, one interior directional light and six
non-shadowing directional ambient lights. Interior ambient axes follow the
Skyrim-to-Godot coordinate mapping. Only the interior lights are visible for
ordinary interiors, including cells with Show Sky but without Use Sky Lighting.
Exterior sunlight color and direction come from RND-18's current WTHR and
clock sample. The weather sun is hidden below the horizon. Use Sky Lighting
selects this weather lighting for an interior independently of Show Sky.

SkyRecords supplies the resolved cell fields through the same CELL/LGTM
resolver used by `cell_lighting`. LGTM's far fog color, maximum opacity and
light fade fields start at byte 72, after its unused ambient-color block.
Directional ambient colors come from DALC. The resolver accepts the legacy
24-byte LGTM DATA prefix and modern 92-byte DATA, and rejects truncated
modern data or DALC. Interior fog uses both authored colors, near and far
distances, power and maximum opacity, with far distance bounded by a positive
fog clip distance. Runtime point-light RIDs receive their cell's inherited
light fade start and length converted to metres through RenderingServer.
Invalid or empty fade ranges disable distance fading. No saved state,
dependency or per-reference node path is added.

The `rnd_21_` unit tests cover inherited and retained fields, independent
LGTM tail offsets, legacy and malformed template sizes, and point-light fade
conversion. `rnd_21_lighting.rs` checks both production scripts for exclusive
controller ownership. Its installed-data windowed cases run the walkthrough
and the interior and exterior benchmark setup, inspect the active Environment
and every directional light, and check weather and time changes and lighting
replacement in both directions. The walkthrough check pauses simulation
stepping and captures eight rendered frames. It does not measure gameplay
frame time. The benchmark check uses its production setup and warm-up, then
quits before the timing sample.

`godot/rnd_21_capture_test.gd` streams the Bannered Mare main room through the
production controller and captures it at 640 by 360. Its stored average
linear luminance range is 0.02 to 0.65, with fewer than one percent of pixels
clipped in all three channels. Captures are saved under `target`.

## Exterior skies and interior lighting, RND-18

The walkthrough and bench attach `godot/record_sky.gd`. Its SkyRecords
RefCounted sampler reads the gameplay PlayerNode's shared World, or owns a
preview World for a benchmark without gameplay. The controller owns one
WorldEnvironment, one weather sun, one interior directional light and six
non-shadowing directional ambient lights, plus one full-screen fog mesh.
Walkthrough and bench attach this controller without creating a second
environment or independent cell lights. It creates no node per placed
reference.

The sky shader draws upper, lower and horizon NAM0 colors. CLMT sun and glare
textures, moon textures selected by TNAM flags and phase length, and the
texture named by CLMT's night NIF come through the DataSource VFS. The sun
light follows the sampled sun direction and WTHR sunlight color. The ambient
light uses WTHR ambient color. Cloud layers use their authored colors, alpha
and XY velocities. Unique textures upload to an RGBA8 Texture2DArray at
512 by 512 per layer. The shader supports 256 cloud contributions, including
source and destination layers during interrupted weather transitions.

The fog mesh reads Godot's depth buffer and blends NAM0 near and far fog
colors across FNAM near and far distances, with its power and maximum opacity.
Sky pixels are excluded. Environment retains the same fog inputs for camera
image-space copies, with built-in fog disabled to avoid a second fog pass.
Interiors use CELL/XCLL and inherited LGTM fields for ambient light,
directional light and fog. Show Sky draws their region sky. Use Sky Lighting
selects weather lighting independently of that background flag. Ordinary
interiors keep a color background. Changing the bound source clears texture
selections and cached uploads.

Interior directional ambient colors use the six lights with shadows disabled.
They are hidden for weather lighting. Interior fog clip limits the far fog
distance, and authored light fade distances are converted from Skyrim units
to metres before reaching the lights.

The night texture reader accepts ordinary NIF texture sets, effect-shader
source textures and SSE BSSkyShaderProperty's baseTexture. The last layout
comes from local nifly `src/Shaders.cpp` and `src/Objects.cpp`. The NIF crate
retains that block as opaque data, so the host reads its texture field without
changing the parser's crate scope. Godot's local sky shader and Environment
documentation supplies the host APIs. The near and far fog color contract
requires the depth shader because Environment exposes one fog color.

`godot/rnd_18_sky_test.gd` streams Whiterun, renders vanilla SkyrimClear at
noon and midnight, captures both views under `target/rnd_18`, and compares the
sky region with NAM0. Its thresholds are 0.08 per color channel and at least
500 matching pixels. It checks sun visibility, fog distances, cloud count,
VFS sun/moon/night textures, moon phases, a live weather transition and
interior replacement. The SKYRIM_DATA-gated Rust runner is
`rnd_18_installed_whiterun_noon_night_sky_capture_and_host_controls`.
The shader-texture unit test covers an SSE block with extra data and truncation.
The installed Bannered Mare has Show Sky set and Use Sky Lighting cleared.
The capture checks that its region sky draws while its own ambient,
directional and fog lighting reach the host.

The capture also checks cloud velocity uniforms and compares two sky captures
120 presentation seconds apart, requiring at least 100 pixels to change by
more than 0.02 in one color channel. The walkthrough smoke checks that only
one WorldEnvironment exists. The benchmark smoke runs the benchmark setup
and rendering warmup for Whiterun, then checks that its sole environment and
sun come from the record sky before exiting. It does not measure frame time.

On Godot 4.4.1 with software Vulkan, the Whiterun capture passed with 9,938
noon pixels and 6,663 night pixels inside that tolerance. Mean brightness in
the sampled sky region was 0.639 at noon and 0.272 at night, exceeding the
0.05 difference threshold. The host texture, fog, phase, transition and
interior checks passed in the same windowed run, which exited with status 0.

### WTHR cloud scroll rates, RND-25

`godot/record_sky.gd` converts each decoded WTHR cloud velocity to UV units
per presentation second by dividing both axes by 400 before uploading them to
the sky shader. The shader advances UV coordinates by that velocity times
elapsed seconds. WTHR speed bytes use `(byte - 127) / 127 / 10`, as described
by the local UESP WTHR page and implemented by Mutagen's Skyrim weather
reader. The `/400` time scale follows OpenMW's sky cloud animation update in
`apps/openmw/mwrender/sky.cpp`.

`godot/rnd_25_cloud_speed_test.gd` loads installed data, selects
`SkyrimCloudy`, and checks each uploaded axis against the source weather rate
divided by 400 with a 3 percent tolerance. It also requires at least one
moving layer. The Rust runner is
`rnd_25_installed_skyrimcloudy_cloud_rate_matches_its_weather_record`, and a
source contract test checks that the shader applies the uploaded rate over
elapsed seconds.

The radius-zero Whiterun bench used 51 renderer instances, 42 meshes and
31,146 triangles. Its 600-frame software Vulkan sample had a 33.105 ms p99
at the benchmark's fixed 1600 by 900 render size in a 640 by 360 window.
It exceeded the existing 16.7 ms p99 budget and exited with status 2. This
measurement includes the world renderer and sky together. The walkthrough
initialized gameplay and the record sky, but its 120-frame capture did not
finish before the supplied launcher's 300-second limit, which returned 124.
The separate `rnd_18_walkthrough_shared_world_interior_smoke` test loads the
walkthrough with gameplay, pauses simulation stepping, checks its shared
World interior lighting and captures eight rendered frames. It passed and
exited with status 0. This smoke run does not measure interactive frame time.

## Sky depth and hemisphere continuity, RND-30

`record_sky.gdshader` uses Godot's background sky pass for the atmosphere,
clouds, sun, moons and stars. These inputs depend on viewing direction rather
than camera position or a finite mesh radius. World geometry occludes the
background, and the sky writes no world depth. The controller creates no
finite sky mesh, cloud billboard or node per placed reference.

Cloud UVs use a stereographic upper hemisphere, `eye.xz / (1 + eye.y)`.
There are no cube faces or clamped projection bands. Cloud alpha fades to
zero across the lowest 0.16 of the vertical viewing direction, meeting the
horizon color continuously. Cloud uploads generate mipmaps after resizing,
and the texture array uses linear mipmap filtering. Star texture sampling
wraps at the spherical longitude boundary.

Sources read were local OpenMW `apps/openmw/mwrender/sky.cpp`, its
`files/shaders/compatibility/sky.vert`, and Godot's
`tutorials/shaders/shader_reference/sky_shader.rst`,
`shading_language.rst` and `tutorials/shaders/advanced_postprocessing.rst`.
OpenMW places a camera-relative sky in an early render bin and disables
depth writes. Godot supplies the background pass and direction inputs,
so the host retains that API instead of porting OpenMW's OSG mesh path.
The depth-mask test uses Godot 4.4's reversed depth buffer, with zero denoting
the background. No dependency or saved state is added.

The `rnd_30_` tests launch `godot/rnd_30_sky_capture_test.gd` windowed at
640 by 360 and quit on completion. Synthetic captures check occlusion at
10, 10,000 and 1,000,000 metres, camera translation with every sky component
enabled, and adjacent samples across sixteen cube-edge, pole and horizon
views. The continuity limit is 0.04 per channel. The installed-data case
uses the walkthrough's PlayerNode and record sky, exits the Bannered Mare
and views the Throat of the World from Whiterun's wall gate. It checks the
gameplay sky binding, then uses shared-source preview controls for noon
SkyrimCloudy. It identifies
BTR mountain pixels by toggling terrain LOD, and distinguishes geometry
from sky with a depth mask. Replacing the sky with magenta must leave those
mountain pixels unchanged and fill every sampled background pixel. The
minimum is 500 pixels in each region, with a tolerance of 2/255 per channel
and no contaminated samples. Fog and reflections are disabled for this
diagnostic comparison. A separate capture retains the authored cloudy sky
and fog. Eight installed-cloud views sample former cube edges and corners
with a limit of 0.08 per channel. PNGs and process logs are written under
`target/rnd_30`.

## NIF effect materials, RND-22

BSEffectShaderProperty meshes use a separate ShaderMaterial path. The host
carries their base color, scale, falloff cosines and opacities, soft depth,
palette path, UV transform, clamp mode, NiAlphaProperty blend factors and
alpha test into material and batch identity. Source and palette textures use
the shared VFS and DDS caches. Effect vertex colors and alpha reach the
mesh upload, and memory accounting includes those arrays and palette textures.

The shader uses base RGB times its scale once, multiplies texture and authored
vertex colors, and applies base alpha, texture alpha and flagged vertex alpha.
Falloff interpolates the two opacities with a cubic smooth step between the
absolute view-normal cosines. Equal endpoints use a step without division.
Palette color uses source green and multiplied base red as coordinates. Palette
alpha uses source alpha and accumulated opacity. The palette flags control
each lookup separately. UV scale, offset and per-axis wrapping are retained.
Soft effects sample the host depth buffer and fade both at scene intersections
and near the camera, using the authored depth converted to metres.

Alpha mix, additive, premultiplied alpha and multiplicative factor pairs use
Godot blend modes. Additive source-one and source-alpha pairs retain distinct
strengths. Other factors evaluate the blend equation against Godot's opaque
screen copy, which cannot include preceding transparent surfaces. Blended
effects never write depth. Opaque effects honor the authored depth-write bit,
and all effects honor depth testing, culling and the alpha-test function.
Only soft effects declare a depth sampler, and only screen-copy blend pairs
declare a screen sampler. Opaque effects with depth writes stay in the opaque
pass. Depth-disabled opaque effects use alpha one in the transparent pass to
avoid Godot 4.4's opaque depth prepass. Its Forward+ surface classification
assigns a depth pass to every opaque material, see
[render_forward_clustered.cpp](https://github.com/godotengine/godot/blob/4.4/servers/rendering/renderer_rd/forward_clustered/render_forward_clustered.cpp#L3595-L3604).
Native blend paths apply the authored lighting influence when effect lighting
is flagged. Otherwise the output uses emission. The screen-copy fallback
uses unlit effect color. This uses stock Godot shaders and adds no reference nodes.

Sources read are nifly's effect fields, OpenMW's `components/nifosg/nifloader.cpp`,
`files/shaders/compatibility/bs/nolighting.vert` and `lib/particle/soft.glsl`,
Community Shaders' `package/Shaders/Effect.hlsl`, and Godot's spatial shader
reference. OpenMW supplies falloff and soft-depth semantics but does not
implement palette sampling in its NIF effect path. Community Shaders supplies
the palette coordinates. The implementation translates those operations into
Godot shader syntax instead of adding a second renderer.

`model_meshes` builds model resources through the production material cache
for inspection and capture tests. The Rust `rnd_22_` tests cover independent
NIF/DDS fixtures, effect material and batch identity, and installed texture
resolution. `godot/rnd_22_effect_material_test.gd` captures synthetic blend,
falloff, palette, vertex-alpha, alpha-test and depth-intersection controls,
then installed FXMistLow01HalfVis near Whiterun and Windhelm interior window
glow over two backdrops. It also checks all 121 factor pairs against an opaque
backdrop. Additional controls capture emissive multiples of 0.5 and 2 without
scaling alpha, falloff at several viewing angles with ascending, descending
and equal cosine endpoints, and all eight alpha-test comparisons at equality.
A rear additive probe checks opaque depth writes against depth-disabled and
blended controls. Soft surfaces are checked both near the camera and away from
scene intersections. Captures are written under `godot/.godot`, outside
tracked content. The SKYRIM_DATA-gated
`rnd_22_installed_mist_and_window_captures_are_translucent_and_bounded` Rust
test runs the installed captures, checks their brightness and backdrop
transmission bounds, and requires the Godot process to exit successfully.

## Additive hearth flames, RND-33

The source-alpha plus one effect pair uses Godot's additive blend with the
computed texture, vertex, base and falloff opacity in `ALPHA`. The blend applies
source alpha once, so low-alpha flame texels do not fill the effect mesh hull.
Blended effects keep depth writes disabled. Greyscale color
and alpha lookups retain the authored palette flags, and falloff remains part
of the opacity calculation. For a fire NIF with an authored greyscale color
palette, an additive effect layer without its own palette path inherits the
first sibling color palette and enables palette-color sampling. Existing
palette assignments remain unchanged.

The installed Bannered Mare survey finds `FireplaceWood01Burning` with an
additive greyscale-palette surface and a separate palette surface with view
falloff. It also finds the placed `Campfire01Burning` effect, whose unpaletted
additive layer inherits the sibling `GradFireExplosion` color palette. The
Godot capture test streams the cell and saves a room view, then draws the first
additive palette-color surface from the installed fireplace NIF through
`model_meshes` and its production material cache against dark and contrasting
backdrops. It reads that surface's installed palette texture and checks that
visible fire pixels are brighter than black, stay within palette hues and that
every near-transparent pixel transmits the backdrop. It checks the shader
retains falloff and saves the isolated hearth capture under `godot/.godot`.

`rnd_33_installed_bannered_mare_hearth_capture_matches_its_palette` runs the
windowed Godot capture when `SKYRIM_DATA` and `SOE_GODOT_BIN` are set. The local
nifly effect-property definitions and OpenMW's
`components/nifosg/nifloader.cpp` supplied the property and blend behavior.
Community Shaders' `package/Shaders/Effect.hlsl` confirms the palette
coordinates already implemented in RND-22. The implementation continues to
use Godot's stock spatial shader and blend state.

## Detection neighborhoods, PRF-31

`godot/prf_31_detection_test.gd` measures one installed Whiterun exterior
update, then walks the player at Whiterun's gate for 30 seconds in
WhiterunWorld and 30 seconds in Tamriel. It samples every physics frame for two 30-second wall-time phases, checks resident
NPCs and movement in both worldspaces, and rejects every
complete detection update over the fixed 2,000-microsecond budget. Each run
quits after reporting its sample count, maximum and over-budget count.
The capsule and exterior residency advance through the host physics and
streaming paths. NPC placements are held fixed and AI planning and VM ticks
are omitted. The timing still covers the whole gameplay detection updater.
The CPU light and actor grids use no new dependencies. The local OpenMW
`components/sceneutil/lightmanager.cpp` intersects light spheres with object
bounds and limits renderer lights. `clusteredlighting.hpp` declares the
rendering light grid. Those OSG paths do not fit this host CPU query, which
retains every influencing light without a renderer light limit.
CK `GetLightLevel` and `CreateDetectionEvent` supply the light range and the
separate source/owner contract. The Rust PRF-31 tests compare indexed samples
with a full reference scan and cover reference mutations, sound ownership,
sparse actor neighborhoods and native-save cache reconstruction.

The installed headless run completed both 30-second phases with 3,623 updates,
9 Whiterun actors near the gate and 446 Tamriel actors. Tamriel formed 530
nearby observations. Maximum detection time was 6,657 microseconds and 1,813
updates exceeded the 2,000-microsecond budget, so the check exited one.
The full-update timing requirement remains unmet, see [performance.md](performance.md).
The check runs a zero-time gameplay frame after load-door transitions to settle
pending unload before exterior walking.

## Papyrus health overlay, PRF-19

`PlayerNode.papyrus_health` exposes Game's total, script and attachment-plugin
health rows as dictionaries. An absent game returns zero totals and empty
rows. The F3 system overlay shows backlog and age, four timer counts, suspended
stacks, waiting stack seconds, running CPU milliseconds and event queues with
session peaks per second. F7 switches between per-script and per-plugin rows.
A scroll container keeps long reports inside the viewport. The adapter adds
no per-reference nodes.

`godot/prf_19_health_test.gd` checks the adapter, F3 and F7 views and installed
script rows when `SKYRIM_DATA` is set. `SOE_PRF_19_CAPTURE` selects a PNG path
for a windowed capture. The script frees its nodes and quits with the result.
The host dictionary and UI calls use the existing Godot 4.4 API path.

## Placed model coverage and reference accounting, RND-32

The shared render and collision model selector accepts TREE, FLOR, LIGH,
MSTT, SCOL, TACT, MISC, WEAP, BOOK, INGR, ALCH, KEYM, SLGM, AMMO, SCRL and
APPA through MODL, alongside STAT, ACTI, FURN, DOOR, CONT and GRAS. ARMO
uses its male MOD2 ground model, falling back to female MOD4 when MOD2 is
absent or empty. ARMO MODL contains armor-addon FormIDs and is never decoded
as a filename. Cell preparation, deferred reference preparation and collision
selection use this shared selector. New types do not increment the old
container-only statistic.

Vanilla tundra shrub NIFs include export paths such as
`skyrimhd\\build\\pc\\data\\textures\\plants\\TundraShrub01.dds`.
Texture lookup folds repeated separators and trims prefixes at a complete
`textures` directory component before consulting the existing VFS. Canonical
and export-prefixed aliases share one texture-cache slot, and loose-file
precedence remains the VFS's responsibility. Material descriptions retain
their authored paths.

The path rule follows local `nifly/src/NifFile.cpp`, `TrimTexturePaths`, and
OpenMW `components/misc/resourcehelpers.cpp`, `findDirectory` and
`correctResourcePath`. Nifly mutates C++ NIF objects and OpenMW also supplies
Morrowind extension fallbacks. This host only needs the directory trimming
and separator folding in its existing read-only DDS cache. Synthetic RND-32
tests check path boundaries and shared cache payloads. The installed census
also rejects checker albedo on named TREE and FLOR texture fields.

The sources are local `TES5Edit/Core/wbDefinitionsTES5.pas`, each named
record's `wbGenericModel`, ARMO world models, Is Marker flags and `wbXESP`,
plus `uesp-modfileformat/REFR.wikitext`. The generic ESM string decoder and
existing VFS, NIF, mesh and bhk readers supply the implementation. These
changes add no dependency, saved field or per-reference scene node.

Bit 23 suppresses rendering only for STAT, ACTI, FURN and DOOR, whose xEdit
definitions assign Is Marker to that bit. Existing NIF EditorMarker subtree
filtering retains ordinary sibling geometry. Occlusion-plane, room, portal
and plane-primitive references are also excluded from deferred rendering,
matching the cell preparation path. Collision retains its existing marker
selection rules.

`CellStats::reference_counts` counts every indexed placement by its winning
base signature, including actors and deleted winners. Built counts unique
FormIDs with returned geometry, rather than NIF shapes, triangles or batches.
The disjoint skip categories are Initially Disabled, XESP parent, runtime
Disable, editor markers and named other reasons. NIFs containing only filtered
marker geometry count as markers. Missing records, absent models, malformed
references, deleted winners, empty non-marker geometry, deferred references
and mesh failures have explicit other reasons. LAND and generated water are
outside this placed-reference accounting.

`DataSource::reference_counts` can audit prepared surfaces against a live
World. A live Disable that changes the authored result is counted separately,
including a Disable of a parent. An enabled reference awaiting preparation
counts as deferred. The streamer repeats the audit against the gameplay World
when installing a cell and exposes totals by signature in
`last_stats().reference_counts`. The CPU loader emits per-cell counts under
the `host::references` debug target. The audit does not instantiate references
or retain authoritative enable state.

Enable checks reuse RND-17's winning-record chain. An enabled parent overrides
a child's Initially Disabled flag, the opposite-state flag inverts the parent
and parents outside the resident cells still resolve through the load order.
The synthetic `rnd_32_` tests exercise those cases, runtime Disable, both
render build paths and solid bhk collision for each added type. Their bhk
fixture follows local `nifxml/nif.xml` and the existing physics synthetic
writer. Marker, model-less, deleted and failed-model outcomes are also tested.

The installed `rnd_32_installed_cells_account_for_every_placed_reference`
test streams the Bannered Mare and three Tamriel plains cells. It checks every
winning REFR enable result against an independent flags/XESP traversal and
asserts that returned model-bearing reference geometry equals placed models
minus markers and truly disabled references. Every other indexed placement
must have a counted reason. The survey writes its counts and plant placements
to `target/rnd_32_whiterun.json`. The windowed
`godot/rnd_32_references_test.gd` verifies uploaded TREE and FLOR identities
and captures their placements under `target/rnd_32` at 640 by 360.

### Installed cell census, 2026-10-06

The census uses Skyrim.esm, Update.esm, Dawnguard.esm, HearthFires.esm and
Dragonborn.esm. Runtime Disable is zero in these fresh authored preparations.
Every row sums to placed = built + initial + XESP + runtime + markers + other.
Model-bearing REFRs satisfy built = placed models minus markers minus truly
disabled references. NPC_ uses actor presentation, IDLM is an idle marker,
SOUN is an audio source and model-less LIGH references provide lighting.
The model-less ACTI bases carry no MODL geometry. They remain identified in
the census rather than being counted as failed meshes.

#### WhiterunBanneredMare, 0:01605E

| Base | Placed | Built | Initial | XESP | Runtime | Markers | Other |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| ACTI | 3 | 1 | 1 | 0 | 0 | 0 | 1 no placed model |
| ALCH | 107 | 106 | 1 | 0 | 0 | 0 | - |
| ARMO | 1 | 1 | 0 | 0 | 0 | 0 | - |
| BOOK | 3 | 3 | 0 | 0 | 0 | 0 | - |
| CONT | 40 | 40 | 0 | 0 | 0 | 0 | - |
| DOOR | 6 | 6 | 0 | 0 | 0 | 0 | - |
| FLOR | 18 | 18 | 0 | 0 | 0 | 0 | - |
| FURN | 31 | 29 | 0 | 0 | 0 | 2 | - |
| IDLM | 12 | 0 | 0 | 0 | 0 | 0 | 12 no placed model |
| INGR | 2 | 2 | 0 | 0 | 0 | 0 | - |
| LIGH | 18 | 0 | 0 | 0 | 0 | 0 | 18 no placed model |
| MISC | 124 | 124 | 0 | 0 | 0 | 0 | - |
| MSTT | 4 | 4 | 0 | 0 | 0 | 0 | - |
| NPC_ | 7 | 0 | 1 | 0 | 0 | 0 | 6 actor presentation |
| SOUN | 1 | 0 | 0 | 0 | 0 | 0 | 1 no placed model |
| STAT | 319 | 302 | 0 | 0 | 0 | 17 | - |
| WEAP | 1 | 1 | 0 | 0 | 0 | 0 | - |
| Total | 697 | 637 | 3 | 0 | 0 | 19 | 38 |

#### WhiterunExterior04, 0:0095FC

| Base | Placed | Built | Initial | XESP | Runtime | Markers | Other |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| ACTI | 14 | 0 | 6 | 7 | 0 | 1 | - |
| HAZD | 2 | 0 | 0 | 2 | 0 | 0 | - |
| LIGH | 3 | 0 | 0 | 0 | 0 | 0 | 3 no placed model |
| MSTT | 17 | 12 | 0 | 3 | 0 | 2 | - |
| NPC_ | 14 | 0 | 14 | 0 | 0 | 0 | - |
| SOUN | 2 | 0 | 0 | 0 | 0 | 0 | 2 no placed model |
| STAT | 130 | 90 | 0 | 6 | 0 | 34 | - |
| TREE | 27 | 27 | 0 | 0 | 0 | 0 | - |
| Total | 209 | 129 | 20 | 18 | 0 | 37 | 5 |

#### WhiterunExterior13, 0:0095FD

| Base | Placed | Built | Initial | XESP | Runtime | Markers | Other |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| ACTI | 6 | 0 | 3 | 3 | 0 | 0 | - |
| HAZD | 4 | 0 | 0 | 4 | 0 | 0 | - |
| LIGH | 1 | 0 | 0 | 0 | 0 | 0 | 1 no placed model |
| MSTT | 13 | 10 | 0 | 2 | 0 | 1 | - |
| NPC_ | 18 | 0 | 18 | 0 | 0 | 0 | - |
| SOUN | 2 | 0 | 0 | 0 | 0 | 0 | 2 no placed model |
| STAT | 52 | 34 | 0 | 4 | 0 | 14 | - |
| TREE | 32 | 32 | 0 | 0 | 0 | 0 | - |
| Total | 128 | 76 | 21 | 13 | 0 | 15 | 3 |

#### WhiterunExterior17, 0:009639

| Base | Placed | Built | Initial | XESP | Runtime | Markers | Other |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| ACTI | 2 | 0 | 0 | 0 | 0 | 0 | 2 no placed model |
| FLOR | 2 | 2 | 0 | 0 | 0 | 0 | - |
| MSTT | 3 | 3 | 0 | 0 | 0 | 0 | - |
| SOUN | 1 | 0 | 0 | 0 | 0 | 0 | 1 no placed model |
| STAT | 60 | 57 | 0 | 0 | 0 | 3 | - |
| TREE | 72 | 72 | 0 | 0 | 0 | 0 | - |
| Total | 140 | 134 | 0 | 0 | 0 | 3 | 3 |

Every selected model parsed, with zero mesh failures. The three chimney-smoke
placements in Exterior04 and Exterior13 have no mesh geometry after their
EditorMarker shape is removed. They count as marker-only models, alongside
the Exterior04 catapult-impact marker. This census measures placed mesh
coverage and does not claim smoke-particle rendering.

The Godot 4.4.1 software Vulkan check passed and exited with status 0. All 133
TREE and FLOR reference identities had uploaded geometry, and the host's
per-kind built counts matched the CPU census for each of the three plains
cells. The streamer child count did not change, and no plains material bound
the checker albedo. The tree and flora captures contain 160,648 and 153,433
scene pixels respectively, above the 100-pixel
threshold fixed before the run. Captures remain under `target/rnd_32`, outside
the source tree. The installed Rust test launches this check when
SOE_GODOT_BIN is also set, after writing the manifest. Build the release
extension and import the Godot project before running that test.

Validation passed eight RND-32 Rust tests, eight model-selection tests, seven
texture-cache tests and two RND-17 cell preparation regressions. These groups
overlap where new tests belong to an existing module. Formatting and
`cargo clippy -p soe-godot-host --all-targets -- -D warnings` also passed.

## PRF-28 light capacity probe

`godot/capacity_lights.gd` measures a baseline and a doubling ladder of
RenderingServer omni-light RIDs. `SOE_CAPACITY_LIGHTS` selects `dynamic` or
`shadow`, `SOE_CAPACITY_WORLD` selects `synthetic` or `whiterun`.
`SOE_CAPACITY_START` and `SOE_CAPACITY_MAX` default to 1 and 256. Whiterun
loads the installed radius-one WhiterunOrigin exterior through
CellStreamerNode. The synthetic scene uses a lit plane and 64 shadow-casting
boxes. Whiterun adds the 64 calibration boxes to its real streamed geometry,
without the plane. Its camera and light grid use the upper end of the vertical
bounds because buried geometry extends the minimum far below the city.
Each Whiterun sample requires more triangles than the calibration boxes alone.
A bounding camera keeps the light grid in view.

Lights move vertically each measured frame. Both geometry and lights use
RIDs, with zero nodes per light. The probe verifies that light pixels differ
from the baseline. Shadow rungs also disable shadows temporarily after the
measurement and check that the rendered image changes. The script owns and
frees each light and instance RID, clears the streamer and quits at the end.

The viewport renders at 1600 by 900, independently of the window size.
Warm-up uses the existing render pipeline helper. Each rung measures at least
600 frames and five seconds with the bench.gd wall-frame, GPU and render CPU
contract. SOE_BENCH_P99_MS and SOE_BENCH_WORST_MS retain the existing defaults
of 16.7 and 33 ms. Reports include the baseline result, renderer, adapter,
scene counts and zero per-light nodes. `CAPACITY_RESULT` distinguishes the
first breach from a limit not reached. A measured boundary exits zero,
an invalid measurement exits nonzero. This is a throughput probe, not a
passing-frame gate.

The probe requires Forward+ and bounds its requested maximum by the project's
clustered-element limit. Its 4096-pixel positional shadow atlas has four
quadrants of 256 maps. These settings are reported with every sample.
The headless `prf_28_capacity_test.gd` checks only ladder arithmetic.
Sources are local Godot RenderingServer, Viewport, light/shadow and pipeline
references, plus godotwind's benchmark manual. Stock Godot supplies the
renderer. No second rendering dependency or new reference-node path is added.

## Load timing phases, PRF-29

DataSource startup records an 18-second diagnostic budget. Cell geometry,
collision, upload, localization, scripts and save operations use 33-millisecond
diagnostic budgets. These thresholds report breaches rather than enforcing
loading deadlines. The report names the largest exclusive retained cause
phase and its asset. Worker queue phases measure submission to chunk entry.
Worker results retain load ownership alongside the existing work recorder.

Static loading carries the resolved base form and model path as owning object.
It separates VFS reads, placed-model parse, CPU surface construction and
texture parse/preparation. Collision loading separates model parse from shape
construction. Actor assets record skeleton and behavior parsing, CPU mesh
construction and actor/base ownership for record-selected rigs. Server mesh,
MultiMesh and cached texture creation record upload submission time, including
waits inside those host calls, rather than GPU execution time.

`CellStreamerNode.load_timing_dump` returns the retained phase table and budget
reports. `write_load_trace` writes ordinary spans and load phases as a Chrome
trace. PlayerNode's session summary also includes load phases. The walkthrough
adds one `load_diagnostics.gd` control node, independent of reference count.
It observes the five pipeline-source counters after drawn frames and writes
`user://soe-loads.txt` and `user://soe-loads.json` on exit. The text dump combines
Rust load phases with compilation observations and current counters.

Each compilation observation reports counter deltas, a drawn-frame interval
and whether that interval exceeds 33 milliseconds. Compiler duration and
responsible asset report unavailable. The local Godot pipeline guide and
`godotwind/src/core/diagnostics/pipeline_compile_monitor.gd` define these source
counters. The public stock Godot API supplies counts rather than compiler
durations or identities. These observations do not establish that compilation
caused a hitch. Specialization remains separate from draw-time compilation.

PRF-29 is partial. Startup, geometry, collision, upload, worker queue and save
phases carry cause classes and budgets. Still missing: compiler duration and
the responsible asset for shader and pipeline compiles (Godot exposes counters
only), the PRF-18 dump (this checkout has no recorder, so breaches are named in
the log and in the `load_timing_dump`, Chrome trace and `user://soe-loads`
files instead), PRF-20's per-asset cell split, PRF-21's per-plugin save split,
and GPU execution time (upload records submission time only).

`prf_29_loads` Rust tests exercise plugin cold/cache paths, archive and loose
reads, engine-save failures, and installed startup, Whiterun geometry, collision
and navigation causes. `godot/prf_29_loads_test.gd` checks windowed startup,
Whiterun uploads, pipeline observations and both dump formats, then quits.

## Reference sync and scene phases, PRF-34

The streamer indexes resident references to their geometry batches when cells
attach or detach. Independent game observers notify rendering, movement
collision and PhysicsServer placements of effective presentation changes.
Idle synchronization drains empty queues. A changed merged batch rebuilds its
members from the original upload. Deferred model references subscribe while
hidden and build through the existing asset path when enabled. Static physics
placements use an index. Movable bodies still poll Godot for simulated poses.
No new reference scene nodes are allocated. Automatic gameplay uploads
image-space presentation once per rendered frame. Manual advances retain
immediate presentation.

PRF-12's report adds reference collision sync, reference change sync, node
transforms, mesh and material uploads, RenderingServer calls and other host
scene work. Nested simulation scopes remain excluded. `host_scene_cpu` sums
these host phases except RenderingServer calls. The F3 overlay reads the same
report. The fixed PRF-34 windowed walk records every completed presentation
frame for sixty seconds per location and checks sync maxima and scene CPU
percentiles. Timings and acceptance results are in [performance.md](performance.md).

Inspection for HST-21 found that `open_swing_door` queues its animated node for
freeing when the reference closes and creates a new node on the next open.
The batched door hides while open. Movement-body visibility uses effective
enable state and does not remove a static door body solely because it opened.
Query bodies additionally hide opened doors. Change notification retains this
existing handoff. Door node lifetime and open-door capsule passage belong to HST-21.

## Object controller rendering, RND-23

Cell loading retains live NIF controller assets for animated object surfaces.
Those surfaces use local geometry outside static batching and MultiMesh
instancing. Each shape has a RenderingServer instance and a private material,
with a RenderingServer skeleton when the NIF supplies skin bones. Shapes of
one placement share their immutable controller asset. CellStreamerNode advances
object transforms, skin poses and material parameters every frame. It adds no
per-reference scene nodes. Animated shapes are excluded from stationary cell
occluders.

Skin transforms combine the sampled model-space bone with its NIF inverse
bind in the mesh's bone order, using the existing host axis and bind-matrix
conversion. Reference movement updates the visual placement from World.
Reference disable and destruction selection hide the original visual.
Reference enable uses the same live loader. Clear, cell departure and tree
exit release instances, skeletons and private materials. Inventory previews
and destruction overlays retain their frozen mesh path.

Successful player activation tries authored Open, Activate, Trigger, Pull,
Push, Down, On, Forward and Play sequences before the older swinging-door
node path. Game animation requests target the same resolved form identity and
start matching named sequences. Each shape maintains its own cursor into the
game's animation request log. The host changes no saved fields and registers
no save section. Playback clocks and poses are transient rendering state.

Animated shader instances update UV offsets and scales, alpha, emissive color
and strength, and effect falloff values. They start from the NIF's authored
UV and alpha defaults, and sampling leaves other placements' materials
unchanged. Lighting materials use the existing shader-material texture,
glow and environment path. Effect materials use the RND-22 shader.

`tests/rnd_23_object_animation.rs` verifies skin transforms, material ownership
and installed controller retention. `tests/rnd_23_capture.gd` draws a Whiterun
banner through the production RID path at two fixed playback times, compares
uploaded transforms and rendered pixels, saves both captures under target,
and checks zero added scene children and resource release. Run it with the
built extension and the installed-data variable:

```text
$SOE_GODOT_BIN --path godot --resolution 640x360 --script "$PWD/crates/godot-host/tests/rnd_23_capture.gd"
```

The installed-data capture on Godot 4.4.1 with software Vulkan samples the
banner at 0.1 and 0.7 seconds. It reports eight uploaded transforms, a maximum
transform difference of 0.025225, 19,151 visible pixels and 17,279 changed
pixels. The same run starts a chest's Open sequence through activation,
observes its changed pose, starts Close and rejects an unknown sequence.
It checks resource release and exits with status zero. The Rust row tests
pass twelve NIF cases and four host cases with installed data.

Controller layout and sampling references are recorded in `docs/nif.md`.
Object animation uses no new dependency. Collision remains on its existing
record-backed host path and does not follow these visual bone poses.

The RID update path uses the pure `ObjectAnimationClock::update` sampler.
Its elapsed counter is f64, skips nonfinite or negative frame deltas and
resets when a named sequence starts. The counter saturates at the largest
finite f32 sampling time. Host unit tests exercise 30 simulated minutes with
steps cycling from 1 to 500 milliseconds, all three cycle modes, invalid
deltas, reset and small-step clock advancement after a billion seconds.
Instance and bone submission loops are bounded by loaded resource counts.
The reported `AnimatedObjects::update` freeze symbol is absent from this
checkout. No live capture or installed-data reproduction was run for this
timing audit.

## Distant scenery, RND-28

PlayerNode streams archived BTO objects and BTT trees alongside BTR terrain.
The load-order VFS supplies each world's available block levels, including
loose overrides. Selection uses coarse blocks farther away and descends into
finer archived blocks within twice the coarse block's cell span. Selected
levels never cover the same block square. A missing finer level retains the
available coarse block. Near coverage excludes whole blocks, and retained
full cells outside the near radius also exclude their blocks. Selection is
cached until the focus or retained-cell set changes. Updates cap new block
loads, release obsolete level choices immediately and enforce the recorded
mesh byte budget. Worldspace inheritance follows WRLD's Use LOD Data flag.
World changes replace all three streams, and interiors and node teardown free
their RenderingServer instances. Scenery adds no per-reference scene nodes.

BTO meshes apply their authored NIF transforms once. Unlike BTR terrain,
archived object shapes already contain the block translation and level scale.
Adding the terrain block translation moves objects away from their cells.
BTT positions are absolute world coordinates. Its type groups index the
world's LST table by the stored type index. The table supplies billboard
width, height and atlas rectangle. Each reference builds two perpendicular,
two-sided quads with authored rotation and scale, using the world's
`textures/terrain/<world>/trees/<world>treelod.dds` atlas and alpha testing.
The reader bounds-checks counts and rejects truncated fields, non-finite
values, invalid dimensions, unknown type indices and trailing LST data.
BTT trailing data is unused, matching xEdit's reader and several shipped
Solstheim blocks. UV overscan and mirrored rectangles are preserved. Archived
reference IDs remain metadata. Render surfaces identify the resolved
worldspace, as merged BTO surfaces do.

Sources are xEdit `Core/wbLOD.pas`, `TwbLodTES5TreeType`,
`TwbLodTES5TreeRef`, LST/BTT read and write routines, block naming and atlas
naming, xEdit `Core/wbDataFormatMisc.pas` array layouts, and
`nifxml/nif.xml` AV-object transforms and segmented shapes. The
local UESP WRLD page documents LOD inheritance and distant settings but the
modfileformat corpus contains no separate LOD file page. The RND-10 source
name `tes4skyrim terrain_lod.py` is absent from the local corpus. xEdit's
Windows generator does not fit runtime asset consumption. The host reads
its output through the existing Rust VFS and Godot mesh pipeline instead.

The `rnd_28_` Rust tests exercise level selection, immediate handoff,
retained cells, byte eviction, LST/BTT validation and billboard placement.
The installed-data case checks every shipped Tamriel object level and tree
block. `godot/rnd_28_player_lod_capture.gd` crosses from Whiterun into Tamriel,
looks toward the Throat of the World from the plains and freezes gameplay.
A flat sky isolates silhouette coverage in a terrain-only control. The check
requires at least 500 object-contributed pixels where that control shows sky
and at least 25 tree-contributed pixels. It captures terrain-only, object and
combined images under `godot/.godot`, checks available rendered levels,
movement, a Solstheim asset switch and full-cell exclusion, then returns
indoors and quits.
`set_object_lod_visible` and `set_tree_lod_visible` toggle resident instances
for these comparisons without changing residency.

## Audio through cell transitions, AUD-13

SoundBank and its music and ambience players are children of PlayerNode. Cell
rendering and collision unloads leave them alive. Load-door transitions and
exterior walking focus changes refresh CELL/XCAS and ambient descriptors.
ASPC/SNAM supplies the acoustic sound, ASPC/RDAT supplies interior region
sounds, and CELL/XCLR supplies weather-filtered exterior region sounds.
Descriptor conditions, gain, pitch, loop mode and chance draws use the shared
Game world. RDSA compares stored fractional chances with normalized draws,
so a 1.0 regional loop survives every valid draw. The bank exposes the selected
acoustic-space identity.

Ambient descriptors resolve SNDR paths relative to Sound/, preserving an
existing Sound/ prefix, then decode on workers through the active DataSource.
Decode failures reach both last_error and the engine log. Cell-selection logs
name the acoustic space, selected and prepared counts and rejected conditions.
The old voices keep playing until the first replacement succeeds. Current
voices then fade out over one second while replacements fade in over one second. Empty
selections fade out directly. A failed replacement keeps the prior voices and
reports its asset error. Each cell selection invalidates pending results from
the preceding selection. Finished and faded voices release their nodes.
This adds no node per placed reference, only players per selected ambient
sound. Music retains its existing two slots.

Music selects CELL/XCMO first, then sound-type REGN/RDMO entries by descending
RDAT priority, then WRLD/ZNAM. Equal priorities retain CELL/XCLR order. An
explicit null CELL music link suppresses inheritance. A regional override
stops lower-priority regional entries. Crossing into another cell with the
same music type keeps the playing track. A completed track requests another
record selection unless MUSC specifies one selection. Silent selections wait
for their authored duration. New music decodes before the existing record
fade transition starts, and a finished file can restart from the cache.

`preview_cell_ambience` provides record-selected audio for scene and editor
previews without a running VM. Conditional descriptors reject that preview.
The runtime uses `sound::prepare_for` with its live VM instead.
The selection and ownership rules come from local UESP CELL, ASPC, REGN and
WRLD pages and TES5Edit/Core/wbDefinitionsTES5.pas. Godot's local
`audio_streams.rst` documents host players and bus output. Those sources
supply semantics and APIs, with no new dependency or independent mixer.
Mutagen's SkyrimSoundAssetType.cs supplies the Sound/ asset root.
The existing game helper visits regional music in CELL order. The host reads
RDAT priority before choosing a type, retaining the game's track and condition
selection rather than copying that interpreter. Vanilla exterior tracks use
GetIsCurrentWeather, so the game evaluator reads the existing shared sky
weather for that predicate. Audio retains no separate weather state.

The `aud_13_` Rust tests cover music precedence, priorities, null selection,
regional overrides and malformed fields. `godot/aud_13_ambience_test.gd`
checks short authored loops, fade overlap and cleanup, weather masks, stale
decode cancellation, failed replacement continuity and surviving ownership.
It also checks music fade overlap and an ended file restarting from the cache.
The installed-data `godot/aud_13_transition_test.gd` activates the Bannered
Mare exit, crosses Whiterun's gate to Tamriel and walks across exterior cell
boundaries through host capsule movement and streaming, with AI and VM ticks
omitted. It runs headless because these assertions concern mixer samples,
ownership and cell identities. Captured samples are split into half-second
windows for 20 seconds after each door crossing, requiring each window's
peak above -60 dB. Sampling follows the mix rate rather than render frames.
In Whiterun it also seeks the current music near its end and checks that
another selection starts.
Both scenes quit and the Rust runners require successful process exit.
The runners use isolated projects with explicit extension registration, so
a fresh checkout needs no shared Godot import cache for these checks.
The fixture scene runs in a window. Their stdout and stderr captures are
written under `target/aud_13`.
