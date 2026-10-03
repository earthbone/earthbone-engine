# Status, per subsystem

## Checkpoint, 2026-10-03, backlog run round 3

The lead refreshes this section after every backlog round. The table comes from
`.soe-run-tools/status_table.py` over BACKLOG.csv. Open rows carry their
remaining work in the evidence column, partial rows name the open clause.

| Area | Done | Open | Blocked |
|---|---:|---:|---:|
| Papyrus natives (SCR) | 43 | 30 | 0 |
| Saves (SAV) | 55 | 3 | 11 |
| Animation (ANM) | 51 | 8 | 0 |
| Magic (MAG) | 65 | 5 | 3 |
| CommonLib shim (SHM) | 15 | 8 | 0 |
| Playtest bots (PLY) | 0 | 7 | 0 |
| Test harness (TST) | 10 | 7 | 0 |
| AI packages, schedules, detection (AI) | 86 | 1 | 4 |
| Godot host (HST) | 14 | 5 | 0 |
| Menus and HUD (UI) | 13 | 5 | 0 |
| Audio (AUD) | 6 | 4 | 0 |
| Progression, perks, skills, leveled lists (PRG) | 17 | 3 | 1 |
| Combat (CMB) | 35 | 2 | 1 |
| Editor (EDT) | 25 | 3 | 0 |
| File formats (FMT) | 28 | 2 | 0 |
| Mod data, SPID, KID, BOS, OAR (MOD) | 8 | 2 | 0 |
| Performance (PRF) | 8 | 2 | 0 |
| Rendering (RND) | 14 | 1 | 1 |
| Tools (TOOL) | 3 | 2 | 0 |
| Crafting (CRF) | 8 | 0 | 0 |
| Dialogue (DLG) | 21 | 0 | 0 |
| Inventory (INV) | 8 | 0 | 0 |
| **All 655 rows** | **533** | **100** | **21** |

### What runs today

`godot/walkthrough.gd` is the playable prototype. With `SKYRIM_DATA` set and
the release extension built, it loads the installed plugins and archives,
streams Whiterun's Bannered Mare and places the player there: WASD moves, the
mouse looks, E activates, Space jumps, Ctrl sneaks, F switches the camera.
`PlayerNode::start_gameplay` starts the game VM over the same records and
archives the renderer uses. Every frame `Game::tick` (`crates/game/src/lib.rs`)
runs AI packages, activation, force greets, dialogue, magic, scene actions,
quest lifecycle tasks and menu mode.

The host also has the native menu framework, the dialogue menu (DLG-10),
footsteps, dialogue voice playback, ranged release and projectile flight
(CMB-08, CMB-22), death ragdolls (CMB-19), water, grass, rock instancing and
distant terrain LOD (RND-02, RND-11, RND-04, RND-03). It saves and reloads its
own native save generations (`save_game_generation`,
`recover_game_generation`, SAV-63). Merged but partial, with the open clause
in the backlog row: the main menu lifecycle (UI-18), music (AUD-05), stagger
(CMB-27) and attaching a restored player (SAV-19, blocked). See
[godot-host](godot-host.md) for each feature's evidence.

### What does not run yet

- No vanilla save loads from the walkthrough. The host can convert a `.ess`
  into one of its own save generations (`accept_vanilla_recovery_session` in
  `crates/godot-host/src/save_recovery.rs`, SAV-61), refusing a save whose
  PCON or PARW change forms it cannot read, and `attach_restored_generation`
  resumes from a native generation. No GDScript call opens a `.ess` yet, and
  SAV-19 (resume the host from a restored vanilla game) still lacks the test
  that connects the two.
- No new game starts. The walkthrough always begins in the Bannered Mare, and
  the host has no new-game path.
- 30 Papyrus native rows are open. The SCR rows in BACKLOG.csv name each
  native and what its implementation still lacks.
