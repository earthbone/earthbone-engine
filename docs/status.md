# Status, per subsystem

Reconciled on 2026-09-19 against engine commit `75cfb55` on `master`.
This is a source and history review. The full gate, Godot sessions and live
corpus tests were not rerun for this documentation update. Numbers below are
recorded results from September 18, not new measurements. A subsequent
pre-commit gate on September 19 passed at this code baseline, including all
three clippy passes and tier 1 tests. It used `CCACHE_DISABLE=1` and a writable
temporary `XDG_DATA_HOME` because the sandbox cannot write the default tool
caches. This does not add any live-game or Godot runtime verification.

The engine has working readers, simulation and rendered demonstrations.
An integrated, verified walkthrough is still pending, and Skyrim gameplay
is incomplete. See [intent.md](intent.md) for the goal and compatibility
boundaries, and `ENGINE-PLAN.md` for the historical plan and remaining work.

The `host-player-run` follow-up adds a movement-only walkthrough and executes
PlayerNode in Godot. Input/camera checks pass on 4.4.1 and 4.7.2. A live-data
4.7.2 run settles the player on 281 Bannered Mare collision bodies and walks
about 90 units in half a second. The exact PC control map and its DirectInput
codes replace an incorrect console-map-based interpretation. See the host
doc for the test commands and remaining integration limits.

## Implemented foundations and their limits

| Subsystem | Evidence at the reviewed commit | Remaining boundary |
|---|---|---|
| Records and assets | Load-order-aware plugin reader, resolved forms, winning overrides, cell index, archives and loose-file overlay, meshes and DDS. Deleted references resolve disabled, deleted navmeshes are removed with edge repair. | Format support and synthetic tests do not establish compatibility with every mod loadout. |
| Scripts and quests | Papyrus VM, scheduler, state serialization and native registry. The recorded five-master startup runs 423 quests through 1,388 native calls with zero unresolved calls. | The registry includes stubs. Startup coverage does not prove quest completion, combat, dialogue or magic. |
| Actors and AI | NPC base state, navmesh, sandbox/travel/sleep packages, behavior interpreter and its second pass. Recorded package resolution: 4,635 of 5,961 vanilla packages. | Remaining procedure trees and procedures, package fragments, interruptions and combat behavior are unfinished. |
| Rendering | Batched RenderingServer instances, compressed texture upload and session cache, interior/exterior streaming, terrain geometry, skinned characters with bind-pose and partition-index fixes. | LOD consumption, terrain texture layers, water and further exterior work remain. Rendering demonstrations are not the complete game loop. |
| Physics and input | Headless collision/controller, a Godot Jolt path, first/third-person camera and keyboard, mouse and gamepad bindings. PlayerNode has live PC-map and cell evidence. Injected device tests pass on Godot 4.4.1 and 4.7.2. | PlayerNode uses SimpleWorld. Physical controller feel is unverified. The movement launcher has no gameplay VM, door activation or footsteps. |
| Doors | Simulation activation, cell transitions, prefetch/attach split, doorway commit and deferred unload are merged. | The host activate action only records a ray hit. It does not drive arm_doorway, commit_doorway or later unload settlement. Cold-texture fallback is unmeasured. |
| Audio | FUZ/xWMA and open audio decoding, viseme generation, host SoundBank and conversion into Godot audio buffers. | Speaker output was unverified. The game loop does not drive footsteps or dialogue playback. Original LIP decoding remains incomplete. |
| Saves | Engine-owned world/VM save-load paths and a vanilla ESS reader, with real-save corpus results in save.md. | Reading a vanilla save is not an end-to-end resume of its gameplay and VM state. |
| Mod data | SPID, KID, BOS and OAR parsing. Game-side application exists for distribution and swapping. | OAR trees are counted but not consumed by animation. Some distribution types are held but unused, level/trait-filtered rules are skipped, and chance differs from the original plugins. No real mod has validated this path yet. |
| Determinism and testing | State hashes, repeated-seed checks, parser fuzzing and three phases of adversarial testing. | Full input record/replay, wider corpus coverage and fault cases remain. The performance-limits harness adds CPU/Godot sweeps, bounded soaks and synthetic CI checks. See performance-limits.md for executed runs and remaining integration limits. |
| Compatibility API and tools | MIT CommonLib-shaped Rust skeleton, debug/harness commands, editor design. | No plugin has compiled against the shim and C++ headers do not exist. No implemented Creation Kit replacement, player UI or MCM layer. WASM is a planned extension path. |

The subsystem documents carry details and evidence: [game](game.md),
[godot-host](godot-host.md), [audio](audio.md), [save](save.md),
[moddata](moddata.md), [shim](shim.md), [editor](editor.md), and
[performance](performance.md).

## Verification baseline

The pre-input handover recorded 750 tier 1 tests passing. The older CLAUDE
status recorded 711 tests at `15dd049`. Neither is a fresh gate result for
`75cfb55`, which includes the later input merge. Native counts also differ
between old summaries, so use the registry report for a new exact count.

