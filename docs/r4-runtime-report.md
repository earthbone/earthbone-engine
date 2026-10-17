# Round 4 runtime part A, 2026-10-17

Branch: `lead/r4-runtime`. The loading, camera and first person regression
checks pass. Total software-rendered frame time still exceeds 33 ms, and this
run does not establish the user's real GPU frame budget.

## Commits

| Commit | Root cause and change |
|---|---|
| `3bb460f34` | Earlier agent: dropped physics time replaced with a wall clock |
| `77317179a` | Earlier agent: tree pipelines warmed under the loading cover |
| `271f0f436` | Static removal indexed by body with bounded occupied cells |
| `44732cc60` | Poll preparation, slice publication and prepare query hulls on workers |
| `a36680f5b` | Distant visibility selection runs on a worker |
| `368dfc565` | Tree billboard handoff uses source cells instead of whole blocks |
| `f2d87e97f` | Camera samples timestamped controller poses on the wall clock |
| `7dd30eac2` | First person geometry follows the current animated camera anchor |

The inherited uncommitted loading work was reviewed and completed here. New
backlog rows split the measured causes into bounded units. PRF-41 and PRF-42
already refer to completed rig preparation and retirement work in this tree.
This task uses PRF-46 and PRF-50 through PRF-56 for the runtime work instead.
RND-62 covers the medium tree band and HST-57 covers first person heading.

## Before and after evidence

All installed scenarios extend `godot/walkthrough.gd`, starting in the
Bannered Mare and entering Whiterun. Stock Godot 4.4.1 runs on Xvfb :99 with
llvmpipe for rendered timings. Player view checks use stock headless Godot.
The release extension includes all fixes. Tracked scenario processes exited
and their results were collected before any extension replacement.

| Check | Reproduced failure | Finished confirmation |
|---|---|---|
| Game time divided by wall time | Inn 0.33525, exterior 0.02654 | Inn 1.00184, exterior 1.00193 |
| Door route streaming | 139.294 ms, later movable load 43.680 ms | Preparation 26.374 ms, walking 26.909 ms |
| Grounded broader traversal | Streaming 252.595 ms and removal 247.823 ms | Streaming 27.641 and 29.606 ms, removal at most 0.595 ms |
| Distant selection on frame path | 94.054 ms | 0.051 ms |
| Medium tree coverage | 715 missing of 850 archived trees | Zero missing, zero near billboard duplicates |
| First-use full-tree pipelines | Three surface compilations per species | Zero mesh, surface and draw compilations for pine and aspen |
| Camera speed-error time | Inn 38.550 percent | Inn 9.008 percent, exterior 23.349 percent |
| First person heading error | 179.858 degrees while moving | Inn 0.0000071 degrees, exterior 0.0000025 degrees |

The streaming limit remains 33 ms. Both broader traversals gather 360 frames,
travel 5,680 and 5,998 horizontal units, cross cell boundaries and finish on
collision ground. Worst physics synchronization is 15.468 and 18.715 ms, with
one physics callback per frame. The door route's exterior samples are all
grounded. The old probe could fall while meeting its travel assertion. The
fixture now chooses collision support and requires a grounded finish while
retaining its other assertions.

The medium band uses archived BTT billboard placements grouped by source cell.
Near coverage hides only the cells with full geometry. The previous whole
four-by-four block exclusion left the reported gap. The survey records 190
archived blocks, 2,131 cell instance RIDs, 103,792 triangles and 7,888,192
geometry bytes. It adds no scene nodes per reference. Worker preparation of
26,352 billboards took 84.848 ms in aggregate, about 3.22 microseconds per tree.
The largest source cell took 12.813 ms and the shared atlas took 3.540 ms.

The full-tree diagnostic now creates both ArrayMesh and MultiMesh instances.
Pine synchronous model preparation and upload takes 45.064 ms plus 0.077 ms
instance creation for 3,734 vertices. Aspen takes 24.841 plus 0.106 ms for
2,758 vertices. Neither compiles a mesh, surface or draw pipeline. Each reports
two background specializations. These synchronous diagnostic model totals are
separate from production worker preparation and streaming frame cost.

The camera fixture observes after node processing, sets physics to 10 Hz and
caps rendering at 120 FPS. It keeps the fixed 25 percent speed-error-time
limit, 25 percent speed-error magnitude and 10 percent stationary limit. It
samples for at least 1.5 seconds and sixty frames. The exterior setup reaches
the flat plaza before measuring constant horizontal speed. Inn and exterior
stationary fractions are 4.94 and 3.125 percent. Timestamp history supplies
render interpolation even when callbacks arrive at uneven wall intervals.

The first person fixture keeps its fixed one-degree limit and confirms motion
in all four inn directions. The exterior segment moves 331 units and finishes
grounded. Reference attachment alone passed before the fix, so the new fixture
observes the animated camera bone while locomotion clips run. Placement now
cancels that bone's current transform. Hand animation remains relative to it.
The existing eye-height, hidden-body and camera-attachment fixture also passes.

An early camera baseline sampled before node processing and was discarded.
An exterior heading attempt reached the separate obstruction during warmup
and then failed its movement assertion. The final fixture measures the clear
segment before that obstruction and retains its movement assertion. Neither
failed setup is presented as a passing result.