- The playtest bots (PLY-01 to PLY-07) are drafted, not merged.
- Checks only the user can run are pending: the TOOL-04 probe capture in the
  original game, HST-05 on a physical speaker, PRF-08 windowed cold starts and
  a SAV-65 crafting save. The kit is in `.soe-run-tools/session/`.

The sections below predate the backlog run and are no longer maintained. Where
they disagree with BACKLOG.csv or a subsystem doc, the backlog and that doc
are current.

## Earlier status, before the backlog run

The completed combat integration adds difficulty selection, weapon and armor
templates, armor slot replacement, race-based unarmed damage and the authored
melee contact bridge.
SOEW version 18 combines difficulty with hand assignments and reads the
distinct version 16 and 17 branch layouts. The checkpoint before unarmed
integration passed 1,191 Rust tests and 11 Python tests. The installed record survey executes 195 supported
weapons. BACKLOG.csv records the completed rows and their prerequisites.
Live actor collision synchronization, host attack input, bleedout and combat
AI remain unfinished. See [combat](combat.md) for evidence and limits.

The unmerged `ess-quest-restore` follow-up applies QUST stages, objectives,
aliases and journal instances without replaying quest events. SOEW version 11
persists their mapped runtime state. The 60-save application survey restores
32,256 quests. Missing world identities, lifecycle tasks, VM bindings and saved
execution remain open. See [save.md](save.md#applying-saved-quests).

The `ess-world-bindings` follow-up adds saved plugin-name mapping and atomic
global-variable restoration into live GLOB records, including the engine-defined
PlayCredits form. Actor/quest state, calendar and scheduler clocks, Papyrus
object bindings, world change forms and saved execution remain unfinished.

The `ess-quest-state` follow-up decodes QUST objectives, runtime aliases and
story events, quest instances and global quest-static data. Across 60 saves,
all 32,256 quest payloads and all quest-static blocks reproduce their original
bytes from typed fields. Applying this state to live quest execution remains
unfinished.

The combat attribute milestone adds actor-value components, corrected
NPC stat offsets and race/template attribute sources. Its installed-data
survey resolved 4,718 of 6,361 NPC bases, with 1,643 leveled templates pending
spawn selection. This is a combat prerequisite, not weapon-hit execution.
See [actor values](actor-values.md) for sources, evidence and remaining work.

## Integrated execution milestones, 2026-09-20

The integration combines combat attributes, crafting execution, dialogue,
active magic, host gameplay and both vanilla-save groundwork branches.
The native registry has 366 stateful entries and 179 stubs. These are
incremental implementations, not completion of Skyrim gameplay.

| Area | Integrated behavior | Still open |
|---|---|---|
| Combat foundation | Separate actor-value components, corrected NPC fields, race and stats/traits template sources | Weapon hits, equipment damage rules, live leveling, leveled templates and combat AI |
| Crafting | Condition-checked COBJ transactions and reference/alias inventory events | Tempering, alchemy, enchanting, item instances, progression and menus |
| Dialogue | Record-backed choices, conditions, responses, named fragments, history, saved cursor, blocking and exclusive branches | Host menus, text localization, voice/LIP, scenes, services and remaining conditions/branch modes |
| Magic | Direct non-hostile applications, effect script objects, expiry, dispel and saved state | Hostile hits, projectiles, concentration, costs, many archetypes, player input and visuals |
| Host | Shared Game/VM, reference activation, locked doors and keys, load-door round trips and material-selected footsteps | Swing-door animation/collision, lockpicking, walking exterior streaming and physical speaker verification |
| Vanilla saves | Typed execution parsing and explicit-mapping import of instances, references and arrays | Binding to live world forms/aliases/effects, stack and latent-call resume, world change forms and complete ESS restoration |

SOEW version 9 combines actor-value components, dialogue and active effects.
Cross-branch tests cover inherited conditional variables, modifier preservation,
and reloading an active conversation and buff without replaying either.
Legacy saves with ambiguous actor-value modifiers return an explicit error.
See [game.md](game.md), [save.md](save.md) and
[execution-plan.md](execution-plan.md) for contracts and remaining work.

Combined installed-data validation passes 65 game tests and nine save tests.
The 60 save bodies round-trip byte for byte. Object import restores 2,649,854
instance/reference entries and 19,194 arrays, without resuming saved stacks.
Startup reaches 423 quests and 1,490 native calls with zero unresolved calls.
Godot 4.7.2 passes the Bannered Mare door round trip, landing collision and
E-key activation, plus grounded footstep playback through the audio mixer.
These are headless checks, not rendered-image or physical speaker verification.

The actor-templates follow-up adds persistent leveled selections and correct
source ownership for runtime NPC groups. All 6,361 installed NPC bases initialize
at selection levels 1, 20 and 50 and survive a population save/load check.
SOEW version 10 persists these choices. Encounter-zone difficulty/reset rules,
generated ActorBase identity and remaining template consumers are still open.
See [actor-values.md](actor-values.md#per-reference-actor-templates-2026-09-20).

The reference-placement follow-up decodes REFR/ACHR prefixes and reconstructs
identities, bases and transforms in a fresh world table. It restores player
location and reserves created IDs without dispatching events. Inventory, actor
state, saved masks, extra data and Papyrus execution remain unapplied. This is
one pass of reconstruction, not a complete ESS loader.
Across 60 saves it restores 1,154,526 placements. Applying globals and quests
afterward leaves zero unresolved quest identities. Missing cell lookups and
unplaced references remain reported, and saved gameplay is not resumed.

## Earlier baseline, 2026-09-19

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
| Audio | FUZ/xWMA and open audio decoding, viseme generation, host SoundBank and conversion into Godot audio buffers. Player movement drives footsteps, and selected dialogue responses resolve voice paths and call SoundBank. | Speaker output and end-to-end dialogue playback remain unverified. Original LIP decoding remains incomplete. |
| Saves | Engine-owned world/VM save-load paths and a vanilla ESS reader, with real-save corpus results in save.md. | Reading a vanilla save is not an end-to-end resume of its gameplay and VM state. |
| Mod data | SPID, KID and BOS game-side application, plus OAR condition and priority selection. | OAR selects supported replacement paths but does not apply them to live animation playback. Some distribution types are held but unused, level/trait-filtered rules are skipped, and chance differs from the original plugins. No real mod has validated this path yet. |
| Determinism and testing | State hashes, repeated-seed checks, parser fuzzing and three phases of adversarial testing. | Full input record/replay, wider corpus coverage and fault cases remain. The performance-limits harness adds CPU/Godot sweeps, bounded soaks and synthetic CI checks. See performance-limits.md for executed runs and remaining integration limits. |
| Compatibility API and tools | MIT CommonLib-shaped Rust skeleton, generated C++ header subset, synthetic module loader fixture, debug/harness commands and editor design. | No surveyed plugin has compiled or loaded against the shim. The C++ declarations have no engine binding. No implemented Creation Kit replacement, player UI or MCM layer. WASM is a planned extension path. |

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

The registry now reports 345 stateful natives and 191 stubs. SOEV version 3
retains version 1 and 2 reading, distinct wait clocks and variable slots keyed
by declaring script. SOEW is version 4.
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

## Dialogue source milestone, 2026-09-20

The `dialogue-execution` source milestone adds a record-backed conversation
API, topic/quest ownership, conditions, response sequencing, named INFO
fragment calls, shared responses, links and engine-save state. Its native
registry reports 348 stateful entries and 188 stubs. See [game.md](game.md)
for source attribution, tests and unsupported behavior. Earlier tables above
remain the recorded master baseline.

The five-master survey covers 40,678 INFO records, 7,799 fragments and 19,368
ordered topic groups. An authored Hulda topic reaches response completion.
This is headless evidence. Menus, localized text, voice playback, greetings,
services, several branch modes and missing conditions remain unfinished.
Combat, full magic and crafting, host integration and full vanilla-save
restoration remain part of the active objective. Work on other unmerged
branches is not evidence that their combined runtime has passed the gate.

## Active magic source milestone

The `magic-effects` source milestone implements direct non-hostile Script,
Value Modifier and Dual Value Modifier applications, per-application script
objects, duration and expiry, effect queries, dispelling and SOEW version 5
persistence with version 4 reading. Its registry has 353 stateful natives and
183 stubs. These counts describe this worktree, not master.

Hostile spells, concentration, projectiles, conditions, peak modifiers and
other archetypes, learned abilities, enchantments, potions, player casting input
and audiovisual effects remain unfinished. See [game.md](game.md#active-magic-applications)
for the exact supported path and reference sources.

## Inventory worktree, unmerged

The inventory follow-up replaces quantity-only reference inventory with item
totals and distinct stack metadata. Transfers, crafting and SOEW version 12
preserve that state. The ESS inventory application validates an entire pass
before mutation and maps stack references through saved plugin identities.
The full reference-contents corpus test still requires zero decoder failures.
Nonempty PackageData remains unsupported, so full vanilla restoration is not
complete. This work is not part of the master integration reported above.

Validation passes the full gate with 1,114 Rust tests and 11 Python tests,
28 installed-data game-save tests, and 17 quest/header decoder tests. The
non-actor application survey restores and persists 59,854 inventories across
60 saves. The broader reference-contents decoder still rejects 169 kind-18
PackageData payloads. That failing test remains a required unfinished check.

The same worktree now restores all 156,169 saved actor-template choices across
60 saves without consuming RNG. Those choices persist in SOEW version 12,
including an explicitly unknown original encounter level. The actor-inventory
diagnostic applies 114,696 decodable inventories and preserves their stack
state. It explicitly excludes 126 inventory-bearing actors blocked by
PackageData. Inline NPC changes on 4,930 actors, temporary ActorBase identities,
complete actor state and all package execution restoration remain unfinished.

The combined follow-up gate passes 1,120 Rust tests and 11 Python tests. All
33 game-side save tests pass with installed data. Optional gate tools ran.
The normal comment-lint clone index reported its existing disk I/O warning,
while a separate fresh-index run passed. The full reference-contents corpus
check still fails on all 169 unsupported PackageData records. No worktree
changes in this follow-up have been committed or merged.

## NPC save changes, work in progress on ess-npc-changes

The save crate decodes complete NPC_ changes and exposes the same named fields
for embedded changes. The game applies standalone NPC changes to shared
record overrides before initializing references and actors. All 13,362 NPC
bases across 60 saves restore and persist through engine save/load, with no
missing mapped links or pending form-flag masks in that corpus. Synthetic
checks verify live stats, race, class, factions, spells, shouts, outfits, names
and a Papyrus query, plus atomic rejection and load-order changes.

SOEW version 13 preserves the overrides, mapped links, original race, exact
body-tint bytes and any pending generic form-flag masks. Versions 4 through 12
remain readable. The full installed-data vanilla-save suite passes 38 tests,
including NPC restoration before placement/template/inventory surveys. The
actor inventory survey still excludes 126 package-bearing records. The
separate complete-contents test still fails on 169 package payloads.

The combined gate passes with 1,134 Rust tests and 11 Python tests. All optional
gate tools ran. These changes remain uncommitted on `ess-npc-changes`.

Both embedded-NPC paths now use the same named decoder. All 156,169 leveled
entries re-encode exactly. The 4,930 nonempty changes include four ACBS-only
payloads and 4,926 ACBS/AIDT/DNAM payloads. The nine NPC decoder tests pass,
including the external corpus. The repeated strict contents survey still
fails only on the same 169 PackageData payloads. Runtime inline application
is covered by the temporary-base follow-up below.

This worktree includes the frozen inventory diff as a prerequisite. The
original review worktree remains unchanged.

The temporary-base follow-up creates a private NPC record for every saved
leveled actor and applies its inline changes before actor initialization.
GetActorBase and GetLeveledActorBase now expose the authored and private
identities separately. Inherited links keep their source namespaces, and
footsteps read restored traits. SOEW version 14 persists the private records,
their origins and their owning actors while retaining older-save loading.
World-state maps serialize in sorted order so load/save preserves byte order.

The combined gate passes with 1,139 Rust tests and 11 Python tests, with no
optional tools skipped and no warnings. All 42 game-side save tests and nine
footstep tests pass with installed data. The 60-save corpus creates and
persists 156,169 temporary bases and applies all 4,930 inline changes.
The frozen 41-file inventory review tree remains unchanged.

Temporary-base cleanup and saved Papyrus rebinding, generic form-flag
application, rendered appearance, leveled spell execution and saved actor/AI
execution remain unfinished. The complete-contents corpus test still fails
on 169 PackageData records. These results do not establish a complete ESS
load or a resumed game.

## Combat lifecycle worktree, unmerged

The combat-lifecycle increment builds on the frozen 57-file NPC/inventory
review. It adds persistent deferred death requests, killer attribution,
OnDying/OnDeath delivery to actor, alias and active-effect scripts, and essential
base-flag propagation. SOEW version 15 retains undelivered notifications.
The combined full gate passes 1,151 Rust tests and 11 Python tests, with no
warnings or skipped optional tools. Installed-data validation passes 11
lifecycle tests, 42 game-side save tests and nine footstep tests. The lifecycle
survey initializes 6,361 actors, records 6,018 deaths and retains 343 essential
actors. All surveyed death state and queued events persist through save/load.

Weapon hits, damage formulas, bleedout, animation timing and combat AI remain
unfinished. KillSilent event scope and deferred death after healing still need
comparison with the original runtime. See [actor values](actor-values.md#death-lifecycle-increment-unmerged).
No changes in this worktree have been committed or merged.

## Ordinary melee worktree, unmerged

The combat-hits increment evaluates ordinary physical contacts from winning
weapon/armor records, current skills, equipment and game settings, then queues
OnHit before any death notifications. It builds on the frozen lifecycle review.
Host hit detection, special attacks, perks and full combat remain unfinished.
See [physical melee execution](combat.md) for supported rules and sources.
The combined full gate passes 1,163 Rust tests and 11 Python tests, with no
warnings or skipped optional tools. Installed-data validation passes 12 melee
and 11 death-lifecycle tests. It decodes 3,262 winning weapons and 3,675 armors,
then executes 184 supported weapons against synthetic actors. The remaining
weapons reject unsupported types, templates or enchantments. Real armor records
were decoded, while armor damage calculations use synthetic fixtures.
These changes await commit and merge review on `combat-hits`.

## Remaining branch integration

Master e9cc2f9 contains the inventory/NPC, death-lifecycle and ordinary melee
increments described above. Their old unmerged labels record the review state
at the time of those measurements. The inventory, NPC and lifecycle worktrees
are earlier snapshots of that result. All existing branch commits are merged.

The remaining-work-merge candidate brings over the independent empty
PackageData decoder from ess-package-state. Its original corpus run reduced
strict reference-content failures from 169 to 81, recovering 88 reference
sections and 73 inventories. Named procedure states remain unsupported.
See [saved package runtime state](save-package-data.md) for the integration
results and remaining limitations. Broader implementation work is paused.

The combined candidate passes 1,165 Rust and 11 Python gate tests, with no
warnings or skipped optional tools. All 42 installed-data save-restoration
tests pass. The actor inventory survey now restores 114,769 inventories and
excludes 53 unsupported records, compared with 126 before this increment.
The separate strict decoder test remains failing on the 81 named states.

## LIP body decoder, AUD-01 on lip-decode

The audio crate now decodes native frame-indexed LIP weight tracks, including
zero-run compression, stored target counts and additional gesture channels.
The installed English archive test passes all 74,070 nonempty tracks, totaling
9,566,160 frames and 311,857,205 samples. Another 1,338 payloads are empty.
Hand-authored raw and compressed fixtures verify exact sample weights and
malformed-input rejection. See [audio.md](audio.md#lip-body-decoding-aud-01).

This updates the older table's statement that the original LIP body was
undecoded. Playback seconds, interpolation and named facial morph binding
belong to ANM-13 and DLG-20 and are not implemented by this decoder.
Fixed-target and big-endian variants remain explicitly unsupported.

## Skin override assembly (INV-08)

NPC and race WNAM setters now drive a shared body-part view for footsteps and
host rendering. Four SKSE skin natives use canonical record overrides, including
save restoration of race skins. Skin forms never become carried inventory.
An existing loaded character node can bind to PlayerNode's gameplay world and
refresh authored addon surfaces when skin or equipment changes. The host keeps
its loaded animation rig and uses RIDs for replacement geometry.

## Checkpoint, integrated execution milestones, 2026-09-20

The 2026-09-20 integration combines actor-value components and corrected
race/template sources, crafting transactions and inventory events, dialogue
selection and fragments, direct non-hostile magic, host doors and footsteps,
and vanilla-save execution parsing and object/array import groundwork.
The native registry has 361 stateful entries and 180 stubs. SOEW version 9
combines actor-value components, dialogue and active effects. SOEV version 3
retains declaring-script variable identity.

PlayerNode now drives Game/VM activation, locked-door key checks and the
load-door lifecycle with rendering/collision replacement. Grounded movement
selects footstep sounds from records and reaches the Godot mixer. Physical
speaker output, swinging-door collision and exterior walking streaming remain
unverified or unfinished. Portal city transitions and revised AI are deferred.

Weapon combat, complete magic/crafting/dialogue, player UI and full vanilla-save
restoration remain open. ESS import requires explicit object mapping and does
not resume saved stacks or restore all world change forms. The CommonLib shim
is a skeleton and the editor is a design. Read docs/status.md and
docs/execution-plan.md for current evidence and the remaining scopes.

## Checkpoint, reference corrections, 2026-09-19

Based on master `75cfb55`, with the reference fixes in this change. The full
gate reports 835 test passes and every lint tool ran. The runtime registers
345 stateful natives and 191 stubs. Waits have distinct gameplay, menu and
game-hour clocks, persisted in SOEV version 2 with version 1 reading retained.
Quest getters agree on the highest stage. Dispel calls preserve learned and
equipped spells pending active effects. SPID deterministic fractional rules
are reported as unsupported, with their percentage and marker preserved.

The host keeps alpha and culling in material identity while sharing uploads,
and sends transformed native collision shapes to Jolt. The installed-game
startup check and Bannered Mare contact tests pass. Detailed evidence and
limits are in `docs/reference-audit.md` and `docs/physics.md`.

`docs/status.md` carries subsystem detail. `ENGINE-PLAN.md` and the reference
map now scope missing gameplay work and original-runtime probes. Those future
systems are pending, not delivered by the reference fixes.

Adversarial testing has reached phase 3. The archive decompression-size
mismatch and the PEX assembler's acceptance of duplicate function names
were fixed. Evidence, limits and repros remain in `TEST_LOG.md`.

A number falling can be a fix. Reading `XLKR` on actor references closed a
real gap, the `ACHR` reader had ignored linked references, and the first
start's native calls fell from 1,560 to 1,388 because scripts that used to
get `None` from `GetLinkedRef` now get the reference and take a shorter
branch. Same quests, same log lines. Check the cause before reading a drop
as a regression.

Known gaps: no player UI or MCM layer, no complete combat, magic or dialogue,
no LOD consumption, no terrain texture layers or water. The CommonLib shim
is a Rust skeleton without generated C++ headers or a compiled plugin.
The editor remains a design. The reference audit scopes pending gameplay
work, with combat, crafting and progression still requiring separate items.
No Proton launch has been reported.
Audio exists with the integration limits above. Open risks are in map section
12, read together with the later decisions in `ENGINE-PLAN.md`.
