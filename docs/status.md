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

## Implemented foundations and their limits

| Subsystem | Evidence at the reviewed commit | Remaining boundary |
|---|---|---|
| Records and assets | Load-order-aware plugin reader, resolved forms, winning overrides, cell index, archives and loose-file overlay, meshes and DDS. Deleted references resolve disabled, deleted navmeshes are removed with edge repair. | Format support and synthetic tests do not establish compatibility with every mod loadout. |
| Scripts and quests | Papyrus VM, scheduler, state serialization and native registry. The recorded five-master startup runs 423 quests through 1,388 native calls with zero unresolved calls. | The registry includes stubs. Startup coverage does not prove quest completion, combat, dialogue or magic. |
| Actors and AI | NPC base state, navmesh, sandbox/travel/sleep packages, behavior interpreter and its second pass. Recorded package resolution: 4,635 of 5,961 vanilla packages. | Remaining procedure trees and procedures, package fragments, interruptions and combat behavior are unfinished. |
| Rendering | Batched RenderingServer instances, compressed texture upload and session cache, interior/exterior streaming, terrain geometry, skinned characters with bind-pose and partition-index fixes. | LOD consumption, terrain texture layers, water and further exterior work remain. Rendering demonstrations are not the complete game loop. |
| Physics and input | Headless collision/controller, a Godot Jolt path, first/third-person camera and keyboard/mouse player node. Vanilla control-map parsing has live-data evidence. | The new PlayerNode uses SimpleWorld. Its Godot execution was not tested in the input report. Gamepad columns are parsed but unused. |
| Doors | Simulation activation, cell transitions, prefetch/attach split, doorway commit and deferred unload are merged. | The host activate action only records a ray hit. It does not drive arm_doorway, commit_doorway or later unload settlement. Cold-texture fallback is unmeasured. |
| Audio | FUZ/xWMA and open audio decoding, viseme generation, host SoundBank and conversion into Godot audio buffers. | Speaker output was unverified. The game loop does not drive footsteps or dialogue playback. Original LIP decoding remains incomplete. |
| Saves | Engine-owned world/VM save-load paths and a vanilla ESS reader, with real-save corpus results in save.md. | Reading a vanilla save is not an end-to-end resume of its gameplay and VM state. |
| Mod data | SPID, KID, BOS and OAR parsing. Game-side application exists for distribution and swapping. | OAR trees are counted but not consumed by animation. Some distribution types are held but unused, level/trait-filtered rules are skipped, and chance differs from the original plugins. No real mod has validated this path yet. |
| Determinism and testing | State hashes, repeated-seed checks, parser fuzzing and three phases of adversarial testing. | Full input record/replay, wider corpus coverage, fault cases, soak and CI performance work remain. |
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

The latest feature merges are `playback` (`5b1a9ed`), `no-load-screen`
(`35a5dd7`) and `input` (`75cfb55`). They are no longer in flight.
No unmerged local branch or extra worktree existed before this documentation
task created its own review branch.

The next recommended integration milestone is a verified Bannered Mare
walkthrough: movement and camera, interaction mapped to a reference, a door
crossing with correct events and deferred unload, and audible footsteps.
This is proposed work, not a claim that the milestone is complete.

Combat, magic, dialogue, inventory/menu interaction, crafting, progression,
quest conditions and aliases still need explicit implementation scope and
done checks. The plan mentions a future quest loop but does not yet decompose
the full gameplay layer. Finishing the listed engine tasks alone does not
finish Skyrim. Portal doorways remain an unimplemented prototype proposal.