Recorded rendering results include a Whiterun 3 by 3 scene p99 of 0.519 ms
and warm cell streaming near 4 ms. The streamed-door timings describe the
record/reference simulation work. They do not include a complete visible door
crossing, cold texture upload or all gameplay. Keep the benchmark conditions
in [performance.md](performance.md) with any quoted number.

`tools/lint.sh` defines the full commit gate, including all three clippy
passes and tier 1 tests with game/save environment variables unset. Real-game
tests use `SKYRIM_DATA`. Save tests use `SOE_TEST_SAVE` or `SOE_TEST_SAVES`
as required by the test. Record when these ran and when they skipped.

## Current stopping point

The player-host step `8485d48` and reference/audit work are integrated. The
host startup follow-up shares loaded data, includes container models, corrects
placed-model transforms and decal ordering, and tests camera/floor handling.
Earlier playback, transition and input branches are no longer in flight.
Recheck worktree ownership before starting work.

The next recommended integration milestone is a verified Bannered Mare
walkthrough: movement and camera, interaction mapped to a reference, a door
crossing with correct events and deferred unload, and audible footsteps.
This is proposed work, not a claim that the milestone is complete.

Combat, magic, dialogue, inventory/menu interaction, crafting, progression,
quest conditions and aliases remain incomplete. The reference audit supplies
pending acceptance checks, with combat, crafting and progression still needing
separate scopes. Finishing the substrate alone does not finish Skyrim.
Portal doorways remain an unimplemented prototype proposal.

## Reference and audit integration

`0c84bbe` corrects native and material contracts. `3f336e3` adds bounded
ordered workers, navigation and test-harness improvements. `6d990e7`
reconciles those branches. The reference-only gate recorded 835 passes,
with gated tests allowed to return early when their inputs were absent.
That is historical evidence, not the combined player-host gate.

The registry now reports 345 stateful natives and 191 stubs. SOEV version 2
retains version 1 reading and distinct wait clocks. SOEW is version 4.
Dispel stubs preserve learned and equipped spells. Deterministic fractional
SPID rules are reported as unsupported. Material keys preserve alpha and
culling while texture uploads stay shared. See [reference-audit.md](reference-audit.md)
for exact sources and pending gameplay acceptance checks.

The audit adds ordered worker results and shared texture caching. The renderer
now includes furniture and containers. The player shares the renderer's data
source, and startup opens it on a CPU worker. Placed-model transforms, rug
decal ordering and thin-floor snapping have follow-up fixes. These changes
improve the movement walkthrough. Activation, door crossing and footsteps
still require game-loop integration.

## Mechanical follow-ups validated on 2026-09-19

The unattended follow-up batch integrated these isolated, audited steps:

| Step | Evidence and boundary |
|---|---|
| Host startup, `5953010` | Shared data source, CPU loading, model transforms, container models, decal depth and thin-floor handling. Godot 4.4.1/4.7.2 checks and the 4.7.2 GPU decal regression passed. Live collision checks used 361 bodies. This remains a movement demonstration. |
| Navmesh funnel, `8d7641c` | Portal-based paths retain triangle ownership for AI. A 100-route installed-data survey found 99 shorter paths, including 50 cross-mesh routes. A* still selects its corridor using centroid costs. |
| Device bindings, `85bdd10` | Mouse/gamepad bindings, axes, chords, short presses, focus and disconnect handling. Injected event checks passed on both Godot versions. Physical controller feel remains untested. |
| Animation crate rename, `f4a45ec` | `havok` became `hkx`, with imports, fuzz targets and docs updated. The live survey read 7,699 packfiles and decoded all 6,126 animations. This renamed the crate, not the format. |
| Script coverage, `1636bb6` | Optional executed-function reporting by script and quest. Startup observed 323 of 54,023 loaded function bodies across 14,026 scripts and 2,274 quest records. Coverage is not gameplay completion. |
| ESS values, `b0ed3fe` | 60 saves yielded 15,900,669 Papyrus value slots and 19,194 arrays. Typed payloads re-encoded to exactly 211,771,719 original bytes. Stack bodies and suspended calls remain raw, with no vanilla VM restoration. |
| Gameplay views, `gameplay-views` | Read-only DIAL, INFO, SPEL, MGEF and COBJ fields and conditions. All 64,547 non-deleted winners in five masters decoded and 25,815 checked links resolved. Seven recipes have no output object. No dialogue, magic or crafting execution was added. |

Each step ran `tools/lint.sh` before commit, with the installed-data checks
reported in the subsystem documents. These are new measurements, separate
from the historical baseline above. The Bannered Mare gameplay walkthrough
still requires host activation/doorway wiring and footsteps. Record views,
function coverage and deeper save inspection do not close those integration
items or the larger gameplay scopes.