## Checks and retained artifacts

Each source milestone passed package formatting, all-target Clippy with
warnings denied and its crate's full library suite before commit. The final
host suite has 600 passing cases and physics has 32. Installed cases return
early when their data variables are absent, so the library totals do not
represent 632 installed scenarios. Installed confirmations were run separately.
The release build and project import exited zero. No assertion or numeric
budget was weakened, and no test was deleted or ignored.

The combined branch receives the same scoped checks:

```text
cargo fmt -p soe-godot-host -p soe-physics
cargo clippy -p soe-godot-host --all-targets -- -D warnings
cargo clippy -p soe-physics --all-targets -- -D warnings
cargo test -p soe-godot-host --lib
cargo test -p soe-physics --lib
```

Markdown lint also passes with zero issues. The workspace gate and workspace
tests are excluded by this task's explicit instructions. All-feature and
no-default-feature workspace checks, comment-lint, Taplo and fuzz formatting
are outside this scoped gate. None is reported as a pass or an absent tool.

Retained reports and traces are local build artifacts:

- `target/r4-runtime/final`: two grounded routes, door and tree coverage,
  full-tree pipelines and the existing attachment check
- `target/r4-runtime/view-final/motion.json`: final inn and exterior camera data
- `target/r4-runtime/rig-final/rig-clear.json`: final animated rig data
- `target/prf40-after`: reproduced grounded retirement failures and traces

## Local sources used

The sources are under
`/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs`.

- TES5Edit `Core/wbLOD.pas` and UESP `TREE.wikitext` describe archived tree
  blocks and records. The existing BTT decoder supplies the placements.
- Mudcrab `crates/engine/src/streaming/lod.rs` supplies the ready-coverage
  handoff precedent. The host keeps its existing planner and worker queues.
- OpenMW `apps/openmw/mwphysics/physicssystem.cpp` and Jolt broad-phase and
  convex-hull sources establish per-object spatial ownership and hull work.
  Existing Rust query geometry is retained because it serves the deterministic
  controller independently of Godot. No second physics library is introduced.
- Godot's local Engine, pipeline compilation and advanced physics interpolation
  documentation explains capped callback time, pipeline counters and pose
  interpolation. The wall-time controller needs timestamped poses rather than
  Godot's fixed-tick fraction.
- OpenMW `camera.cpp` and CommonLibSSE-NG `FirstPersonState.h` separate view
  attachment from player geometry. Existing skeleton transform math supplies
  the correction here.

The preview3 terrain, river and floating tree evidence images were inspected.
No dependency, persisted field, format-crate move or per-reference node path
was added. No game binary was read.

## Limits and task boundaries

The grounded routes still record worst total frames of 456.959 and 450.693 ms
under llvmpipe. Render submission peaks at 43.994 and 34.002 ms. Streaming
below 33 ms does not imply total frames below 33 ms. Real GPU play confirmation
remains unverified under the task's prohibition on GPU launches.

The broader traversal also records sixteen surface compilations outside the
isolated tree probe. Zero draw compilations pass its existing acceptance.
The isolated tree result establishes only tree pipeline warmup. The optional
threaded simulation path has no installed confirmation in this task.

NPC movement, the giant, the separate exterior obstruction, terrain and water
changes from other branches, and the console are outside this worktree's task.
The obstruction was observed while preparing the exterior rig test and was
left for its assigned task. This branch is not merged or removed here.

## Files touched after the inherited commits

- `BACKLOG.csv`
- `crates/godot-host/src/batch.rs`
- `crates/godot-host/src/camera.rs`
- `crates/godot-host/src/data_source/lod.rs`
- `crates/godot-host/src/distant_visibility.rs`
- `crates/godot-host/src/godot_actor.rs`
- `crates/godot-host/src/godot_layer.rs`
- `crates/godot-host/src/godot_layer/distant_upload.rs`
- `crates/godot-host/src/godot_layer/upload.rs`
- `crates/godot-host/src/godot_physics.rs`
- `crates/godot-host/src/godot_player.rs`
- `crates/godot-host/src/godot_player/exterior.rs`
- `crates/godot-host/src/godot_player/gameplay.rs`
- `crates/godot-host/src/godot_player/simulation.rs`
- `crates/godot-host/src/lod_rebuild.rs`
- `crates/godot-host/src/perf_exterior_tests.rs`
- `crates/godot-host/src/runtime_clock.rs`
- `crates/godot-host/src/streaming.rs`
- `crates/godot-host/src/terrain_lod.rs`
- `crates/godot-host/src/tree_lod.rs`
- `crates/physics/src/world.rs`
- `docs/godot-host.md`
- `docs/performance.md`
- `docs/physics.md`
- `godot/prf_40_exterior_test.gd`
- `godot/r4_first_person_test.gd`
- `godot/r4_first_person_test.gd.uid`
- `godot/r4_player_motion_test.gd`
- `godot/r4_player_motion_test.gd.uid`
- `godot/r4_streaming_test.gd`
- `godot/r4_streaming_test.gd.uid`
- `godot/r4_tree_pipeline_test.gd`
- `godot/r4_tree_pipeline_test.gd.uid`
- `godot/r4_tree_test.gd`
- `godot/r4_tree_test.gd.uid`
- `docs/r4-runtime-report.md`
