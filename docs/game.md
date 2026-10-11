# soe-game: the runtime form table and the first natives

## Fixed simulation owner, PRF-37 draft

`simulation::Simulation` constructs Game and its native closures on one named
thread. A 60 Hz fixed clock drives world ticks independently of render
callbacks. `Config` defaults AI and script dispatch to 20 Hz. Fractional
cadence retains phase and accumulated elapsed time, including rates that do
not divide 60. Magic, timers and VM wait clocks advance on every fixed tick.
Menu commands pause world time and AI while menu wait clocks continue.
AI activation and greeting decisions follow the AI cadence. The existing
actor driver supplies its current process policy, without a second actor store.

Snapshots contain loaded actor placements, contact presentations, sky state,
package evaluation counts and tick/worker timings. Consecutive immutable
snapshots are published together with a nonblocking mutex exchange. A retained
reader cannot be overwritten. Contention keeps the previously published pair.
Interpolation clamps between those consecutive placements and wraps angles
through their shortest difference. Visibility changes use the current value.
The snapshot is presentation data, never an authoritative copy of World.

A bounded queue accepts 64 ordered commands and reports rejection when full.
The player pose mailbox coalesces finite poses without blocking. Poses apply
only in the current loaded cell. Initialization failures and panics have a
separate error channel. Shutdown wakes and joins the thread before releasing
its state. Lag combines overdue completion time with age since publication,
so a stalled publisher cannot continue reporting a healthy simulation.

`tick_scheduled` shares a Papyrus execution allowance across all dispatches
within a tick. The default worker allowance is 1.2 ms. AI and magic execution
do not spend it. Event pumps and natives already executing finish before the
next check. Exhaustion retains queued events and runnable stacks. The VM's
deadline runner checks between opcodes and retains a round-robin continuation.
The allowance, continuation cursor, worker and snapshots are transient and
add no saved fields or sections. Ordinary `tick` retains its existing
instruction-budget API.

The source boundary is local `godot-docs/tutorials/performance/thread_safe_apis.rst`
and `tutorials/scripting/idle_and_physics_processing.rst`. Papyrus budget
evidence is recorded in [threading.md](threading.md), including CommonLibSSE-NG
`include/RE/V/VirtualMachine.h` and the CK INI setting. The local CK dump does
not contain that INI page and a direct fetch returned HTTP 403. The fixed-rate
scheduler and snapshot exchange implement engine policy. They do not reproduce
vanilla tasklet locking. PRF-34 reference observers, PRF-35 magic caches and
PRF-36 process clocks remain their separate draft implementations.

## AI measurements, PRF-27

`AiWorld::metrics` counts each NPC driver visit, including early returns, and
records its inclusive time and mean interval on the non-menu real-seconds
clock. The four slots follow local CommonLibSSE-NG `AIProcess.h` PROCESS_TYPE
and `ProcessLists.h` actor arrays. The current driver visits every instantiated
NPC at high process each tick. Middle high, middle low and low remain empty.
These headers name process levels but do not supply scheduling intervals, so
this instrumentation does not introduce a distance-based scheduler.

Procedure phases cover every current `Procedure` variant. Unsupported
UseItemAt requests have a separate category. Immediate UseMagic branches are
timed around their actual cast path. Package selection, schedule windows,
combat target selection, assigned-hand spell and weapon selection, automatic
and assigned force greets, sandbox marker/furniture searches and steering
have named timings. Ambient idle chatter topic scans and candidate checks have
their own phase within automatic greeting evaluation. Durations are inclusive
and can overlap. Per-procedure
movement time includes state lookup and procedure-tree entry, sharing the
existing movement clock pair. Activation and immediate casts have separate
timings. Driver phase accumulation uses enum slots and publishes the named
table once per tick. Actor cadence and progress share one metrics lookup.
Menu and disabled-AI frames clear frame timings and process counts.
Combat style choice has no executing subsystem and reports zero with an
explicit host capability flag.

Navigation records each new request once. Request-to-ready wall time includes
waiting between slices. Expanded nodes exclude stale `A*` heap entries, include
the goal and include unique walkable nodes in flee floods. Route length is the
pulled polyline in game units, rather than `A*` cost. An absent native route is a
failure even when the existing off-mesh straight-line fallback is returned.
Cached failures are not new requests. Cancellation is separate from failure,
and subsequent new requests by the same actor count as re-plans. Session totals
and recent completed/cancelled samples are per actor. The preceding wall-clock
second supplies request and re-plan counts, retaining at most 4096 entries of
each kind per actor. Unloading removes actor measurements.

Accepted detection transitions against the player, player melee contacts,
witnessed player crimes and successful player dialogue requests start reaction
measurements. Actual combat acquisition or StartCombat, assault alarms and
opened conversations finish compatible stimuli. Repeated outstanding stimuli
retain the earliest timestamp. Rejected requests and eligibility probes produce
no response sample. A stimulus without an implemented response remains pending.
These hooks cover the existing entry points, without creating a new response.
Automatic greetings, package actions and scene conversations use the NPC
entry point, so they do not invent a player-dialogue stimulus.

A non-arrived actor with a remaining path is stuck after five non-menu seconds
without decreasing its remaining polyline length by more than 0.01 game units.
Backtracking does not reset the timer. Progress, arrival, inactive actors and a
changed destination clear it. Metrics are transient, add no save sections and
never supply gameplay decisions. The engine-owned timing and progress contract
is PRF-27, rather than a vanilla saved-layout claim.

## Magic tick work, PRF-35

Magic ticks decode winning SPEL and MGEF records once into shared read-only
views. Runtime MGEF overrides bypass those views. The condition-presence
cache stores no evaluated condition result. Live conditions still run at
each authored one-second boundary, and concentration checks each application.
The active-effect loop reads existing target actor state without invoking
the actor builder or scanning its spell list. Unchanged modifiers advance
their elapsed clock and check expiry without cloning their value changes.
Ongoing damage, taper, wards, cloaks and hazards keep their existing tick paths.
Final partial intervals, death dispels and paused clocks retain their behavior.

Effect insertion and removal maintain derived identity indexes for archetypes
and summoned references. Slow Time, Paralysis, Detect Life and the host Grab
Actor and light hooks use those indexes. Native-save load rebuilds them from
the restored active-effect table. Translation ticks visit active translations
and references borrowed for mutation since the previous tick. Completion
removes their candidate identity. Load rebuilds candidates from saved
translations, and direct RefState translation mutations are discovered through
`World::reference`. These caches and indexes add no saved state.

`Game::magic_frame_history` reports image-space, translation, shader-particle,
effect-shader, visual-effect, active-effect and concentration tick phases.
The casting and dispatch remainder includes reactive casts and phase bookkeeping.
It uses PRF-12's exclusive magic clock and rolling 240-tick statistics.
Paused ticks skip the seven update phases. Reactive casts still enter the
remainder. Timing suspension also suspends phases.
The host reports these subphases inside the existing magic system total.

Sources are the repository's MAG implementations and PRF-12 clock, local
OpenMW `apps/openmw/mwmechanics/activespells.cpp` for immutable spell-store
views and active-application iteration, CK `TranslateTo` and `DispelAllSpells`
for events and preserved abilities, and UESP `MGEF.wikitext` for death flags.
OpenMW's Morrowind effect execution is not used for Skyrim archetypes.

## Entity runtime hooks, PRF-13

Optional scopes attribute actor planning, package evaluation, driver movement
and procedure movement by resolved FormID. Resumed navigation and marker
jobs carry their requesting actor's planning identity. Quest start, stop, stage changes,
reference and location alias fills, forced alias changes and quest condition
evaluations carry quest identities. Reached CTDA predicates carry caller IDs
and separate dialogue, perk, magic, package and quest categories, including
failed predicates. Dialogue identifies the topic and INFO. Perk dispatch
identifies the entry-point code, section and condition tab. Other callers use
the condition owner record. Ownership follows that owner even when a dialogue
topic belongs to another plugin. Package conditions use their own evaluator.

Form ownership follows local Mutagen `docs/plugins/ModKey, FormKey, FormLink.md`.
It uses the plugin that defines the resolved FormID, even when
another plugin supplies its winning override. Created forms use `<created>`.
Runtime record overrides carry the same diagnostic identity. Script loading
maps the winning VFS archive stem to a loaded plugin stem. Loose script files
retain their winning path as a provider label because VFS has no plugin
ownership for loose roots. `Vm::set_script_plugin` accepts an explicit provider.

`SOE_PROFILE=1` enables capture on Game construction. Dropping a headless Game
writes `soe-profile.json` and `soe-entities.json` into `SOE_PROFILE_DIR`, or the
current directory when unset. Write failures are reported. This process-wide
capture is independent of world state and adds no saved sections. The
`prf_13_` integration tests exercise two actors, two scripts, one quest, all
five predicate categories and installed master record ownership.

## PRF-24 observation counters

Successful `World::evaluate_detection` batches add their observation count to
`detection_checks` and their actor-to-actor observation count to
`detection_pairs` in the shared transient `soe-trace` metric snapshot. The
observation contains a caller-supplied line-of-sight result. It does not cast a
ray, and the metric snapshot keeps `line_of_sight_raycasts` at zero until a
host raycast is connected to this path.

## Per-system frame timing, PRF-12

PRF-20 records NAVM preparation during `World::load_cell` after reference
prefetch and before attachment. `take_navigation_load_time` consumes the latest
duration for that resolved cell. The host adds it to the cell's streaming
report when attaching the location. Rendering-only loads perform no NAVM
preparation and report zero. These durations are transient diagnostics and
are not saved.

`Game::tick` and `tick_menu_mode` record an always-on exclusive system frame
through `soe-trace`. `Game::frame_history` exposes the last tick's breakdown
and rolling p50, p99 and worst over 240 ticks, with zero samples for idle
systems. The frame denominator is the tick's elapsed wall time, independent
of the simulated time argument. This history is session data and is not saved.

The tick times magic updates, scene/dialogue work, quest lifecycle and story
manager dispatch, and Papyrus scheduling. The actor driver times package and path planning,
movement and combat selection/pursuit. Per-actor movement bookkeeping stays
in the outer movement scope. Combat timing starts after eligibility checks
accept the actor, before scanning targets. Direct spell, melee and dialogue
entry points also carry system scopes, including calls from Papyrus natives.
Sound request queuing uses the audio slot and save serialization uses the
save slot. Nested system scopes exclude their
children's time. Host-only systems remain present with zero tick time.
Host snapshots can surround ticks and include saves and input between them.

The fixed-slot timing model is documented in [trace.md](trace.md).
`prf_12_frame` tests check tick accounting, paused idle systems and host/save
snapshots. Its optimized 1,000-actor calibration uses the public record
builders and travel/sandbox fixture from `crates/bench/src/stress/fixtures.rs`.

## Initial inventory list selection, UI-25

Fresh references resolve CNTO and distributed LVLI quantities through the
existing `World::roll_leveled_inventory` path at their encounter level.
Concrete quantities and leaf COED stacks replace the list entries in the
shared reference inventory before outfit application. Repeated reference
lookups use those stacks without another roll. Dynamic containers and the
fallback player inventory use the same resolver. Invalid initial lists reject
authored reference construction or disable a dynamic placement with an empty
inventory. Failed selection restores the resolver's RNG state.
Vendor VEND lists that name an LVLI match its descendant item forms after
stock materialization, without another roll during vendor refresh.

The existing inventory writer stores the concrete result. Native save loading
and vanilla inventory restoration retain their supplied inventory and do not
generate fresh contents. This adds no field or save section. The raw authored
inventory builder remains available to the vanilla inventory delta importer.

The source is local UESP `LVLI.wikitext` and the repository's shared LVLI
resolver, which implements xEdit's list flags, nested counts and COED metadata.
UI-25 reuses it because a menu-specific list selection would create a second
inventory and change the result on refresh. `ui_25_initial_inventory.rs` checks
concrete counts, repeated lookup, native save reload, dynamic placement and
invalid cyclic lists.

The MIT CommonLib shim's `WorldState` implementation reads and writes actor
values in the existing actor component store for actors already built in the
world. It resolves actor-value indices through the vanilla actor-value name
table. Its default-object lookup reads DOBJ records through the footstep
record view and returns the resolved FormID. It does not cache either result.
SHM-14 adds runtime FormID handles to the `WorldState` player view. Its C++
contract test queries player identity, position, actor values, death state and
loaded actor forms from a Rust service borrowing `World`. Player object-reference
handles contain the runtime FormID.

`ambience::tracks_for_cell` resolves the cell's `XCMO` music type, or the first
regional sound entry's `RDMO` when the cell has none. It returns eligible
single-track `ANAM` paths for host playback, following MUST conditions and
palette `SNAM` entries. Region sound-entry detection uses `REGN.RDAT` type 7.
Silent MUST records are skipped. The local TES5Edit `wbDefinitionsTES5.pas`
and UESP `MUSC.wikitext`, `MUST.wikitext` and `REGN.wikitext` define these
fields. MUSC selection flags, priority and fade, region priority and `RDSA`
sound descriptors are not evaluated yet.

The combat integration includes race-based unarmed damage, difficulty selection,
template-backed hits, weapon enchantment effects and per-instance charge,
armor slot replacement and explicit weapon hands. SOEW version 21 retains
both difficulty and hands, with separate readers for the version 16 and 17
branch layouts. The authored contact bridge reaches the shared Game through
animation events. Live host collision synchronization and attack input are
separate unfinished rows. See [combat](combat.md) and BACKLOG.csv.

Crate `crates/game`. Round 2 of `ENGINE-PLAN.md`, written 2026-09-14. The map's
week 6 to 9 gate.

`world::commonlib` projects existing world state into the MIT CommonLib shim:
ExtraWorn flags from inventory stacks, GameSettingCollection reads through the
runtime override and winning GMST path, placed HAZD reference identity and
placement, the actor's combat target and stagger window, and HitData from an
accepted melee result. The adapter owns no parallel state. Hazard ticking,
head tracking, animation sequence state and executable hit dispatch remain
unsupported hooks with named reasons. The C++ contract has no Rust ABI bridge.
HST-18 adds transient crosshair and grabbed-reference values to `World`.
`Game.GetCurrentCrosshairRef` and `Game.GetPlayerGrabbedRef` create the
corresponding Papyrus form object when a host value is present, and return
`None` when it is absent. The values are host frame state and are not saved.
The MIT `EventState` implementation reads an instantiated reference's inventory,
current actor death and unconscious flags, reference and winning-record deletion,
and each actor's reserved furniture seat. Handles resolve through the current
load order. Unknown or uninstantiated references return no result. A current
seat is represented as a furniture Enter event. Binary hook requests return
the shim's named unsupported-hook error.

## What is in it

`commonlib_world` implements the MIT shim's save/load and leveled-list
contracts on `World`. Its calculated entries resolve LVLI FormIDs through the
load order, retain supported COED owner, rank, global and condition data, and
use the same stateful RNG path as native leveled-list rolls.
Beam projectile impacts and Havok character-controller flags return named
unsupported-hook results because the runtime has no corresponding state.

The CommonLib shim's read-only extra-data adapter borrows a live inventory
stack and resolves stored `GlobalFormId` values through its current load order.
It exposes enchantment identity and charge, hotkey slot, and leveled-creature
base links to the MIT API. The serialized stack does not retain the runtime
enchantment removal flag, so that field is returned as unknown. Malformed
layouts and unsupported mutation hooks do not produce editable extra data.

- `commonlib`: source-level views of loaded form-factory descriptors, reference inventory
  stacks and reference handles for the MIT `commonlib-shim` crate. Factory allocation, inventory
  mutation, and Scaleform item-list selection and refresh return typed unsupported results. The
  SHM-12 C++ contract test exchanges requests with a Rust test fixture that answers from this
  adapter and a live `World`.

- `world`: the runtime form table. A `LoadOrder` from `soe-esm` plus the state
  a running game adds per form: references (position, cell, enable state,
  inventory, links, door teleports), actors (actor values, death, combat,
  factions, spells), quests (stage, done set, objectives, aliases), globals,
  form lists. Base data comes from the winning record the first time a form is
  touched, so the table is a view over the load order and nothing is copied up
  front. Run-time forms from `PlaceAtMe` live in file slot `FF`. The player uses
  the reserved PlayerRef identity 0:000014. `world.rs` holds the state types, the `World`
  struct and its constructor, the methods sit in one child module per
  concern, each an `impl World`: `lookup` (editor ids, records, run-time
  FormIDs, keywords, names, classes), `objects` (handle to VM object table),
  `refs` (reference state, placement, the player, inventory, activation),
  `furniture` (Sit marker reservations and actor seating),
  `actors` (the `NPC_` view, actor state and values), `quests` (stages,
  objectives, aliases), `forms` (globals, form lists, leveled lists,
  locations, factions, actor bases, cells, game settings), `events` (what
  natives raise for listeners, scenes, sounds), `timers`, `rng`, `driver`
  (what the package driver reads), `save` (the `SOEW` format) and `hash`
  (the determinism hash).

`Game::save_state_profiled` returns the same SOEG bytes as `save_state` and
reports encoded size and elapsed write time for the world, Papyrus VM and
outer save envelope. `Game::load_state_profiled` reports those section sizes
and the validation and restore time for each. The benchmark uses these rows
for native save inputs.

- `world/shim_extras`: CommonLib extra-data views over reference XLIB, XLIG,
  map-marker records, authored XLKR links and loaded linked-reference state,
  teleport destinations, inventory outfit/poison extras and resolved XAPR
  activation parents. XAPR targets must be REFR or ACHR records. Missing
  linked-reference targets are reported when their forms are absent from the
  form table. Interior cells expose the zero north-rotation default. Exterior
  cells resolve their owning WRLD and read its runtime north rotation, which
  starts at zero because plugin records do not serialize it. Executable vtable
  hooks return an explicit unsupported result.
- `World::add_quest_start_stop_sink` registers an MIT shim event listener.
  Successful `Start`, `Stop`, and stage-driven quest transitions emit the
  resolved runtime FormID and the resulting failed state. The event source
  observes the existing quest state and does not keep a second quest copy.
  SHM-23 streams live start and failed-stop payloads from this source to a
  compiled C++ sink over a test pipe. The test validates CommonLib event fields
  and does not define a C ABI.
- `vmad`: the `VMAD` subrecord. Scripts, their filled properties, quest stage
  fragments, INFO and PACK begin, end and change fragments, SCEN begin, end
  and phase fragments, PERK entry-point fragments, and alias scripts.
  PERK preserves each fragment index, marker byte, script and function name.
  Round-trips through `write` at the version and object format the
  record was authored with, not a fixed one, since installed Skyrim.esm and
  DLC records mix object format 1 and 2 and VMAD versions 4 and 5.
- `npc`: the `NPC_` view. `ACBS` configuration, `DNAM` stats and skills,
  spells, factions, items, race, class and template, with the template flags
  applied one level. `World::actor` follows each component's template chain.
  When actor state is first built, its resolved spell list applies
  constant-effect `SPEL` abilities to that actor without a cast. Unsupported
  or malformed ability effects are skipped. Their active effect state uses
  the existing SOEW active-effect table and survives save/load.
  AIDT supplies its six AI actor values, see [combat AI](combat-ai.md) for
  sources, validation and combat target selection.
- `detection`: the detection value from sneak GMSTs, and stealth-point pools
  shared by every observer of one target. `World::evaluate_detection` checks a
  whole batch of observations before it changes any pool. MGEF Disguise is not
  executed because its gameplay semantics remain undocumented. xEdit and UESP
  identify archetype 44 and specify no extra archetype data, while the Creation
  Kit archetype table describes it as unknown. See
  [combat AI](combat-ai.md) for the formula sources and what remains.

Detection batches cache each observer's Sneak, Blindness and combat target,
and each target's Sneak, invisibility, movement noise and worn armor weight.
An observer's combat awareness is indexed once per batch. Alert and busy
modifiers remain specific to each pair. Previously known pairs absent from a
batch receive a negative value, the lesser of the base detection value and
negative one, only when their enabled references occupy the same resident
coordinate space farther apart than `fSneakMaxDistance`. Exterior grid and
persistent cells of one worldspace share that space. Their existing loss and recovery
timers then advance without creating awareness for distant strangers.

`World::light_references_in_cell` caches authored LIGH reference identities
per cell and tracks instantiated light identities, including runtime spawns.
It reads current reference cells when returning its sorted list, so moved
lights leave their authored cell and enter their destination. Disabled and
deleted identities remain in the list, visibility is read by the sampler.
Cell prefetch builds membership before detection. Native-save load and vanilla
reference restoration invalidate this derived index. The cache adds no saved
fields. REFR/NAME and LIGH identity come from local
`TES5Edit/Core/wbDefinitionsTES5.pas`, placement and visibility remain in
`World::RefState`.

CTDA function 513, `IsCombatTarget`, checks whether the condition subject's
saved combat target is the placed NPC in parameter 1. It uses the same
`ActorState::combat_target` relationship as `Actor.GetCombatTarget` and
requires both forms to be actors. Functions 396 `GetCauseofDeath`, 397
`IsLimbGone`, 477 `GetThreatRatio`, 488 `GetConcussed`, 495
`GetKillingBlowLimb`, 508 `GetLastHitCritical` and 574 `GetAttackState` return
an explicit unsupported-function error. The local CK condition pages do not
specify usable return contracts for those functions. xEdit's condition table
and the CK `GetCombatTarget` page informed the implemented target predicate.

- `ObjectReference.CreateDetectionEvent` queues one sound at the calling
  reference's current cell and position with the supplied actor owner. Its sound
  level defaults to zero, and levels above 100 remain valid. Invalid actors,
  references and negative levels leave the queue unchanged. The host takes
  queued events once per observation step. Emitted sound uses the source's
  distance and line of sight, independently of the owner's position. The queue
  is transient and adds no save field.

- `quest`: the `QUST` view. Flags, stages with log entries, objectives,
  aliases with forced references, and each objective target's alias, lock
  marker flag and CTDA conditions, plus local or external alias dependencies.
- `natives`: the base game's natives, closed over the shared world.
- `commonlib`: the SHM-04 CommonLib actor adapter. It resolves runtime
  FormIDs into the load order and reads or changes actor values, inventory,
  death flags, race and equipment through the same `World` state Papyrus
  uses. Its equip manager reports invalid handles and unsupported equipment
  as errors.
- `world::visual_effects`: transient `VisualEffect.Play` instances keyed by
  resolved RFCT and placed-reference forms. Calls stack, finite durations
  expire on unpaused game ticks, and `Stop` removes all matching instances.
  Hosts can inspect the active requests in stable call order. The Godot host
  renders a temporary target visual and uses the linked EFSH start texture
  when available. Authored ARTO model rendering is not implemented.
- MGEF archetype 12, Light, resolves its associated LIGH form through the
  winning load order and places that light at the effect target. The active
  effect owns the runtime reference, which expires with the effect and is
  removed on its final tick. A missing or non-LIGH associated form rejects
  the cast before effect or reference state changes. The archetype number and
  associated-item field follow local xEdit `wbDefinitionsTES5.pas`. The CK
  magic-effect pages in the local corpus do not specify additional Light
  runtime semantics.
- `Spell.Preload` and `Spell.Unload` validate the winning SPEL record and queue
  typed spell art requests on `World`. `take_spell_art_requests` drains them in
  Papyrus call order. `Unload` queues only when this world previously received
  `Preload` for that spell. The Godot host does not consume these requests yet.
- `clock`: real seconds drive the scheduler and update timers, game time runs
  at `timescale` (20) for `GetCurrentGameTime`.
- `harness`: the conformance scenario runner, see `docs/validation.md`.
- `soe-validate`: the `soe-game` binary that resolves a portable MO2 profile,
  starts a fresh game world for each `.scenario` file, and reports its pass
  state and cited captures. See `docs/validation.md` for invocation and rules.
- The weekly mod corpus sweep is `tests/tst_12_mod_corpus.rs`. With
  `SOE_MOD_CORPUS` set to a directory containing one mod directory per child,
  it reports parser failures, unresolved Papyrus native declarations and
  plugin asset paths absent from that corpus and optional `SKYRIM_DATA`.
  Asset lookup reads known model, icon and marker filename fields, not editor
  IDs or localized display strings. An empty corpus directory has no mod roots.
  Field selection follows xEdit's `TES5Edit/Core/wbDefinitionsTES5.pas`,
  which defines `EDID` as Editor ID, `FULL` as Name and model fields as
  filenames. Wabbajack's local `README.md` says modlists are reproduced without
  bundling assets, so the sweep reads installed files in place.
  It parses plugins, BSA/BA2 members, NIF, HKX, DDS, PEX, FUZ/LIP, xWMA and
  WAV containers. Synthetic tests check each finding category and verify
  findings stay scoped to their owning mod. The external sweep skips when
  `SOE_MOD_CORPUS` is unset.
- `Scenario::append_papyrus_log_expectations` adds a vanilla log's ordered
  trace lines as one expectation. Differential reports list every missing
  expected trace and allow extra runtime output. The `tst_11` real-log check
  reads `SOE_PAPYRUS_LOG` when set and verifies each parsed trace appears in
  the differential report when absent from the VM log.
- `wasm`: the Wasmtime component host. `wit/mod-api.wit` defines the versioned
  `soe:mod-api` interface with `subscribe-on-activate` and `set-stage` imports
  plus the initialization and activation exports.
- `Game`: owns the VM and the world. `load_scripts_from_data` builds the load
  order's `soe_bsa::Vfs` (each plugin's `<stem> - Misc.bsa` or `<stem>.bsa`
  archive if the install shipped one, loose files on top) and loads every
  `.pex` it answers under `scripts/`, so an install that ships its scripts
  packed rather than loose still has something for the binder to attach.
  `bind` attaches scripts on non-reference forms. Placed-reference scripts
  attach when their cell is attached and the VM is pumped, with `bind_cell`
  available for a specific cell. Missing PEX files are returned in the
  `BindReport` for the cell. `start` also starts quests flagged Start Game
  Enabled, `tick` advances time, `activate` and `send_event` do what a player
  or the engine would, and `pump` drains what natives queued.
- `world::shim`: provides MIT CommonLib views over the existing calendar,
  loaded actor components, DOBJ records and quest alias assignments. It does
  not store duplicate state. Activate-reference-children is explicitly
  unsupported because the runtime has no matching child activation data.
- The MIT CommonLib world adapter reads GMST settings, calendar hour, and active
  reanimation effects from this same `World`. Sit and sleep animation state is
  not stored by the runtime, so the adapter reports it as unsupported. Binary
  address and vtable hooks have no runtime target. SHM-15 exposes this view to
  C++ through the MIT `RuntimeStateCallbacks` table and `RE::CallbackTES`. The
  SHM-15 C++ fixture queries a test host that reads each result from the live
  `World` without keeping a copied state.

`World::commonlib_event_sources` exposes the in-process CommonLib event
channels for disarm, dragon souls gained, level increase and location
discovery. The matching `commonlib_*` methods send those event payloads to
registered sinks. Form identities retain both the load-order file index and
object id. Player level increases are emitted after progression changes the
live player level. Positive `ModActorValue` changes to the player's
`dragonsouls` value emit the accepted increase. A successful weapon disarm
emits the caster and target after the weapon is dropped. `AddToMap` emits
location discovery only when it first adds the reference, using the cell's
resolved worldspace when available. Level increase reads the current player
identity and rejects levels outside the event's `u16` field before dispatch.
The grab and release channel follows held-reference changes, harvest follows a
successful `World::harvest_item` inventory addition, hit follows a committed
melee contact, object loaded follows reference 3D residency changes, and quest
stage follows a successful defined stage change. Quest stage events use item
index zero because quest log callbacks are not run individually here. Form and
reference identities use the live load order's runtime IDs. These channels add
no saved state. Disarm, dragon soul awards and location discovery remain event
entry points for gameplay callers, with successful weapon disarms, accepted
dragon soul changes and first-time map additions already wired to dispatch.
Binary hook requests return the requested hook name as an unsupported result
without changing runtime state. Event payloads, generated C++
declarations, the CommonLib source-level API, and its contract fixtures are in
`crates/commonlib-shim`. Source headers are documented in `docs/shim.md`.

## Design

### Recorded input sessions, PRF-22

`player_input::Driver::start_recording` resets the existing gameplay RNG
streams to the supplied seed and captures the initial SOEG world and VM state.
It retains executed PLY-01 commands and their success or rejection messages.
Hashes cover the initial state, each 60-frame boundary and a final partial
interval. `finish_recording` returns a `Session`, whose version-one SOER file
uses the existing Papyrus Reader/Writer encoding and a payload CRC32.
Truncated files, trailing bytes, unknown command tags, invalid transforms and
missing or unordered checkpoint logs return errors.

`start_replay` requires identical plugin and script provenance and validates
the snapshot before loading it. The session also restores cell residency,
reference 3D residency, deferred unload, binding bookkeeping, greeting range,
held casting hands, MCM registration state and VM native-call counters. These transient inputs to
gameplay are recorded separately because loading a native save clears or
rebuilds them. Replay consumes the shared fixed-step queue, checks every
outcome and scheduled hash, and stops at the first divergence. The error
names its frame and differing hash subsystems. Queue and recording state
remain diagnostic data, outside native saves and gameplay hashes.
Definitions and runtime providers must be loaded before replay, as for SOEG.

The session design follows the local Godot
`godot-docs/tutorials/math/random_number_generation.rst` seed contract.
The existing PLY-01 executor and SOEG snapshot already implement the required
gameplay and state transport, so the session does not replace either reader.
`prf_22_` tests record 600 frames with a running random and latent VM loop,
replay twice, compare all checkpoints and events, and exercise corrupt files,
changed definitions, rejected actions and divergence reporting.
Game ticks also add exclusive system totals to active PRF-13 captures.

### Scripted player input, PLY-01

`player_input::Driver` queues typed commands and executes one per step at
1/60 second. An empty queue does not tick. Each event retains its step,
command, gameplay result or error, player cell and pose, and new Papyrus
traces. Queue and log are transient. World owns all gameplay mutations.

Commands place the player at a reference plus an offset, set XYZ rotation
in radians, activate, execute an authored melee contact, press or release an
equipped casting hand, choose a dialogue topic, confirm a menu operation,
or wait for one step. MoveTo is explicit placement, with optional rotation
matching and normal cell and trigger events. As the local CK
`MoveTo_-_ObjectReference.wikitext` documents, rotation remains unchanged when
the destination cell is unloaded, even when rotation matching is requested.
Menu operations advance a
dialogue response, close a latent gift menu, or commit a COBJ transaction.
Scripted steps advance the simulation independently of host menu presentation.
Attack uses the existing equipment and damage checks with a supplied contact
target. It does not generate an animation or a spatial contact query.

Default activation now moves loose inventory references through
`add_item_notified`, preserving item instance data and delivering inventory
and container-change events. The input layer rejects disabled or deleted
references, preserves OnActivate on blocked activation, and begins dialogue
after a successful actor activation. Invalid transforms are rejected before
enqueueing. Gameplay rejections are recorded and consume one fixed step.

The local CK `Activate_-_ObjectReference.wikitext`,
`MoveTo_-_ObjectReference.wikitext`, `SetAngle_-_ObjectReference.wikitext`
and `OnContainerChanged_-_ObjectReference.wikitext` supply activation,
placement, compass yaw and pickup event semantics. This is a scripted input
schedule over existing Game operations, rather than a new format reader.
`ply_01_` tests exercise timing, interactions, combat and casting, rejection,
and an installed vanilla load door. Optional `SOE_PLY_01_FIXTURE` exports the
synthetic plugin, scripts and headless log for the windowed host comparison.

### Headless playthrough routes, PLY-02

`play::Route` parses one authored operation per line. Blank lines and `#`
comments are ignored. Double quotes preserve spaces in form names. A form
is an editor ID, `player`, `Plugin.esm:hexObjectID`, or `dynamic:hexObjectID`.
Plugin names resolve through the live load order rather than a saved slot.

```text
navigate <reference> <units-per-second> <max-ticks>
navigate-marker <cell> <x> <y> <z> <units-per-second> <max-ticks>
activate <reference>
talk <speaker> <topic>
wait-stage <quest> <stage> <max-ticks>
expect-stage <quest> <highest-stage>
expect-position <cell> <x> <y> <z> <tolerance>
expect-item <holder> <item> <count>
expect-no-vm-error
```

Navigation uses winning NAVM records within the starting interior or worldspace,
including resolved edge links and deleted meshes. Endpoints must have a walkable
XY footprint in their named cells and lie within 128 units of the surface.
They project onto that surface. Closed bound doors block their triangles and
are refreshed while walking. The runner queries the existing smoothed path at
each tick and moves toward its next waypoint. It retains waypoint cell ownership,
delivers cell and trigger events, checks surface samples within 0.25 units,
and fails on missing paths, interrupted movement or timeout. Load-door crossings
use activation steps. Navigation adds no collision host or reference nodes.
The runner does not consume host obstacle shapes or simulate a physics capsule.

Actions use the shared PLY-01 executor and advance 1/60 second, including gameplay
rejections. Unresolved form names fail before execution. `talk` activates an
actor, chooses the offered DIAL topic, then advances
the response through the existing dialogue API. Waits pump the same clock and
VM until the requested stage is in the quest's done set. A higher stage alone
does not satisfy that wait. Position assertions check cell identity and Euclidean
distance. Item assertions use the shared GetItemCount query for base forms,
original reference instances, form lists and keywords. VM assertions
reject any `error:` entry in the VM log, including entries preceding the route.
Speeds must be positive and at most 600 units per second. A time bound is at most
36,000 ticks. Assertions consume no ticks. The first failure stops execution and
reports the step number, source line, reason, player pose, sorted inventory,
loaded quest stages and flags, live VM thread count and execution trace.
Routes and reports add no saved state or dependencies.

`soe-play <Data-directory> <external-save.ess> <route-file>` reads plugin names
from the ESS, loads their scripts through the existing archive and loose-file
overlay, and invokes transactional world/VM restoration before binding scripts.
It does not run new-game quest startup. A route failure or unsupported save import
exits with status 1, invalid arguments with status 2, and success with status 0.
It uses the existing ESS import boundary, including inert queued messages and
suspended call tables. It does not claim complete vanilla-save resumption.

An explicit fourth argument, `--world-checkpoint`, imports the supported world
passes without saved VM variables or execution and binds fresh scripts. It
prints that choice on stderr and never falls back from strict import. This
allows world assertions and authored input from corpus checkpoints while the
existing importer rejects their unresolved ReferenceAlias identities. Checkpoint
startup omits saved timers and queued calls. It does not reproduce a resumed
quest's script variables. Both modes reject failed world passes. Neither mode
changes the external save. `load_world_checkpoint` exposes this startup choice
to callers without a command-line process.

`play::Checkpoint` captures completed stages, current and highest stages,
running, active, completed and failed flags, the other quest flags, saved form
flags and player item quantities. `run_to_checkpoint` executes an authored
route and compares both worlds over the union of their loaded quests. A quest
outside one world's loaded table uses its winning record defaults without
loading it or dispatching events. Missing player state and invalid scoped
quests return checkpoint errors. Zero and absent inventory entries compare as
quantity zero. Reports sort quest and item identities and name each differing
field. A route failure retains its trace and the checkpoint differences.
`CheckpointReport::passed` requires both successful gameplay and zero differences.
Checkpoints and comparison reports are transient and add no saved state.

The `ply_03_` synthetic tests exercise flag and stage differences, item additions
and removals, absent quests, record defaults, invalid scope, failed routes and
an inventory pickup route matching an independently constructed successor.
The gated acceptance test executes two authored Helgen routes against the first
three numbered main quest saves and requires zero differences for both pairs.
It requires `SKYRIM_DATA` and `SOE_TEST_SAVES`, reads saves in place and lists
all fields even when a route fails. It uses world-checkpoint startup and does
not import saved VM variables, scene execution or timers.

Current result on installed data (2026-10-09, head 4b90a696): this acceptance
test and two breaker tests do not pass. `cargo test -p soe-game --test it --
ply_` with `SKYRIM_DATA` and `SOE_TEST_SAVES` set gives 117 passed and 3 failed,
`ply_03_main_quest_replay::ply_03_two_consecutive_main_quest_pairs_list_quest_and_inventory_differences`,
`ply_03_breaker_adversarial_3::ply_03_replay_reaches_both_consecutive_corpus_checkpoints`
and `ply_03_breaker_adversarial_4::ply_03_each_route_reaches_its_successor_checkpoint`.
The remaining difference is the done set of MQ101DragonAttack (0xD0593) in the
second pair, where the replay lacks stages 174 and 195, and the first pair
differs in `current`, `highest` and `done` for the same quest when the route
stops at stage 190 against the save's 192. The `Activate` step on
`Skyrim.esm:25fae` logs `GetTriggerObjectCount called on a non-trigger` from
the lever script on REFR 0x945B9 (MQ101CollapsingBridgeLever, base script
TrapLever on TrapTriggerBase). The CK wiki says the call errors on a
non-trigger, so the message matches the original, but the route's
`expect-no-vm-error` step does not see it and the trace from this error to the
missing stages is not done. The default gate runs with both variables unset and
exercises none of this.

The comparison uses the existing ESS world importer. The local CommonLib
`include/RE/T/TESQuest.h` defines the quest flag bits and the CK
`Quest_Script.wikitext` defines highest completed stage queries. The installed
MQ101 and MQ101DragonAttack PEX fragments and VMAD properties identify the
Helgen actions used by the authored routes. No assets or saves are copied into
the repository and no dependency is added.

Placed-reference binding inherits the base form's VMAD scripts and property
values before applying reference overrides. Removed scripts stay unattached,
and saved instances retain their values. Each property's FormIDs resolve in
its own source plugin. The local CK `Papyrus_Introduction.wikitext`, References
Inheriting scripts and property values from parent base objects, supplies this
contract. `quest_flow` checks inherited quest properties, overrides, script
removal and single activation delivery. The installed Helgen trigger's MQ101
property is authored on its base activator.

Local CK `PathToReference_-_Actor.wikitext` supplies path failure and interruption
semantics, and `Activate_-_ObjectReference.wikitext` supplies default activation
and blocked OnActivate behavior. `GetItemCount_-_ObjectReference.wikitext` describes
base-object quantities. Navigation reuses `soe-navmesh`, whose source is local
Recast/Detour `DetourNavMeshQuery.cpp`, `findStraightPath`. Detour's independent
mesh representation is not introduced. `ply_02_` tests cover grammar, navigation,
interaction steps, bounded waits, assertions, diagnostics and external corpus
startup through the binary.

`ply_05_seeded_bot_walks_interacts_checks_invariants_and_repeats_its_report`
chooses a movement rate from its seed, runs the route through the PLY-02
executor, and advances another 30 minutes of game time at 1/60-second steps.
It checks finite actor and player transforms after every tick, rejects Papyrus
VM errors and compares repeated reports for the same seed. Navigation checks
the player's path against NAVM surfaces through the existing PLY-02 executor.
`ply_05_installed_city_cells_are_available_for_headless_soak` is gated by
`SKYRIM_DATA` and confirms the installed Whiterun and Riverwood exterior cells
resolve. The current soak does not yet drive the installed city records or
compare actor heights against exterior terrain.

### AI-driven player routes and workstation commands, PLY-06

`Command::RunPackage` gives the player a priority 100 forced ActorUtil
override and enables the same AI flag as `Game.SetPlayerAIDriven`. The actor
driver ticks the player only while that flag is true. `Driver` holds later
commands until the package tree finishes, then ends the package through the
normal boundary event path and restores the prior flag and any prior override
for that package. A positive optional duration bounds endless packages such
as Sandbox using non-menu seconds. Expiry and explicit `cancel_package`
return ownership without marking the package complete. Unsupported procedures
and interrupted handoffs report errors and release ownership. Manual scripted
actions reject while AI owns the player. Wait remains available. AI movement
updates the player's cell residency and trigger crossings through Game's
existing APIs. Sit accepts a target input as used by the vanilla SitTarget
template, reserves its furniture marker and releases it on completion.
Successful Activate attempts advance their package branch.

The route queue, deadline and prior ownership are transient Driver state.
Call `cancel_package` before discarding a driver with a running route.
Persistent ActorUtil overrides belong to each actor, separate from SPID
insertion positions. The five ActorUtil package natives add, remove, count,
clear and remove across actors. Priority is clamped to 0 to 100, and the last
added override wins a tie. Only the highest override is tested. Its failed
schedule or conditions fall back to the ordinary package stack. Flags value 1
bypasses those checks. Completion removes its override. Overrides retain
addition order and resolved PACK identity in the PLY-06 actor save section
and participate in the actor state hash. Missing sections leave no overrides.

Forge confirmations call `Game::craft`. Alchemy confirmations supply two
ingredient forms to `Game::brew_potion`. Both use the shared inventory and
notification paths. A FURN station's WBDT type 5 accepts alchemy without a
recipe keyword. Brewing with authored MGEF costs grants Alchemy skill use
from the sum of floored effect values, then applies the AVIF use multiplier
through `World::advance_skill`. NoMagnitude and NoDuration select unit value
factors. The calculation uses the effects the existing two-ingredient brewer
produces. Definitions without MGEF DATA retain the prior no-experience path.
Missing components or failed progression leave inventory and notifications
unchanged.

The installed-data PLY-06 tests execute `WhiterunAdrianneEstablishingScene`,
`WhiterunAdrianneSandboxForgePackage6x11` and `C06EorlundReforgeWuuthrad`.
They craft the vanilla iron dagger recipe at Warmaiden's forge and combine
Blue Mountain Flower with Wheat at Breezehome's enabled alchemy lab. The
checks verify component removal, output inventory, skill experience and
engine save/load. Synthetic cases cover cancellation, unsupported procedures,
priority and eligibility, cell residency, menu pauses and section corruption.

The local CK `Game_Script.wikitext` defines SetPlayerAIDriven's flag, and
`EvaluatePackage_-_Actor.wikitext` describes reevaluation. The named individual
SetPlayerAIDriven and Package pages are absent from this corpus. The existing
PACK reader and procedure tree use local xEdit and UESP PACK definitions.
`PapyrusUtil/Scripts/Source/ActorUtil.psc` supplies priority and latest-added
semantics, while `PapyrusUtil/PackageData.cpp` supplies forced eligibility and
completion removal. The script's 0 priority boundary is retained despite that
older C++ implementation clamping it to 1. Skyrim address hooks in that source
cannot run in this Rust engine, so these operations use the existing form-backed
package interpreter. The per-effect value calculation follows the author's
[W.H.A.T. technical notes](https://www.nexusmods.com/skyrimspecialedition/articles/11367).
Local UESP `MGEF.wikitext` supplies the flags and base-cost field.

### MCM script lifecycle, UI-05

`Game::start` initializes enabled quests without registering MCM scripts.
The first `Game::mcm_configs` call is the host's manager-ready boundary. It
scans running quest forms in winning load-order order and
reads `SKI_ConfigBase.ModName` and `Pages` from their attached Papyrus scripts.
It registers at most 128 configs, delivers `OnConfigRegister` once per config,
and returns the current page labels. `mcm_open`, `mcm_select_page` and
`mcm_close` call the script's `OpenConfig`, `SetPage` and `CloseConfig`
functions. Those functions deliver the initial empty-page reset, selected-page
reset, open and close callbacks. The registry and active menu selection are
session UI state, while page labels remain script properties. This bridge
also exposes `mcm_custom_control` and `mcm_custom_controls`, which call
registered configs' `GetCustomControl(keyCode)` methods and return string
results in registration order. The methods use ordinary Papyrus dispatch so
config overrides are honored. It does not execute SkyUI's Scaleform panel or
its option APIs. The lifecycle order and limit come from the local SkyUI
`SKI_ConfigBase.psc` and `SKI_ConfigManager.psc` sources. The focused fixture
assembles equivalent lifecycle methods in Papyrus bytecode.
`ui_05_original_skyui_source_defines_lifecycle_order` checks the original
source when `SOE_SKYUI_CONFIG_BASE` names its `SKI_ConfigBase.psc` path.
`ui_05_executes_unmodified_skyui_base_when_pex_is_available` loads and runs
the unmodified `SKI_ConfigBase.pex` when `SOE_SKYUI_CONFIG_BASE_PEX` names that
file. The PEX execution check skips when the compiled SkyUI script is
unavailable. It was not available in the local reference checkout or installed
game data during this change, so unmodified-script execution remains
unverified here.

### Furniture activation, HST-09

Activating a FURN reference selects the first unreserved Sit marker from its
four-byte FNPR entries whose corresponding low MNAM bit is active. A FURN
record with the MNAM Disables Activation flag set cannot be reserved. Marker
indices are zero-based, and only indices 0 through 23 have active MNAM bits.
Malformed FNPR entries are skipped. The actor reserves that marker and moves
to the furniture reference transform. Activating the same furniture again,
or calling `World::stand_from_furniture`, releases the reservation.
`World::furniture_seat` exposes the actor's current furniture and marker, and
`World::furniture_marker_in_use` reports reservations, including ones not yet
in animation. The actor's furniture reference and marker are stored in the
HST-09 actor save section. The FURN field layout follows local xEdit
`wbRecord(FURN)` and `uesp-modfileformat/FURN.wikitext`. Marker types and the
active MNAM mask come from those definitions. The CK `IsFurnitureInUse` and
`IsFurnitureMarkerInUse` pages define in-use checks to include reservations by
default. Marker transforms are not read from the furniture model by this path.

Patrol marker data reads `XPRD`, `INAM` and typed `PDTO` entries from each
linked `REFR`, as defined by xEdit `wbRecord(REFR)` and Mutagen `Patrol.xml`. A
resolved `INAM` IDLE form emits its editor ID through the actor's animation
event path once when the actor arrives. A marker whose base is FURN uses the
existing Sit-marker reservation. The first non-null DIAL reference in the
marker's PDTO entries starts a dialogue session when the actor and player are
eligible and no other session is open. Topic subtype entries are validated
but do not select a dialogue session. The reservation is released before the
route advances, and the Patrol dwell deadline prevents the idle event or topic
action from firing again after save and restore. These field layouts follow
xEdit `wbRefRecord(ACHR)` and Mutagen `Patrol.xml`. AI-23's UseIdleMarker
reservation and interruption behavior is not in this worktree, and its
reservation contract remains separate from Patrol's FURN reservation.

### Player sleep, HST-11

`World::sleep_at_furniture` accepts only enabled, non-deleted FURN references
with an active FNPR Sleep marker and an available reservation. It places the
player at the bed, emits `OnSleepStart`, advances the shared clock for the
requested elapsed game hours, emits `OnSleepStop` with the interruption flag,
and releases the marker. Non-finite or non-positive durations, a paused
timescale, inactive markers and occupied markers leave the clock unchanged.
`Game::sleep_player_at_furniture` settles the queued Papyrus events.

This marker path follows local xEdit `wbRecord(FURN)` and
`uesp-modfileformat/FURN.wikitext`: FNPR value 2 is Sleep, and the low 24 MNAM
bits enable marker indices 0 through 23. The CK furniture-use pages describe
reservations as in-use, so an occupied marker cannot be selected by another
actor. The host currently completes an eight-hour sleep on bed activation.

`vanilla_save::SavedFormMap` remaps ESS references through saved plugin names,
including regular and light slots. `World::restore_vanilla_globals` atomically
applies the global table to winning GLOB forms. `World::restore_vanilla_items`
applies INGR, ALCH and ENCH change markers to shared record overrides, with
form flags visible through record queries and marker state retained in SOEW
version 37. ALCH parsing reads the same shared record view. xEdit defines the
item value, fullname and ingredient-use fields as payload-free markers, so
those markers do not supply replacement field values. These are restoration
components, not a full ESS load or resumed game. See [save.md](save.md) for
mapping rules, failure behavior and the remaining world and VM work.

`World::restore_vanilla_activators` applies decoded ACTI, TACT and DOOR flag
masks to shared records. It validates the full change set before publishing
record overrides, so consumers such as `World::record` and `World::name_of`
observe one restored result. Marker-only saved fields are available through
`World::restored_activator_extra`.

`World::restore_vanilla_items` applies decoded ARMO, WEAP and AMMO changes as
shared record overrides. It prepares and validates every target before
installing any override. `World::record` and record-backed consumers continue
to read the same winning record. `restored_item_extra` exposes the six-byte
form flags, zero-byte value and full-name markers, unnamed flags and remaining
payload bytes. xEdit defines no application operation for the mask, so this
pass retains it without changing record flags. SOEW version 37 saves this
metadata, and earlier versions remain readable.

Saved REFR and ACHR form flags XOR the authored record flags with xEdit's
`Mask Invert Flags` word before deriving runtime enable and persistence. A
deleted winning record remains disabled. The second `Mask Set unk010` word is
retained in the restore report and does not affect those two runtime properties.
The reference-mask path accepts ESS versions 12 and 13 with matching
change-form/form versions 77 or 78. Versioned synthetic cases and a gated
installed Skyrim.esm record case check state before and after restoration.
FallrimTools preserves both words without applying them, and CommonLib exposes
the authored runtime flag queries without connecting them to save masks.
`Game::restore_vanilla_world_and_vm` runs
the supported restore passes in dependency order and restores the prior SOEG
snapshot if any pass fails. The passes cover globals, NPC and
CLAS/FACT/RELA record changes, reference placement, actor templates,
inventories, reference extras, quests, lists and Papyrus execution. The
individual world passes validate before applying their own changes.
`Game::restore_vanilla_papyrus` resolves saved script-instance RefIDs to live
form objects and imports their saved variables into the VM. Scripts deriving
from Alias bind to `Handle::Alias` on the saved owning quest instead of its
form object. UESP's Papyrus ScriptInstance numeric component is the low two
bits of `unknown_2bits` times 1000 plus the signed `unknown` word. The importer
validates that component against the quest's alias definitions and checks
ReferenceAlias or LocationAlias against the alias kind. Distinct aliases on one quest therefore
retain distinct objects and native dispatch. Existing alias handles persist
through the native save layout, with no new saved field. The `sav_19_` unit
checks cover distinct handles, the signed word, missing aliases and alias kind
rejection. FallrimTools keeps these instance header fields separate from variable
data. Form objects still use one object per form. Distinct saved inventory item
script objects targeting one form therefore remain unsupported, as reported by
the SAV-19 corpus survey in [godot-host.md](godot-host.md).
`Game::restore_vanilla_papyrus_execution` also converts and schedules
validated active script frames, rolling back VM state if the import fails.
Saved reference-only objects are created using their saved runtime type.
Instances whose signed
`unknown` field is `-1` ignore their saved RefID and receive standalone VM
objects. Values reach the existing Papyrus native path and engine save state.
`Game::resume_vanilla_quest_story_events` delivers saved `SCPT` contexts as
`OnStoryScript` after its handler is loaded. It passes the six mapped arguments
in order and clears the saved context only when the VM accepts the event.
Contexts without a loaded handler or with unresolved mapped references remain
saved for a later call.
Queued messages and suspended stacks remain inert. Unsupported change-form
types and the remaining world and VM state are not imported by this transaction.
This is not a full ESS load or resumed game. See
[save.md](save.md) for mapping rules, failure behavior and the remaining work.
`Game::restore_vanilla_save_file` opens an ESS path and sets the SPID save
character ID from field two of the standard nine-field filename before running
those passes. For a nonstandard filename it retains the host supplied game
player ID, which defaults to zero. The ID remains runtime metadata and is not
written into SOEG or SOEW.

Reference placement reserves created identities found in saved change forms,
the FormID array and the four Created Objects tables before runtime placement
can allocate another identity. Regular and light plugin IDs map by their saved
owner names. Their masks follow Mutagen's `FormID.cs`, while the FormID array
and Created Objects table order follow UESP's save format. The reservation
also materializes entries in those tables into the shared record lookup as
dynamic `ENCH` or `ALCH` records before references are installed. Their saved
effect forms are relocated through the saved plugin map. Fields absent from
the ESS table remain zero, and the created records use the existing record
fields are persisted in the `SAV-17` world section. Inventory restoration
requires each holder base to have a live record, applies item deltas by saved
identity and reports inventory items whose records are absent. Created Objects decoding follows UESP
`Created_Objects` and skymp's reader and writer field order in `soe-save`.

`World::restore_vanilla_record_changes` validates CLAS changes and atomically
applies FACT and RELA fields to record overrides. FACT reaction records feed
the faction runtime state, and FACT crime values and RELA data are read from
the overridden records. `Actor.GetRelationshipRank` falls back to matching
RELA records when no runtime relationship value exists. Record overrides
persist through SOEW save and load.
xEdit defines CLAS tag-skills as a null payload, so it changes no runtime
field. The `SAV-46` world save section retains the saved form flags and their
undocumented 16-bit word.

**Forms, not structs.** Every native takes and returns forms by
`GlobalFormId`. The VM sees them as objects created on first use with the
class the record's signature implies (`QUST` is a `Quest`, `ACHR` an `Actor`),
so a form with no script still answers its class's natives. The VM gained
`create_object_of` and `NativeCall::create_object` for this.

**Destructible references.** `ObjectReference.DamageObject` reads the base
record's DEST header and ordered DSTD stages. Damage lowers reference-owned
destruction health, advances every crossed percentage threshold, and queues
`OnDestructionStageChanged` with the prior and current stage numbers. DSTD flags can cap damage,
disable the reference, mark it destroyed, or ignore external damage. The
`World::destruction_model` returns the latest nonzero-stage DMDL through the
current stage, so later stages without a replacement keep the last model. The
Godot host hides the base model and draws that stage model through its
RenderingServer path while the reference is resident.
`GetCurrentDestructionStage` returns `-1` before a stage and the
zero-based DSTD position afterward. `ClearDestruction` restores full health,
clears the destroyed flag and returns the stage to `-1` without queuing a
stage-change event. The CK page reports that cleared destruction effects do
not return after later damage, so the reference retains a cleared marker.
Health and stage are saved in the reference-level `CMB-37` section. SCR-40
stores that marker and `SetNoFavorAllowed` state in a separate reference
section. `SetNoFavorAllowed` defaults to true and controls whether a follower
can use the reference as a favor. DEST field widths and flags
follow `TES5Edit/Core/wbDefinitionsTES5.pas`. The UESP format corpus indexes
DEST and its subrecords but does not include the linked DEST field page. The
Creation Kit `DamageObject` and `GetCurrentDestructionStage` pages establish
native behavior and the undamaged return value.

`KnockAreaEffect` and `PushActorAway` enqueue impulses through CMB-38. The
host consumes queued vectors once. See the CMB-38 section below for argument
validation and the documented attenuation approximation.

**Natives cannot touch the VM.** They run inside a VM thread. Anything they
want the VM to do, an `OnActivate` event, an `OnInit`, a stage fragment call,
goes into `World::pending`, and `Game::run_until_idle` alternates draining that
queue with running the VM until no runnable work remains. Deferred death
requests stay queued until EndDeferredKill. Timers are the same: `tick`
moves due registrations into the queue as `OnUpdate` events.

`Game.QuitToMainMenu` and `Game.StartTitleSequence` enqueue transient
`HostRequest` values on the world for Godot to consume. Those requests are not
saved because they describe host presentation actions, not game state. The
host drops them after each runtime tick.

**Provider collisions.** `Game::add_natives` runs a second provider (an
extender in the PAPER mould) and returns the functions it lost. First
registration wins, the registry never replaces silently. This is the answer to
the map's third open seam.

The four `Debug` script and stack profiling natives delegate to the VM's
transient profiler. Stop calls write Chrome trace JSON profile files, with
per-function VM instruction time and call counts, under the configured profile
directory. Script profiling uses the supplied script name, case-insensitively.
Stack profiling uses the current Papyrus thread. A failed file write is added
to the Papyrus log. `Debug.TakeScreenshot` queues its path for the host to
capture from the active viewport.

**Debug globals.** `Debug.SetGodMode` writes `World::debug_god_mode`. While
enabled, incoming player health damage is ignored by melee, magic and
`DamageActorValue` paths. Other actors and non-health actor values keep their
normal damage behavior.
`Debug.ToggleAI` flips `World::debug_ai_enabled`, and `Game::tick` skips the
package driver while it is false. `Debug.ToggleCollisions` flips
`World::debug_collisions_enabled`. The Godot host applies this flag to resident
cell collision bodies and player query collision bodies. `Debug.DumpAliasData`
logs each runtime alias id and its filled form
or `empty`, and logs a diagnostic when its argument is not a quest or has no
runtime quest state. `Debug.DebugChannelNotify` logs its two string arguments
as `[channel] message`. The CK `Debug` class page documents these global
functions. The local Skyrim Platform function catalog supplies the two-string
signature for `DebugChannelNotify`, which is absent from that CK page.
Player movement continues to use the `soe-physics` `SimpleWorld` solver.

**Semantics.** Consult the local Creation Kit function pages and shipped PSC
wrappers before implementing natives. `engine-refs/ck-wiki/` contains 402
wikitext pages as of 2026-09-19. When an individual page is absent, class pages
and `engine-refs/skse64/scripts/vanilla` can establish the contract.

`conditions.rs` evaluates `GetIsPlayableRace` and the race part of `IsChild`
from the actor base's winning RACE DATA flags. `HasAssociationType` and
`HasFamilyRelationship` scan winning RELA records, resolve their actor bases
and ASTP association, and read the ASTP Family Association flag.
`HasParentRelationship` respects RELA's authored parent-to-child direction.
`GetFactionCombatReaction` reads the first FACT's XNAM entry for the second
FACT, defaulting to neutral when no relation is authored. These record layouts
come from `TES5Edit/Core/wbDefinitionsTES5.pas`. Association, family, parent
and child wording comes from the corresponding CK pages. The local CK corpus
has no usable semantics for `GetClassDefaultMatch` or `GetIsCreatureType`, so
both return the evaluator's explicit unsupported-function error.
The CTDA evaluator also reads actor ghost and unconscious flags for functions
237 and 242, checks a tested reference's explicit XOWN against an `NPC_` or
FACT owner for function 278, and reads the tested reference's parent CELL
interior flag for function 300. `IsOwner` with no owner parameter checks the
player's `NPC_` base, matching the optional-owner note in
`TES5Edit/Core/wbDefinitionsTES5.pas`. The local CK corpus has no usable
semantics for functions 262 `IsWarningAbout`, 274 `IsSmallBump`, 285
`IsLeftUp` or 305 `GetPlayerAction`, so these remain unsupported with a
function diagnostic. Function 577 `IsCloserToAThanB` compares the subject's
world distance to both reference parameters and is true only when it is
strictly closer to the first. Invalid reference parameters return a form
diagnostic. Functions 608 `GetKeywordDataForAlias`, 611
`IsNullPackageData`, 612 `GetNumericPackageData`, 631
`IsEnteringInteractionQuick`, 638 `IsInFriendStateWithPlayer`, 642
`GetLastBumpDirection` and 645 `GetIsInjured` remain unsupported with a
function diagnostic because the local CK corpus provides no usable condition
semantics and the required alias keyword, package or actor-process state is
not represented. The reference conditions are covered by
`ai_73_reference_conditions.rs` and `ai_77_reference_conditions.rs`.
`ai_70_reference_state_conditions.rs` covers condition 27 `GetLineOfSight` by
reading the latest successful actor perception step, condition 36 `MenuMode`
from the host's generic menu-mode flag for parameter 0, and condition 39
`GetDisease` by checking active effects for the winning `MagicDisease` keyword.
xEdit supplies function names and parameter types. The local CK corpus has no
CTDA pages for these functions. The LOS behavior follows the CK `Actor.HasLOS`
description, menu state follows `Utility.IsInMenuMode`, and disease matching
follows the CK `HasMagicEffectWithKeyword` disease example. The runtime does
not retain menu identities, so nonzero menu IDs return an explicit diagnostic.
`World::evaluate_detection` replaces the transient LOS pairs only after the
whole observation batch succeeds. LOS and menu state are not saved.
`ai_79_relationship_conditions.rs` covers condition functions 42 `SameFaction`,
43 `SameRace`, 44 `SameSex`, 60 `GetFactionRankDifference`, 68 `GetIsClass`,
129 `GetPCIsClass` and 130 `GetPCIsRace`. These read actor factions, race,
sex and class from the runtime actor and its winning `NPC_` base. Function 60
subtracts the parameter actor's rank in the requested faction from the tested
actor's rank, treating an absent faction as rank -2 and preserving authored
rank -1 membership. Function 64 `GetIsCreature` remains unsupported because
the local CK corpus does not define a usable condition contract.
`ai_82_relationship_conditions.rs` covers 453 `GetPlayerTeammate`, 615
`GetHighestRelationshipRank`, 616 `GetLowestRelationshipRank`, 617
`HasAssociationTypeAny` and 618 `HasFamilyRelationshipAny`. Teammate state
comes from the runtime actor. Relationship extrema use winning authored RELA
ranks, decoding Lover through Archnemesis as 4 through -4. This follows the CK
note that the Papyrus highest-rank call does not account for runtime changes.
Association and family queries check all RELA records for the actor base.
Functions 449 `GetFactionRelation`, 454 `GetPlayerTeammateCount` and 637
`IsExitingInstant` remain unsupported because the local CK pages define no
usable condition semantics for them. Function ids and RELA fields come from
`TES5Edit/Core/wbDefinitionsTES5.pas`. Behavior follows the CK pages for
`IsPlayerTeammate`, `GetHighestRelationshipRank`, `GetLowestRelationshipRank`,
`HasAssociation` and `HasFamilyRelationship`.
`ai_83_relationship_conditions.rs` covers 641 `IsUnique` by reading the
tested actor's winning NPC_ base flag, with a form diagnostic for a non-actor
subject and parameter diagnostics for authored values on unused slots. CK's
`ActorBase.IsUnique` page defines the flag query and xEdit supplies function
641 with no parameters. The CK corpus provides no usable condition contract
for 703 `IsExitingInteractionQuick` or 715 `IsUndead`, so both remain
unsupported with explicit function diagnostics.
[The reference audit](reference-audit.md) records missed contracts and gaps.
Its SCR-57 section compares all 26 registered Debug globals and both
EffectShader methods with the local CK and PSC contracts, including quiet
stubs and the host behavior they still require.

**Game presentation natives, SCR-29.** `Game.SetHudCartMode` updates the
transient `World::hud_cart_mode` flag. `ShowRaceMenu`, `TriggerScreenBlood`
and `PlayBink` enqueue typed `GameUiRequest` values for the host, preserving
the supplied value, file name and playback flags. An empty Bink file name
returns `GameUiError::EmptyVideoName` and queues nothing. Because these
Papyrus calls return void, rejected requests are recorded in
`World::native_errors`. The existing `AdvanceSkill` native delegates to
`World::advance_skill`. Invalid use is recorded there as a native diagnostic.
The signatures and short behavior descriptions come from the local
`ck-wiki/Game_Script.wikitext`, since individual pages for these five
functions are absent. The host displays cart mode, the race editor and a
timed screen blood overlay. `PlayBink` reaches the host as a signal with all
four options, but Bink decoding and playback are not implemented. Skill
effects use the record-backed AVIF and GMST path documented in
[progression](progression.md).

**World reference placement.** `World::ref_placement` (in `world/refs.rs`) is
the read-only view a host uses to keep rendering and collision resources in
step with a reference's runtime position, rotation, scale and enable state. It
returns `None` for a reference never instantiated, whose record-authored
placement is still current. A deleted reference remains present with
`visible: false` so the host can release its resources. Host physics may write
finite poses through `World::set_host_ref_pose`. Other host presentation reads
placement without mutating it. The host contract and its split between
collision and rendering change detection are documented in
[godot-host.md](godot-host.md#runtime-enable-and-placement-reflection-hst-04).

`Actor.SetEssential` delegates to `World::set_actor_essential`. For actors
with a base-data source it updates that source and initialized inheriting
actors, using the same path as `ActorBase.SetEssential`. Baseless actors update
their runtime flag. `GetSitState` reports 0 without a Sit procedure, 2 while
that procedure is approaching furniture, and 3 after arrival. Transient
animation states 1 and 4 are unavailable because the host does not report sit
transition animations. `GetFlyingState`
returns the live movement state when present, otherwise 3 for the AI Hover
procedure and 0 in other cases.

`SetVehicle` stores a placed-reference target on the actor. `None` and the
actor itself detach it. `SetLookAt` stores its placed-reference target and
pathing argument, and `ClearLookAt` clears the target. The actor section
`SCR-11` persists these relationships and they are included in the
determinism hash. `World::actor_vehicle` and `World::actor_look_at` expose the
values. `World::tick_actor_script_relationships` applies target movement and yaw
changes to the actor's shared reference placement. It rotates the actor's
vehicle-relative offset with the target, updates its cell when the target has a
cell, turns the actor toward a live look target, and supplies the target to the
AI movement step when the look request enables pathing. That step routes toward
the live target, updates its route when the target moves and stops within its
arrival radius. Disabling or changing a pathing request clears the prior forced
route. The AI tick applies the relationship update, and the host observes
changed actor placement through the existing reference change path. Save
loading clears the transient vehicle motion baseline so the first post-load
tick uses restored placements.

The bindings follow the local CK `SetVehicle_-_Actor.wikitext`,
`GetSitState_-_Actor.wikitext` and `GetFlyingState_-_Actor.wikitext` pages, plus
the `Actor_Script.wikitext` signatures and descriptions. The CK sources define
the vehicle attach/detach contract and flying-state codes. The engine keeps
sit transition animation states unavailable because the host does not report
them.

**Sun and camera natives, SCR-26.** `Game.GetSunPositionX/Y/Z` return the
current time-driven sun position in Skyrim units, on the runtime's 100,000-unit
orbit. The model places the sun east at 06:00, overhead at 12:00 and west at
18:00, relative to the player's current placement. `Game.IsPlayerSungazing`
compares the host camera direction with the sun direction and returns true
within the runtime's one-degree angular cutoff. These orbit and cutoff values
are runtime model choices. The local CK `Game_Script.wikitext` describes the
coordinate getters and says sungazing checks whether the player looks directly
at the sun. The shipped `Game.psc` confirms their global signatures. Neither
source specifies an orbit formula, radius, or angular tolerance. The tests
`scr_26_sun_orbit_uses_100000_unit_radius_and_clock_cardinals` and
`scr_26_sungazing_uses_one_degree_angular_cutoff` pin the chosen model.
`Game.ShakeCamera` queues a transient request for the host. Its source defaults
to the player, strength defaults to 0.5 and clamps to 0 through 1, and duration
zero uses `fCameraShakeDuration` or a 0.25-second fallback. Invalid source
types, non-finite values and negative durations do not queue an effect. These
camera requests are transient and are not saved. The shipped `Game.psc` and CK
page define the native signature and description. Shipped script defaults
provide shake defaults and clamping.

### Debug natives, SCR-20

`Debug.OpenUserLog(name)` creates one transient named log and returns false for
an empty or already-open name. `CloseUserLog(name)` removes it and is a no-op
when it is absent. `TraceUser(name, text, severity)` appends the formatted text
to an open named log and the Papyrus log, and returns whether that log was
open. User logs are runtime diagnostics and are not included in SOEW saves.

TOOL-07 adds profile-controlled file output. A host passes the resolved
`soe_profile::Settings` and the profile's My Games directory to
`Game::configure_papyrus_logs`. `[Papyrus] bEnableLogging` enables
`Logs/Script/Papyrus.0.log` and `Logs/Script/User/<name>.0.log` output.
`bEnableTrace` controls non-error trace lines, while `bLoadDebugInformation`
controls source-line detail in VM errors. Existing numbered log files move up
one slot when logging starts. Lines use the vanilla-style
`[MM/DD/YYYY - H:MM:SSAM]` timestamp prefix. VM log entries
continue through `soe-trace`, including when file tracing is disabled.

`Debug.CenterOnCell(name)` resolves a CELL editor id and queues a host request
to move the player to that cell's origin. `CenterOnCellAndWait(name)` queues
the same request and suspends its Papyrus thread until the host installs the
destination, then returns 0. An unknown cell logs an error and queues no
request. `Debug.QuitGame()` queues a host request to quit the Godot scene tree.
The Game crate does not perform rendering, cell-resource loading or process
shutdown itself.

The local CK source is `ck-wiki/Debug_Script.wikitext`, which documents the
global names, signatures and summaries, and
`ck-wiki/CenterOnCell_-_Debug.wikitext`, which documents the cell argument and
notes the vanilla function can freeze. Separate named pages for the other
four functions were absent from this corpus. SCR-20 tests cover native
arguments, duplicate and missing names, queued host effects and latent
completion. This host path does not reproduce the freeze note.

### Debug presentation natives, SCR-23

`Debug.SetFootIK(enabled)` changes a transient world setting. The current actor
renderer has no foot-IK solver, so this setting does not change rendered
poses. `Debug.ShowRefPosition(reference)` accepts a
resolved placed reference, records its identity once, and logs an invalid
reference when the object does not resolve to a placed reference.
`Debug.PlayerMoveToAndWait(editorId)` resolves the destination reference,
queues a host move, and suspends the Papyrus thread. The Godot host applies the
placement and resolves the wait with 0 after destination resources install.
Missing references log an error and return 0. Failed host installation returns
-1. `Debug.MoveToCell`
resolves a CELL editor id and moves the player to the cell origin. An unknown
cell logs an error and leaves the player unchanged.

The local `ck-wiki/Debug_Script.wikitext` documents the listed signatures and
summaries, and `CenterOnCell_-_Debug.wikitext` documents the cell teleport
argument and its warning about freezing. Individual pages for the four SCR-23
functions are absent from the corpus. Host rendering displays each requested
reference position as a small emissive sphere using `RenderingServer` instances.
Foot IK remains a presentation gap because no solver or ground-contact input is
available in the actor renderer.

`ObjectReference.GetCurrentScene` searches active SCEN records and resolves
their participant alias IDs through the owning quest. It returns the first
matching scene in sorted FormID order, or None if the reference is not an
active participant. `PlayAnimationAndWait` validates the animation and event
names, queues a request and latent wait by target reference, and returns true
after that target reports the named graph event. The host fails the wait if
the bound visual unloads first. The host does not dispatch the requested
animation on arbitrary references, and neither native fabricates a graph
event.
`PlayGamebryoAnimation` returns false because legacy NIF playback is not
implemented. It does not report success without starting an animation.
`SetMotionType` validates the documented motion range and queues the motion
request for the host. Motion type 7 maps to a kinematic host body.
`ApplyHavokImpulse` rejects zero-length or non-finite direction data,
logging the Creation Kit Papyrus warning for a zero-length vector. It
normalizes finite directions without intermediate float overflow and queues
the magnitude and direction for the host. Placed-reference Havok behavior
remains a stub. These contracts follow the local
`GetCurrentScene_-_ObjectReference.wikitext`,
`PlayAnimationAndWait_-_ObjectReference.wikitext`,
`PlayGamebryoAnimation_-_ObjectReference.wikitext`,
`SetMotionType_-_ObjectReference.wikitext` and
`ApplyHavokImpulse_-_ObjectReference.wikitext` pages.

**Quest alias dependencies.** The `QUST` reader retains local alias IDs from
`ALFA` and external quest and alias IDs from `ALEQ` plus `ALEA`.
`World::resolve_alias_ref` follows those local and external ID chains in
dependency order. A cycle returns `AliasResolutionError::Cycle` with the cycle
path, and resolution does not write partial alias values.
Unfilled dependency aliases have no runtime override. An explicit empty
override suppresses dependency resolution and survives save/load.
`World::alias_ref` preserves its optional result and returns `None` when a
dependency cycle prevents resolution. The layout follows `wbRecord(QUST)` in
`engine-refs/TES5Edit/Core/wbDefinitionsTES5.pas` and
`engine-refs/uesp-modfileformat/QUST.wikitext`. The CK `Alias_Script` page
establishes that aliases are objects owned by their quest. Fill selection for
location reference types and `ALFI` forced-into copying are not implemented.

**Quest transition queries.** `Quest.IsStarting` and `Quest.IsStopping` are
registered natives backed by restored quest lifecycle tasks. Quest start and
stop normally complete synchronously, so both queries return false then. A
transitional startup or shutdown task restored from a save is reported until
the lifecycle scheduler resumes it. See SAV-10 in BACKLOG.csv.

**World reference placement. World reads out, host never writes back.**
`World::ref_placement` (in
`world/refs.rs`) is the read-only view a host uses to keep its own rendering
and collision resources in step with a reference's runtime state: current
position, rotation, scale and enable, in the small, `Copy` `RefPlacement`
struct, not the mutable `RefState` a native or VM path uses to change that
state. It returns `None` for a reference never instantiated, whose
record-authored placement is still current. A deleted reference remains
present with `visible: false` so the host can release its resources. The host
side of this contract, and the split it takes
between collision and rendering change detection, is documented in
[godot-host.md](godot-host.md#runtime-enable-and-placement-reflection-hst-04).
`World::set_host_ref_pose` accepts finite position and rotation from a live
rigid body for an enabled, non-deleted, non-actor reference. The host calls it
after a physics step, and rendering reads the resulting placement in the same
tick. It writes the existing saved fields, so the world save format is
unchanged. A late pose from a disabled or deleted body is rejected.

**CommonLib world reads.** The MIT `soe-commonlib-shim` boundary exposes
player identity, player position and actor values, plus the actor forms whose
runtime state is built. `World` implements these reads directly through its
player, reference and actor methods. The shim's `FormKey` retains both load
order file and object identity. It does not keep a parallel copy of world
state. Projectile runtime access returns an explicit unsupported-hook result
because projectile simulation is not implemented. The matching local
CommonLib declarations are `BSPointerHandle.h`, `PlayerCharacter.h`,
`ProcessLists.h` and `Projectile.h` in the alandtse reference checkout. The
shim exposes source-level C++ declarations, with no general FFI bridge.

`World::commonlib_extra_worn`, `commonlib_game_setting`, `commonlib_hazard`,
`commonlib_high_process` and `commonlib_resolve_hit` project current inventory,
setting, placed hazard, actor combat-process and resolved melee-hit state into
the MIT shim types. Hazard reads require a placed reference whose base record
is HAZD. High-process reads include combat membership, target and stagger only.
Hit resolution runs through the existing melee combat path. The SHM-11 game
tests cover live settings, worn markers, present and absent hazards, actor
combat process state, and a resolved hit. These Rust adapters do not provide a
C++ to Rust binding.

condition API also has a request/reply bridge for compiled C++ condition
calls. The game handles each request against the current world and VM. This
transport does not provide general plugin loader services.
`Game.PrecacheCharGen` and `Game.PrecacheCharGenClear` enqueue transient host
requests. The host loads model files for head parts offered by playable races
and their chargen TRI files before processing later race menu requests. It
processes these host requests before emitting the race menu presentation
signal. The clear request drops those retained file buffers and resets their
residency and byte accounting. These requests add no saved state.

`Game.ShowTrainingMenu(actor)` queues a training-menu request containing the
actor form when the argument resolves to an actor reference. Invalid or missing
arguments return quietly without a request. Valid trainers also queue the
UI-16 presenter request. The host opens that menu once and consumes the typed
request without presenting it again.

**Witnessed crime.** `crime.rs` decodes `FACT.CRVA`, the bounty table a
faction charges for murder, assault, trespass, pickpocketing, theft
(scaled by a steal multiplier against the stolen item's value), escape
and being seen as a werewolf. Layout came from `wbRecord(FACT` in
`TES5Edit/Core/wbDefinitionsTES5.pas` and `uesp-modfileformat/FACT.wikitext`.
The struct grew across record versions, 12, 16 or 20 bytes, and earlier
fields stay at the same offset as later ones are appended, so a short
struct still lines up with the start of a long one.
`World::record_witnessed_crime` (`world/crime.rs`) prices a crime against
the perpetrator's own crime faction, the one `Actor.GetCrimeFaction` and
`SetCrimeFaction` name (ck-wiki, same page titles), and adds the result to
that faction's bounty and infamy, split violent or nonviolent the same way
`Faction.ModCrimeGold` already does. It also records that the witness saw
that perpetrator do it, in the new `ActorState::witnessed_crimes` set,
carried in SOEW version 31. Every check, witness is an actor, perpetrator
is an actor with a crime faction, that faction tracks crime, runs before
anything is mutated, so a bad call changes nothing. `CRGR`, the crime
group that would let one faction's bounty spread to its allies, is not
parsed. This API charges only the perpetrator's faction. A `FACT` with no
`CRVA` subrecord prices every crime at zero rather than failing, since a
faction can legally exist without one. No Papyrus native calls this
function yet. A surviving target hit by the player is the witness for a melee
assault. The combat path uses the victim's crime faction and its `CRVA`
assault price, unless that faction's `DATA` sets Ignore Assault. NPC attacks
and lethal player hits do not add an assault bounty. The victim's witnessed
perpetrator set and the faction's violent bounty are updated through the
existing crime recorder. `Faction.SendAssaultAlarm` also records a player
assault using the nearest built actor in its faction as witness and marks that
actor alarmed.

**Crime conditions, AI-43.** `IsIntimidatedbyPlayer` (116) reads the subject's
`LastBribedIntimidated` actor value, as documented by CK `IsIntimidated -
Actor` and `SetIntimidated - Actor`. `IsTrespassing` (145) reads the subject's
runtime `trespassing` flag, which is saved in the `AI-43` actor section.
`GetIsCrimeFaction`
(152) compares the subject's crime faction with its required `FACT` parameter,
matching CK `GetCrimeFaction - Actor`. These predicates reject nonzero unused
parameters and invalid actor or faction context. The trespassing flag is also
included in the world state hash. `GetCrime` (122),
`GetTrespassWarningLevel` (144), `IsPCAMurderer` (176),
`GetAmountSoldStolen` (190) and `GetIgnoreCrime` (192) remain unsupported with
`conditions::Error::Function`: TES5Edit gives their signatures, but the local
CK corpus supplies no usable return contract for these condition functions.
**Jail sentence accounting.** `Game.ServeTime` serves the player's tracked
crime faction at one in-game day per 100 gold of bounty, rounded up and
capped at seven days. It advances the shared clock, clears that faction's
violent and nonviolent bounty, clears partial skill-use experience, and adds
the served days to the player's `days_in_jail`. The counter and serving
faction are stored in the `PRG-13` actor save section. Release uses that
faction even if the player's current crime faction changes. When the winning
FACT supplies a valid `PLCN` reference, ordinary player inventory moves to
that player belongings chest.
Stacks with an owner extra move to the faction's valid `STOL` evidence chest
when it is present. Inventory changes are prepared before commit, and missing
or invalid chest references leave the affected items with their current
holder. `Game.ClearPrison` clears the served-day counter and transfers the
contents of the serving faction's `PLCN` back to the player. Contents in
`STOL` remain evidence. Jail marker travel through `JAIL`, outfit changes
through `JOUT`, and guard movement remain unimplemented. The FACT field
definitions follow `TES5Edit/Core/wbDefinitionsTES5.pas` and
`uesp-modfileformat/FACT.wikitext`.
`Faction.SendPlayerToJail` serves the receiver faction's bounty through this
accounting path and marks the player arrested after a positive sentence. Its
inventory-removal option removes any inventory left with the player after
confiscation. The real-jail option still produces a Papyrus trace because
jail-marker travel is not implemented.

**Guard arrest choices, PRG-12.** `World::arrest_options` offers payment only
when the player has enough `Gold001`, jail for a positive bounty, and
resistance for any valid arrest. It reads arrest policy from the first byte
of the winning faction's `CRVA` and requires the faction's `DATA` Track Crime
flag. `World::resolve_arrest` applies only the selected choice. Payment
deducts the combined bounty and clears its violent and nonviolent ledgers.
Jail uses `serve_jail_time`, including its clock and skill-progress effects.
Resistance leaves the purse, bounty, infamy and jail state unchanged.
The `FACT.DATA`, `CRVA`, `JAIL`, `STOL`, `PLCN` and `JOUT` fields follow
`TES5Edit/Core/wbDefinitionsTES5.pas` and `uesp-modfileformat/FACT.wikitext`.
The arrest flag and `IsArrested` status name the local CK faction and actor
contracts. Jail marker travel, evidence storage, belongings restoration and
combat behavior after resistance are not handled here.
**Crime conditions.** CTDA functions 458, 459, 497, 499, 533 to 535 and 652
read the actor witness set, faction crime ledgers, player gold and jail-day
counter, or actor crime-faction links. Functions 459 and 533 to 535 accept an
explicit `FACT` parameter or use the evaluated actor's crime faction when the
parameter is null. Function 497 compares that actor faction's bounty with the
player's `Gold001` count. Function 499 reads the player's accumulated served
days. Function 458 is true when the evaluated actor witnessed the player
commit a crime. Function 652 compares the evaluated actor's crime faction with
the actor reference parameter. xEdit names these conditions and their
parameters. The CK corpus has no pages for the CTDA condition functions, so
their detailed behavior is inferred from the matching Papyrus faction and
actor contracts and the runtime crime state. Invalid actor and form context
returns a condition error.

`Actor.SendTrespassAlarm` calls `World::record_witnessed_crime` with its self
actor as witness and the argument as the trespasser. Invalid or untracked
actors leave bounty and witness state unchanged and add a native trace.
`Actor.StopCombatAlarm` clears the actor's combat target, combat flag and alert
flag, and clears combatants targeting that actor, through
`World::stop_combat_alarm`. `ShowBarterMenu` queues a transient
host menu request for the actor. `OpenInventory` queues the actor inventory
menu for teammates, or for any actor when `abForceOpen` is true. Invalid
argument counts and types are reported in the Papyrus trace. `ClearForcedMovement`
clears the actor's current package destination and navmesh route, then marks
the package state for reevaluation. The package assignment remains intact.
The local CK corpus has no named function page for `ClearForcedMovement`.
SKSE's vanilla `Actor.psc` describes it as restoring the actor's standard
state, so the runtime clears its currently represented package route.
The host barter panel displays the resolved merchant stock. Price calculation,
purchase controls and selling items remain outside the panel.

**Prison flag clearing (SCR-27).** `Game.ClearPrison` clears the player's
`ActorState::arrested` flag, the same state read by `Actor.IsArrested` and
cleared by `Actor.ClearArrested`. It also resets the player's cumulative
served-day counter. `Game.ServeTime` applies the remaining jail time.
`Game.SendWereWolfTransformation` records the werewolf crime with the first
sorted loaded high-process actor whose shared detection state marks the player
Alert or in Combat. `Game.SetSunGazeImageSpaceModifier` validates and saves an
optional IMAD form, then applies it when the player starts looking at the sun.
`Game.ShakeController` queues finite strengths and a non-negative duration for
the Godot host, clamping strengths to the host's 0 to 1 range. Invalid
controller-shake values are appended to the world's native error collection
without queuing a request.

**Vendor service lookup.** `Game::resolve_vendor_service` reads the winning
`FACT` vendor flag, `VEND`, `VENC`, `VENV`, `PLVD` and faction conditions. It evaluates
conditions for a loaded merchant with the player as target and applies the
authored hours. It filters direct and leveled stock from the merchant container
through the buy/sell list, including its inverse-list flag, and reads gold
from the same container. An unloaded merchant has an
unavailable service with no exposed merchandise or gold, matching the CK
`ShowBarterMenu_-_Actor` note. The result retains the service radius and
stolen-item policy. Leveled stock uses the shared LVLI resolver, which
retains entry metadata. `Game::buy_vendor_item` accepts an already-adjusted
unit price and exchanges listed stock for player gold. It requires the
merchant service to be available and prepares both holder inventories before
committing either. Insufficient stock or gold and inventory overflow leave
both holders unchanged. The exchange emits the ordinary inventory events for
each side. SkyUI's `BarterMenu.as` confirms a selected quantity with its
per-item price, while `BarterDataSetter.as` applies buy/sell multipliers and
rounding before the transaction callback. Price calculation, selling items
to a merchant and the visible barter menu remain outside this API.
The location check supports near-reference, in-cell, near-editor-location and
near-self modes. Other PLVD modes return a typed error. The 16-bit `VENV`
radius and location modes follow CommonLibSSE `TESFaction.h` and UESP
`FACT.wikitext`.

**ObjectReference.Reset.** `World::reset_reference` rebuilds a record-backed
reference from its winning REFR or ACHR record, including authored transform,
enable state, lock metadata, and inventory from the base object. It clears
runtime reference fields held in `RefState`. The optional target supplies the
destination position and cell only while the reset reference's cell is
resident. Actor health and death state
use the existing resurrection path. Actor faction and relationship state is
stored separately and remains unchanged. All reference fields are already in
SOEW, so reset adds no save section. This follows the local CK
`Reset_-_ObjectReference` contract. Dynamic references without a winning
placement record cannot be reset.

Reset selects leveled base inventory again through the shared LVLI resolver.
Its chance-none draw can produce different contents from first construction.
Native save/load preserves the selected inventory and RNG without another
draw. SAV-71 checks reset after loading changed references, winning plugin
overrides, fixed and leveled inventory, actor state and failed cyclic-list
selection. The installed check samples 32 fixed and 32 leveled containers.
The two SAV-68 installed equality checks select fixed CNTO containers.

The SAV-71 historical check found both installed failures already present at
`3547411b`, before the requested bisect range. All ten SAV-68 checks pass at
`f9db8db7`. Its child `f16d16ac` (UI-25) fails both installed checks because it
materializes leveled contents during reference construction and Reset. Their
former first container can roll empty initially and four items on Reset,
before any save is written. This follows the local UESP `LVLI.wikitext`
chance-none contract and CK `Reset_-_ObjectReference` inventory reset contract.
No saved field, version or section changes.

**ObjectReference.TetherToHorse.** The native moves the supplied horse by the
CK page's local cart offset (72, 300, 0) Skyrim units, rotates that offset by
cart yaw, copies the cart rotation and cell, then queues the pair for host
physics. The host waits until both references have resident collision bodies
before creating their pin joint, using the CK chassis pivot `[0, 3.5, -0.2]`
and horse pivot `[0, 0, -0.5]` in Havok units. A missing, invalid or
self-reference argument leaves the world unchanged. The CK
`TetherToHorse_-_ObjectReference.wikitext` page names `HorseSpine2` as the
horse connection. The runtime connects whole reference bodies and does not
sample that bone or build the cart wheels, pole and suspension rig from NIF
data. Replacing a tether or unloading its collision cell frees the old joint.

`ObjectReference.TranslateTo`, `SplineTranslateTo` and `StopTranslation` are
registered but remain quiet stubs. Their movement state, completion and failure
events belong to HST-16's world translation behavior, which is not present in
this worktree. The CK pages describe absolute destinations, movement speed,
optional maximum rotation speed and spline tangent magnitude. This runtime does
not yet apply those arguments.

**Facial expression override, ANM-59.** `Actor.SetExpressionOverride` stores a
CK expression id from 0 through 16 and strength from 0 through 100 on the
actor. `ClearExpressionOverride` removes it. `World::facial_expression`
returns the active override, or maps the actor's record-backed AIDT mood to
the corresponding mood expression at full strength after clearing. The
override is an actor save section named `ANM-59` and participates in the
determinism hash. The host maps expression ids to head blend shape names and
loads matching expression deltas from the head's companion `.tri` file.

**Enchanting.** `Game::enchant_item` accepts one selected weapon or armor
instance, one selected soul gem stack, an ENCH form and the caller's learned
enchantment set. It checks the selected soul level against the gem's SLCP
capacity from SLCP and its per-instance ExtraSoul value, writes
ExtraEnchantment to only that item instance, consumes the gem, and queues its
inventory removal event as one inventory transaction. The ENCH
record and ExtraEnchantment fields follow `TES5Edit/Core/wbDefinitionsTES5.pas`
and CommonLibSSE-NG `ExtraEnchantment.h`, `ExtraSoul.h` and `TESSoulGem.h`.
The runtime does not yet own learned enchantment state, so the caller supplies
that set. Enchantment strength and charge calculation are not part of this
transaction.
**Magic conditions.** CTDA function 571 returns the cast type when its selected
left or right spell is the actor's held concentration cast. Function 595 checks
that same current spell, while 596 checks the selected spell's magic effect
keywords. Function 579 compares the equipped shout, and 632 reports a held
concentration cast. Casting source values outside Left, Right, Voice and
Instant return a parameter error. Voice and Instant have no selected spell in
this runtime, so spell comparisons return false and casting type returns zero.
Functions 602, 627 and 676 read host-reported actor state. Function 602
compares the supplied ward state against xEdit's None, Absorb and Break values.
Function 627 returns the host's dual-casting flag, and 676 returns the current
shout voice variation as an integer-valued condition result. `World` setters
signal these values, and unset state returns an unavailable-state diagnostic.
These host signals are transient and reset when an actor is rebuilt after load.
They participate in the determinism hash while present.
The condition ids and parameter types follow local xEdit
`TES5Edit/Core/wbDefinitionsTES5.pas`. The ward enum is also defined there.
CommonLibSSE-NG's `Actor::IsDualCasting` reads the high-process flag, which
supports treating this as animation/process state. The local CK dump has no
condition pages for these three natives, so shout variation's numeric value is
passed through as reported by the host without interpreting its range.

**Follower services.** `World::recruit_follower` validates the actor,
service faction and follow package. It enables teammate status and the
requested favor permission, joins the faction at rank zero, and adds the
service package before the actor's ordinary AI candidates. Dismissal restores
the actor's prior teammate and favor values and its previous rank or absence
in that faction, then removes the package overlay. The normal alias and base
package stack remains underneath and becomes eligible again on reevaluation.
This follows the CK `SetPlayerTeammate - Actor` description of teammate and
favor flags. SOEW version 37 stores favor permission and active follower
service ownership, including the values dismissal restores.

**Equipment conditions.** CTDA identities and parameter types follow
`TES5Edit/Core/wbDefinitionsTES5.pas` and
`uesp-modfileformat/Raw_Function_Data.wikitext`. CTDA 636 checks the subject
actor's equipped WEAP records for the two-handed sword, axe, bow and crossbow
animation types in xEdit's weapon enum. CTDA 660 resolves its keyword and
checks the explicit tempering item supplied by the condition caller. Perk
entry-point condition contexts carry that item through
`EntryPointContext::tempering_item`. CTDA 723 reads the kind-37 ExtraHealth
value from a loose item reference, defaults to 1.0 when the subject has no item
extra, and preserves damaged values below 1.0, following
`ck-wiki/GetItemHealthPercent_-_ObjectReference.wikitext`. CTDA 664 retains
an unsupported-function diagnostic because the local sources identify its
parameter type but provide no usable evaluation contract.

**Head tracking flag, SCR-12.** `Actor.SetHeadTracking` stores the requested
boolean on an actor. `World::head_tracking` returns that value, or `true` when
no script has set it. The explicit value is persisted in the actor section
`SCR-12` and included in the determinism hash. The host does not yet apply the
flag to authored head-tracking bones. `Actor.SetLookAt` stores its placed
reference target and pathing option, and `Actor.ClearLookAt` clears both. The
actor section `SCR-11` persists these relationships and includes them in the
determinism hash. `Actor.ShowGiftMenu` creates a latent gift-menu request. The
transfer behavior is described under UI-15.

`Faction.ModReaction` adds its amount to that faction's reaction toward the
given faction, defaulting an absent reaction to neutral and saturating at the
`i32` bounds through `World::mod_faction_reaction`. The shipped
`skse64/scripts/vanilla/Faction.psc` supplies the argument contract.
`Faction.SendAssaultAlarm` chooses the nearest built actor belonging to its
faction, marks it alarmed, then records the player's assault through the
witnessed-crime API.
The CK `SendAssaultAlarm_-_Actor.wikitext` page describes the actor alarm as
simulating an assault by the player. `Faction.SendPlayerToJail` routes its
receiver faction through the existing sentence accounting API. The vanilla
PSC declares the inventory and real-jail flags. Inventory removal is applied
when requested. Real-jail travel still produces a trace because jail-marker
travel is not implemented.

**LeveledSpell and Location native contracts.** `scr_65_registry_inventory_matches_location_and_leveled_spell_contracts`
checks the registered methods for `LeveledItem`, `LeveledActor`,
`LeveledSpell` and `Location` against the local scripts. The vanilla SKSE
`LeveledSpell.psc` defines `AddForm(Form, int)` and `Revert()`. Its modified
script adds `GetChanceNone`, `SetChanceNone`, `GetNumForms`, `GetNthForm`,
`GetNthLevel` and `SetNthLevel`. Spell lists do not expose `GetNthCount`.
`SetNthLevel` updates only an existing entry and clamps levels above 65535.
`Location.psc` defines the seven implemented Location methods. Its
`IsSameLocation` wrapper accepts identical locations, a shared ancestor with
the requested keyword, and a keyworded parent/child pair in either direction.
`IsLoaded`, `HasRefType`, `GetRefTypeAliveCount` and `GetRefTypeDeadCount`
remain quiet stubs owned by SCR-35. The corresponding Creation Kit named
pages are not present in the local corpus, so the shipped scripts are the
contract source for these bindings.

## CommonLib world views, SHM-16

`World::commonlib_data_handler` returns the MIT `TESDataHandler` view over winning records and
load-order files. A lookup exposes the resolved runtime FormID, record signature, editor ID
and associated keywords. `World::commonlib_reference` builds a reference through the normal
world path, then exposes its current base, transform, cell and enabled/deleted flags through
the MIT `TESObjectREFR` contract. Its nullable pointer stores the reference's runtime FormID.
File views retain the load-order filename and regular or light slot.
`shm_16_populated_world_views_reach_cpp_mit_contracts` samples non-empty winning
records, regular and light files, reference transform and base, and drawn actor
state from these adapters, then passes those values to a compiled C++ consumer
that exercises the MIT contracts. Unsupported `SetBaseObject` and trampoline
hooks are checked in the C++ fixture. This checks populated cross-language
contract values without providing a C++ to Rust ABI.

`World::commonlib_actor` exposes the already-instantiated actor through the same MIT trait.
Actor-value reads and writes use the shared actor values, and weapon state maps the actor's
`weapon_drawn` field to `Drawn` or `Sheathed`. The world does not yet track animation transition
states. `SetBaseObject` and invalid actor-value requests return an unsupported result without
changing state. Tests exercise these views against a synthetic load order. No installed-data
records or SKSE binary plugin are required for this contract.

**CommonLib event channels.** The shared world exposes the MIT shim's
`ActorKill` and `CriticalHit` event sources. Non-silent deaths with a valid
actor killer dispatch `ActorKill` after the victim enters dead state. The
critical-hit reporting method validates an actor and winning WEAP record,
then sends the caller-confirmed event. Critical chance and damage modifiers
are not calculated by this reporting method. The listeners live only for the
current World instance and are not saved. The SHM-20 channels dispatch after a
successful shout cast, player skill rank gain, successful soul-gem fill, new
player spell addition and streamed cell arrival. Their payloads resolve from
the same world and load order. Binary hooks return explicit unsupported errors.
A game integration fixture forwards these live events to compiled C++ sinks
over a test pipe and checks the responses. This test transport is not an engine
plugin ABI. See [shim](shim.md#event-binding-batch-shm-18) for the projected C++
payload contract and unsupported binary hook boundary.

## Conformance table

### ActiveMagicEffect and Actor native contract audit, SCR-02

The registry binds methods by declaring class. Actor also resolves inherited
ObjectReference and Form methods through the Papyrus class chain. There are no
global bindings declared on Actor or ActiveMagicEffect. The Effect object's
own bindings are listed below. Each is a method registration, not a Form
registration. CK's corpus has no ActiveMagicEffect function pages, so those
contracts come from the shipped `skse64/scripts/vanilla/ActiveMagicEffect.psc`
in `engine-refs`.

| Binding | Contract comparison | Disposition |
|---|---|---|
| ActiveMagicEffect.GetBaseObject | Returns the MagicEffect form attached to this active effect. | Returns the effect form from its active-effect record. |
| ActiveMagicEffect.GetTargetActor | Returns the actor this spell effect targets. | Returns the recorded target actor. |
| ActiveMagicEffect.GetCasterActor | Returns the actor that cast the spell, or None. | Returns the recorded optional caster. |
| ActiveMagicEffect.Dispel | Removes this active effect. | Queues removal for the effect represented by this script object. |
| ActiveMagicEffect.AddInventoryEventFilter | Accepts a Form filter for item added and removed events. Skyrim does not support None. | Adds the filter under this script object's identity. None logs a Papyrus native error and leaves the filter set unchanged. |
| ActiveMagicEffect.RemoveInventoryEventFilter | Removes the matching Form filter. | Removes matching entries for this script object. |
| ActiveMagicEffect.RemoveAllInventoryEventFilters | Removes all filters for this effect. | Clears this script object's filter set. |
| ActiveMagicEffect.RegisterForSingleUpdate | Takes an interval in seconds and schedules one update. | Uses the real-time update scheduler with repeat disabled. |
| ActiveMagicEffect.RegisterForUpdate | Takes an interval in seconds and schedules repeating updates. | Uses the real-time update scheduler with repeat enabled. |
| ActiveMagicEffect.RegisterForSingleUpdateGameTime | Takes an interval in game hours and schedules one update. | Uses the game-time scheduler with repeat disabled. |
| ActiveMagicEffect.RegisterForUpdateGameTime | Takes an interval in game hours and schedules repeating updates. | Uses the game-time scheduler with repeat enabled. |
| ActiveMagicEffect.UnregisterForUpdate | Stops real-time update delivery. | Removes this object's real-time update registration. |
| ActiveMagicEffect.UnregisterForUpdateGameTime | Stops game-time update delivery. | Removes this object's game-time update registration. |
| ActiveMagicEffect.RegisterForAnimationEvent | Takes sender and event name, returns whether registration succeeded. | Rejects a missing sender, de-duplicates the listener and returns success. |
| ActiveMagicEffect.UnregisterForAnimationEvent | Takes sender and event name and removes the registration. | Removes only the matching listener and event. |

The CK `Actor_Script.wikitext` member and Special Edition sections were
compared against each Actor-class registration in `natives.rs`. Named
function pages were opened for registered contracts with argument-specific
semantics, including actor values, death, combat, faction membership,
equipment, relationship ranks, spell management, outfit selection, sneaking,
restraint, and active magic. The concrete contract differences found were:

| Binding | CK contract | Current registry behavior | Owning work |
|---|---|---|---|
| Actor.GetCombatState | Returns 0 not in combat, 1 in combat, or 2 searching. | Reads `in_combat` as 1, otherwise 0. Searching remains unsupported because actor state has no search state. | SCR-10. |
| Actor.GetEquippedWeapon | Returns the left or right hand weapon selected by its Boolean argument. | Reads the actor's selected hand, with the right hand as default. | SCR-14. |
| Actor.GetEquippedShield | Returns the currently equipped shield. | Returns equipped inventory armor whose resolved BOD2 mask includes shield slot 9. | SCR-14. |
| Actor.KeepOffsetFromActor and ClearKeepOffsetFromActor | Store or clear a target actor and body-space offset request. | Store finite offsets and angles with validated catch-up and follow radii in shared actor state. | SCR-14. |
| Actor.SetForcedLandingMarker | Store the reference an actor should use when it lands. | Retain the selected reference in shared actor state. | SCR-14. |
| Actor.HasLOS, Actor.PathToReference | Use viewer/target or target-reference arguments to query or start navigation. | HasLOS accepts finite positions for references in the same cell. PathToReference returns true only for a valid speed and an already reached target. | SCR-10. |
| Actor.IsFlying | Reports the actor's flight state. | Reads live movement flight flags or the Hover package procedure. | SCR-10. |
| Actor.GetSleepState, Actor.GetSitState, Actor.GetFlyingState | Return the documented state code. | GetSleepState maps the sleep procedure and sleeping flag to 0, 2 or 3. | SCR-10 and SCR-11. |
| Actor.EquipItem | `abPreventRemoval` prevents replacement through EquipItem. `abSilent` suppresses the equip message. | The actor retains protected equipped forms, rejects replacement and direct unequip for them, and queues an `Equipped <name>` notification unless silent. `UnequipAll` and `RemoveAllItems` clear the protection. Actor protection survives save/load in the `SCR-74` actor section. | The local CK `EquipItem` and `UnequipItem` pages specify the replacement, removal, and bulk-removal rules. |

`Actor.GetActorValueMax` and its `GetAVMax` alias include current modifiers.
The value path reads the shared actor-value components. `Actor.StartSneaking`
toggles sneaking when called on an already sneaking actor, matching the
function page note. `Actor.SetRestrained(true)` clears unconscious state,
matching the mutually exclusive state described by its function page. The
`scr_02_every_actor_and_effect_binding_has_a_ck_contract_comparison` walks
every direct Actor and ActiveMagicEffect registry method and requires a
matching declaration in `tests/fixtures/scr_02_ck_contracts.txt`. That manifest
records CK Actor member signatures, vanilla Actor PSC declarations for methods
missing from the CK class page, and shipped ActiveMagicEffect PSC native
signatures. The separate binding-kind checks confirm both classes have no
registered globals. `Actor.IsProtected` is compared with the CK ActorBase
member declaration because the runtime query reads the actor base flag. The CK
scrape spells `IsHostileToActor` with a trailing hyphen, which is normalized in
the manifest because Papyrus identifiers do not allow that character. The
registry binding `Actor.SetAllowPCDialogue` was corrected to the declared
`Actor.AllowPCDialogue` method. These checks establish declaration coverage and do not
claim runtime parity for state the engine does not model yet. The local CK and
vanilla PSC sources have no `RemoveAllSpells` declaration. Its registry entry
and behavior remain owned by SCR-16. They also have no `Trespass` declaration,
whose behavior remains owned by SCR-15. Both absent contracts are recorded in
the manifest.

The SCR-02 contract-shape regression compares the local `EquipItem` CK
declaration, including its `Form` and `Bool` parameter types, both false
defaults and its `None` return, with the native invocation. Calling the native
with only the required item exercises those defaults: it equips the item,
emits the ordinary equip notification and returns `None`. `SCR-74` owns the
prevent-removal and silent-message behavior and tests those explicit arguments.

`Actor.KeepOffsetFromActor` stores the target, required body-space XYZ offset,
optional XYZ angles and catch-up/follow radii on the calling actor. Omitted
optional arguments use the CK defaults. Targets must be actors, all numeric
values must be finite, and the catch-up radius must be at least the
nonnegative follow radius.
`ClearKeepOffsetFromActor` removes that request. AI movement rotates the
body-space XYZ offset by the target's yaw, applies its relative XYZ angles,
stops inside the follow radius and selects run speed outside the catch-up
radius. `SetForcedLandingMarker` stores a resolved reference on the actor, and
accepts `None` to clear it. When a Hover flight package ends, AI movement routes
the actor to that marker and retains the in-flight state until it arrives.
Arrival at a prior package destination does not cancel or suppress the landing
route.
Landing uses record-backed walk speed when the package has no positive speed.
These values persist in the actor section named `SCR-14`.
The local CK pages `KeepOffsetFromActor - Actor`,
`ClearKeepOffsetFromActor - Actor` and `GetEquippedWeapon - Actor` define
the offset and weapon argument contracts. `Actor_Script.wikitext` defines the
forced-marker contract and names the shield signature, while the local corpus
has no dedicated pages for those two functions.
`ClearForcedLandingMarker - Actor` says that clearing is equivalent to
assigning `None`.

The SCR-10 tests cover the state codes and argument boundaries described
above. `HasLOS` does not query collision geometry, and `PathToReference` does
not start a latent movement request. GetSleepState cannot report the documented
wants-to-wake state 4 because the actor state has no wake transition. These
bindings remain limited until the runtime has those owning behaviors.

`natives::register` has 388 stateful registrations and 163 quiet stubs,
including `CreateDetectionEvent`, `Is3DLoaded`, `Actor.IsAlarmed`,
`Actor.IsBleedingOut`, `Actor.IsRunning`, `Actor.IsSprinting` and
`Actor.IsHostileToActor`. The alarm and bleedout queries read actor state,
while the movement queries read the running and sprinting flags supplied by
runtime movement. `IsRunning` excludes sprinting, matching the CK distinction.
`IsHostileToActor` is true when the calling actor is in combat
and its current combat target is the supplied actor. A non-actor argument
returns false.
`natives::register` includes the implementations and quiet stubs listed below.
The registration test checks the hand-kept stateful count and rejects duplicate
names. It includes `CreateDetectionEvent`, `Is3DLoaded`, the Actor alert,
bleedout and movement queries, shader-particle controls, EffectShader playback
and Utility's frame-rate functions. `IsAlarmed` reads the actor's alert flag.
`IsBleedingOut` requires an actor to be unconscious, alive and at or below its
record-derived bleedout threshold. `Utility.IsInMenuMode` reads the state most
recently supplied to `tick_menu_mode`. A quiet stub returns the idle value the
game gives when nothing is going on, so vanilla scripts can continue when their
owning system has no behavior yet.

The SCR-61 registry audit found four `GetID` and `GetName` method bindings.
`Form.GetName` returns the form's name-column text, `Quest.GetID` returns its
EditorID, and `Alias.GetID` and `Alias.GetName` return the alias's numeric ID
and unique name. These match the CK pages `GetName - Form`, `Quest Script`,
and `Alias Script`. Form.GetName returns an empty string for actor and
object-reference instances, as the CK page specifies. Regression coverage is
in `tests/scr_61_native_contracts.rs`. The Form page identifies `GetName` as
an SKSE member.

| Class | Implemented | Stubbed |
|---|---|---|
| Form | GetFormID, GetName, GetType, HasKeyword, GetKeywords, RegisterForSingleUpdate, RegisterForUpdate, RegisterForSingleUpdateGameTime, RegisterForUpdateGameTime, UnregisterForUpdate, UnregisterForUpdateGameTime, RegisterForSleep, UnregisterForSleep, RegisterForTrackedStatsEvent, UnregisterForTrackedStatsEvent, RegisterForAnimationEvent, UnregisterForAnimationEvent, RegisterForLOS, RegisterForSingleLOSGain, RegisterForSingleLOSLost, UnregisterForLOS | |
| ObjectReference | Enable, EnableNoWait, Disable, DisableNoWait, IsEnabled, IsDisabled, Is3DLoaded, Delete, IsDeleted, GetBaseObject, GetParentCell, GetWorldSpace, IsInInterior, GetPositionX/Y/Z, GetAngleX/Y/Z, SetPosition, SetAngle, MoveTo, GetDistance, Activate, BlockActivation, IsActivationBlocked, GetLinkedRef, GetItemCount, AddItem, RemoveItem, RemoveAllItems, GetScale, SetScale, GetDisplayName, SetDisplayName, DamageObject, GetCurrentDestructionStage, ClearDestruction, SetNoFavorAllowed, PlayAnimation, PlayAnimationAndWait, WaitForAnimationEvent, GetOpenState, SetOpen, Lock, IsLocked, GetLockLevel, SetLockLevel, PlaceAtMe, AddToMap, EnableFastTravel, SetDestroyed, IgnoreFriendlyHits, IsIgnoringFriendlyHits, GetCurrentLocation, GetEditorLocation, IsInLocation, MoveToMyEditorLocation, GetActorOwner, GetFactionOwner, SetActorOwner, SetFactionOwner, AddInventoryEventFilter, RemoveInventoryEventFilter, RemoveAllInventoryEventFilters, SetAnimationVariableFloat/Int/Bool, GetAnimationVariableFloat/Int/Bool, GetNthLinkedRef, PlaceActorAtMe, GetCurrentScene, CreateDetectionEvent, TetherToHorse, ForceAddRagdollToWorld, ForceRemoveRagdollFromWorld, Reset | IsFurnitureInUse, PlayGamebryoAnimation, SetMotionType, ApplyHavokImpulse, KnockAreaEffect, PushActorAway and 6 more, plus the DLC furniture marker and translation functions: IsFurnitureMarkerInUse, TranslateTo, SplineTranslateTo, StopTranslation |
| ObjectReference | Enable, EnableNoWait, Disable, DisableNoWait, IsEnabled, IsDisabled, Is3DLoaded, Delete, IsDeleted, GetBaseObject, GetParentCell, GetWorldSpace, IsInInterior, GetPositionX/Y/Z, GetAngleX/Y/Z, SetPosition, SetAngle, MoveTo, GetDistance, Activate, BlockActivation, IsActivationBlocked, GetLinkedRef, GetItemCount, AddItem, RemoveItem, RemoveAllItems, GetScale, SetScale, GetDisplayName, SetDisplayName, DamageObject, GetCurrentDestructionStage, ClearDestruction, SetNoFavorAllowed, PlayAnimation, PlayAnimationAndWait, WaitForAnimationEvent, GetOpenState, SetOpen, Lock, IsLocked, GetLockLevel, SetLockLevel, PlaceAtMe, AddToMap, EnableFastTravel, SetDestroyed, IgnoreFriendlyHits, IsIgnoringFriendlyHits, GetCurrentLocation, GetEditorLocation, IsInLocation, IsInDialogueWithPlayer, MoveToMyEditorLocation, GetActorOwner, GetFactionOwner, SetActorOwner, SetFactionOwner, AddInventoryEventFilter, RemoveInventoryEventFilter, RemoveAllInventoryEventFilters, SetAnimationVariableFloat/Int/Bool, GetAnimationVariableFloat/Int/Bool, GetNthLinkedRef, PlaceActorAtMe, GetCurrentScene, CreateDetectionEvent, TetherToHorse, ForceAddRagdollToWorld, ForceRemoveRagdollFromWorld, Reset | IsFurnitureInUse, PlayGamebryoAnimation, SetMotionType, ApplyHavokImpulse, KnockAreaEffect, PushActorAway and 6 more, plus the DLC furniture marker and translation functions: IsFurnitureMarkerInUse, TranslateTo, SplineTranslateTo, StopTranslation |
| Actor | GetActorValue, GetBaseActorValue, SetActorValue, ModActorValue, DamageActorValue, RestoreActorValue, GetActorValuePercentage (and the AV short names), ShowGiftMenu, SetExpressionOverride, ClearExpressionOverride, IsDead, Kill, GetKiller, Resurrect, IsInCombat, GetCombatState, GetSleepState, StartCombat, StopCombat, GetCombatTarget, GetActorBase, GetLeveledActorBase, IsEssential, SetEssential, IsPlayerTeammate, SetPlayerTeammate, IsGhost, SetGhost, IsProtected, GetLevel, GetRace, IsUnconscious, SetUnconscious, SetRestrained, IsDoingFavor, SetDoingFavor, AddSpell, RemoveSpell, RemoveAllSpells, HasSpell, EquipItem, UnequipItem, UnequipAll, IsEquipped, GetEquippedWeapon, GetEquippedShield, IsInFaction, GetFactionRank, AddToFaction, RemoveFromFaction, SetFactionRank, AddPerk, RemovePerk, HasPerk, SetAlert, IsAlerted, IsAlarmed, IsBleedingOut, IsRunning, IsSprinting, IsHostileToActor, StartSneaking, IsSneaking, DrawWeapon, SheatheWeapon, IsWeaponDrawn, IsArrested, ClearArrested, SetAttackActorOnSight, SetDontMove, SetAllowPCDialogue, SetRelationshipRank, GetRelationshipRank, GetHighestRelationshipRank, GetLowestRelationshipRank, GetCrimeFaction, SetCrimeFaction, SetOutfit, and from the DLC: SendVampirismStateChanged, SendLycanthropyStateChanged, SetAllowFlying, SetAllowFlyingEx, IsAllowedToFly, CanFlyHere, StartDeferredKill, EndDeferredKill, KillSilent, SetRace, SetEyeTexture, SetCriticalStage, AttachAshPile, SetNoBleedoutRecovery, GetNoBleedoutRecovery, EnableAI, AddShout, RemoveShout, EquipShout, UnequipShout, GetEquippedShout, EquipSpell, UnequipSpell, SetSubGraphFloatVariable, PlaySubGraphAnimation, WornHasKeyword, IsPlayersLastRiddenHorse, SetAlpha, PlayIdle, HasMagicEffect, HasMagicEffectWithKeyword, DispelSpell, DispelAllSpells, StartVampireFeed | EvaluatePackage, SetLookAt and 34 more, plus DLC methods without a system: IsFlying, GetWarmthRating, StartCannibal, ClearExtraArrows, DoCombatSpellApply, MoveToPackageLocation, WillIntimidateSucceed |
| ActorBase | GetSkin, SetSkin, GetRace, GetClass, GetGiftFilter, GetSex, GetDeadCount, IsEssential, IsProtected, IsInvulnerable, IsUnique, SetEssential, SetProtected, SetInvulnerable, SetOutfit | |
| Quest | Start, Stop, Reset, IsRunning, IsStarting, IsStopping, IsCompleted, IsActive, SetActive, CompleteQuest, CompleteAllObjectives, FailAllObjectives, SetStage, SetCurrentStageID, GetStage, GetCurrentStageID, GetStageDone, IsStageDone, SetObjectiveDisplayed, SetObjectiveDisplayedNoWait, SetObjectiveCompleted, SetObjectiveFailed, IsObjectiveDisplayed, IsObjectiveCompleted, IsObjectiveFailed, ModObjectiveGlobal, UpdateCurrentInstanceGlobal, GetAlias, GetAliasById, GetAliasByName, GetID, GetPriority, GetQuest | |
| Alias, ReferenceAlias, LocationAlias | TryToEnable, TryToEnableNoWait, TryToDisable, TryToDisableNoWait (ReferenceAlias only), GetOwningQuest, GetID, GetName, GetReference, GetRef, GetActorReference, GetActorRef, ForceRefTo, ForceRefIfEmpty, Clear, GetLocation, ForceLocationTo, AddInventoryEventFilter, RemoveInventoryEventFilter, RemoveAllInventoryEventFilters | |
| LeveledItem | AddForm, Revert, GetNumForms, GetNthForm, GetNthLevel, SetNthLevel, GetNthCount, SetNthCount, GetChanceNone, SetChanceNone | |
| LeveledActor | AddForm, Revert, GetNumForms, GetNthForm, GetNthLevel, SetNthLevel, GetNthCount, SetNthCount, GetChanceNone, SetChanceNone | |
| LeveledSpell | AddForm, Revert, GetNumForms, GetNthForm, GetNthLevel, SetNthLevel, GetChanceNone, SetChanceNone | |
| Alias, ReferenceAlias, LocationAlias | GetOwningQuest, GetID, GetName, GetReference, GetRef, GetActorReference, GetActorRef, ForceRefTo, Clear, GetLocation, ForceLocationTo, AddInventoryEventFilter, RemoveInventoryEventFilter, RemoveAllInventoryEventFilters, plus Form identity, keyword and event-listener methods | |
| ReferenceAlias | ForceRefIfEmpty | |
| LeveledItem, LeveledActor, LeveledSpell | AddForm, Revert, GetNumForms, GetNthForm, GetNthLevel, GetNthCount, GetChanceNone, SetChanceNone (the last six are SKSE's, the state is there anyway) | |
| Location | GetKeywordData, SetKeywordData, IsCleared, SetCleared, IsChild, HasCommonParent, IsSameLocation | IsLoaded, HasRefType, GetRefTypeAliveCount, GetRefTypeDeadCount |
| Game | GetPlayer, GetForm, GetFormEx, GetFormFromFile, EnablePlayerControls, DisablePlayerControls, GetGameSettingFloat/Int/String, SetGameSettingFloat/Int/Bool/String, GetRealHoursPassed, IncrementStat, QueryStat, IsFastTravelEnabled, EnableFastTravel, SetPlayerAIDriven, SetInChargen, SetBeastForm, TeachWord, UnlockWord, IsWordUnlocked, AddAchievement, GetPlayersLastRiddenHorse, AddPerkPoints, ShowRaceMenu, SendWereWolfTransformation, ServeTime, ClearPrison, SetSunGazeImageSpaceModifier, ShakeController, ShakeCamera | FadeOutGame, RequestSave, ForceFirstPerson, AdvanceSkill and 16 more, plus the DLC sky functions: GetSunPositionX/Y/Z |
| Debug | Trace, TraceUser, TraceConditional, TraceStack, Notification, MessageBox, SendAnimationEvent, OpenUserLog, CloseUserLog, QuitGame, CenterOnCell, CenterOnCellAndWait, TakeScreenshot, SetGodMode, ToggleAI, ToggleCollisions, DumpAliasData, DebugChannelNotify, StartScriptProfiling, StopScriptProfiling, StartStackProfiling, StopStackProfiling, SetFootIK, ShowRefPosition, PlayerMoveToAndWait, MoveToCell | |
| Faction | SetAlly, SetEnemy, GetReaction, SetReaction, ModReaction, GetCrimeGold, GetCrimeGoldViolent, GetCrimeGoldNonViolent, SetCrimeGold, SetCrimeGoldViolent, ModCrimeGold, CanPayCrimeGold, PlayerPayCrimeGold, GetInfamy, GetInfamyViolent, GetInfamyNonViolent, IsPlayerExpelled, SetPlayerExpelled, SetPlayerEnemy, IsFactionInCrimeGroup, SendAssaultAlarm, SendPlayerToJail | - |
| Game | GetPlayer, GetForm, GetFormEx, GetFormEx2, GetFormFromFile, FindClosestActor, FindRandomActor, FindClosestReferenceOfType, FindRandomReferenceOfType, EnablePlayerControls, DisablePlayerControls, GetGameSettingFloat/Int/String, SetGameSettingFloat/Int/Bool/String, GetRealHoursPassed, GetSunPositionX/Y/Z, IsPlayerSungazing, ShakeCamera, ShakeController, SendWereWolfTransformation, SetSunGazeImageSpaceModifier, IncrementStat, QueryStat, IsFastTravelEnabled, SetPlayerAIDriven, SetInChargen, SetBeastForm, TeachWord, UnlockWord, IsWordUnlocked, AddAchievement, GetPlayersLastRiddenHorse, AddPerkPoints, SetCameraTarget, ShowFirstPersonGeometry, SetSittingRotation, ShowTrainingMenu, AdvanceSkill, SetHudCartMode, ShowRaceMenu, TriggerScreenBlood, PlayBink, ServeTime, ClearPrison, FadeOutGame, RequestSave, RequestAutoSave, ForceFirstPerson, ForceThirdPerson, GetCurrentCrosshairRef, GetPlayerGrabbedRef, QuitToMainMenu, StartTitleSequence, PrecacheCharGen, PrecacheCharGenClear | |
| Debug | Trace, TraceUser, TraceConditional, TraceStack, Notification, MessageBox, SendAnimationEvent | OpenUserLog, CloseUserLog, QuitGame, CenterOnCell, SetGodMode and 14 more |
| Utility | RandomInt, RandomFloat, GetCurrentGameTime, GetCurrentRealTime, GameTimeToString, `SetINI*` and `GetINI*` (eight), IsInMenuMode, CaptureFrameRate, StartFrameRateCapture, EndFrameRateCapture, GetAverageFrameRate, GetMinFrameRate, GetMaxFrameRate, GetCurrentBudget, OverBudget | GetCurrentMemory, GetBudgetCount, GetBudgetName. `Wait`, `WaitMenuMode`, `WaitGameTime` are the VM's own |
| GlobalVariable | GetValue, GetValueInt, SetValue, SetValueInt, Mod | |
| Keyword | GetString, SendStoryEvent, SendStoryEventAndWait | |
| Cell | IsInterior, IsAttached, Reset, GetActorOwner, GetFactionOwner, SetActorOwner, SetFactionOwner, SetPublic | |
| FormList | GetSize, GetAt, HasForm, Find, AddForm | |
| Message | Show, ShowAsHelpMessage, ResetHelpMessage (method and global) | |
| Scene | Start, ForceStart, Stop, IsPlaying, IsActionComplete | |
| Sound | Play, StopInstance, SetInstanceVolume | |
| Spell | Cast, IsHostile, RemoteCast, Preload and Unload record validation | |
| Scene, Package | GetOwningQuest | |
| TopicInfo | GetOwningQuest | |
| ActiveMagicEffect | RegisterForUpdate, RegisterForSingleUpdate, RegisterForUpdateGameTime, RegisterForSingleUpdateGameTime, UnregisterForUpdate, UnregisterForUpdateGameTime, RegisterForAnimationEvent, UnregisterForAnimationEvent, GetBaseObject, GetTargetActor, GetCasterActor, Dispel, AddInventoryEventFilter, RemoveInventoryEventFilter, RemoveAllInventoryEventFilters | |
| EffectShader | | Play, Stop |
| VisualEffect | Play, Stop | |

| EffectShader | Play, Stop | |
| VisualEffect | Play, Stop | |
| ImageSpaceModifier | Apply, ApplyCrossFade, Remove, RemoveCrossFade, PopTo | |
| ShaderParticleGeometry | Apply, Remove | |
| MusicType | Add, Remove | |
| Topic | Add | |
| Weather | SetActive | |
| Weapon | Fire | |
| Scroll | Cast | |

Condition functions 84 (`GetDeadCount`), 323 (`GetCombatState`) and 338
(`GetIgnoreFriendlyHits`) use the local CK contracts and xEdit function
identities. Dead count reads the number of runtime-dead actors for an NPC
base, increasing on death and decreasing on resurrection. Combat state reports
0 when the actor is not engaged, 1 when it has a current combat target, and 2
when it remains engaged without a target and is searching. Ignore-friendly-hits
reads the reference flag used by its Papyrus native. The condition functions 63,
66, 118, 288 and 361 remain unsupported with explicit function diagnostics
because the local CK corpus does not provide usable condition semantics for them.

### Utility and VisualEffect contract audit, SCR-72

The registry contains the following 27 Utility and VisualEffect bindings. CK
contracts come from `engine-refs/ck-wiki/Utility_Script.wikitext` and the
shipped `engine-refs/skse64/scripts/vanilla/Utility.psc`. INI getters are
identified by the CK page as SKSE globals. The memory and frame-capture
functions are documented in the CK page as conditional or beta functions.

| Binding | Contract comparison and disposition |
|---|---|
| `Utility.GameTimeToString` | CK and PSC accept elapsed game days and return a calendar date and time. Fixed in SCR-72 to format `MM/DD/YYYY HH:MM`. |
| `Utility.GetCurrentGameTime` | CK and PSC return game days passed. Reads `GameClock.game_days`. |
| `Utility.GetCurrentRealTime` | CK and PSC return elapsed real seconds. Reads `GameClock.real_seconds`. |
| `Utility.IsInMenuMode` | CK reports menu state. Quiet false stub, behavior is owned by SCR-49. |
| `Utility.RandomFloat` | CK and PSC specify inclusive minimum and maximum. Fixed in SCR-72 so both endpoints are representable. |
| `Utility.RandomInt` | CK and PSC specify inclusive minimum and maximum. The world RNG includes both bounds. |
| `Utility.SetINIFloat` | CK specifies storing a float under the named INI key. Stores its string representation in the runtime INI map. |
| `Utility.SetINIInt` | CK specifies storing an integer under the named INI key. Stores its string representation in the runtime INI map. |
| `Utility.SetINIBool` | CK specifies storing a boolean under the named INI key. Stores its string representation in the runtime INI map. |
| `Utility.SetINIString` | CK specifies storing a string under the named INI key. Stores it in the runtime INI map. |
| `Utility.GetINIFloat` | CK identifies the SKSE global getter for a float. Parses the stored value, defaulting to zero. |
| `Utility.GetINIInt` | CK identifies the SKSE global getter for an integer. Parses the stored value, defaulting to zero. |
| `Utility.GetINIBool` | CK identifies the SKSE global getter for a boolean. Returns true for stored `true` or `1`. |
| `Utility.GetINIString` | CK identifies the SKSE global getter for a string. Returns the stored value or an empty string. |
| `Utility.CaptureFrameRate` | CK marks this as beta-only and returns a comma-separated sample string bounded to 1,000 characters. Quiet stub, owned by SCR-49. |
| `Utility.StartFrameRateCapture` | CK marks this as beta-only and starts sampling. Quiet stub, owned by SCR-49. |
| `Utility.EndFrameRateCapture` | CK marks this as beta-only and ends sampling. Quiet stub, owned by SCR-49. |
| `Utility.GetAverageFrameRate` | CK marks this as beta-only and reports the capture average. Quiet stub, owned by SCR-49. |
| `Utility.GetMinFrameRate` | CK marks this as beta-only and reports the active capture minimum. It returns `None` and logs when no frames have been captured. |
| `Utility.GetMaxFrameRate` | CK marks this as beta-only and reports the active capture maximum. It returns `None` and logs when no frames have been captured. |
| `Utility.GetCurrentMemory` | CK says this initializes and returns memory statistics when tracking is enabled. The runtime has no game-owned memory tracker, so this logs an explicit error and returns `None`. |
| `Utility.GetBudgetCount` | CK exposes memory budget metadata only when memory tracking is enabled. The runtime has no game-owned memory tracker, so this logs an explicit error and returns `None`. |
| `Utility.GetBudgetName` | CK exposes memory budget metadata only when memory tracking is enabled. The runtime has no game-owned memory tracker, so valid indices log an explicit error and return an empty string. |
| `Utility.GetCurrentBudget` | Reports measured Papyrus instructions or exterior cell loads for the latest update, indexed by the two engine categories documented below. |
| `Utility.OverBudget` | Compares the measured count with its configured cap for the indexed engine category. |
| `VisualEffect.Play` | The local CK corpus has no VisualEffect function page or shipped VisualEffect PSC. The registered shape is a method. The sibling `EffectShader` PSC describes starting an effect on a target with negative duration meaning indefinite. The shared World retains each call, and the Godot host presents active RFCT effects as described in [godot-host](godot-host.md#papyrus-visual-effects-scr-52). Authored ARTO model lifecycle belongs to MAG-24. |
| `VisualEffect.Stop` | The local CK corpus has no VisualEffect function page or shipped VisualEffect PSC. The registered shape is a method. The sibling `EffectShader` PSC describes stopping an effect on a target. The shared World removes matching calls and the host releases their render instances. |

The latent `Utility.Wait`, `WaitMenuMode` and `WaitGameTime` calls are
registered by the Papyrus VM rather than `game::natives::register`, so they are
not part of this registry inventory. Their clock contracts are documented in
the VM section above.

`Actor.UnequipAll` clears equipped inventory markers, weapon and spell hands,
and the equipped shout while retaining the actor's known spells and inventory.
`Actor.RemoveAllSpells` clears known spells and spell hand assignments while
leaving unrelated equipment in place. `StartVampireFeed` queues the documented
`OnVampireFeed` event on the calling actor when its target resolves to a
sleeping actor. `StartCannibal` queues the `CannibalStart` graph event on the
calling actor when its argument resolves to a dead actor. `ClearExtraArrows`
is a no-op because the runtime does not model attached-arrow extra data. The
CK native clears arrows from actor 3D, so host renderer support remains a
prerequisite for implementing this behavior.

The SCR-23 Debug natives update runtime debug state or the shared player
reference. `SetFootIK` stores the requested setting, defaulting to enabled.
The actor renderer has no foot IK solver, so changing this setting does not
change rendered poses. `ShowRefPosition` records a resolved placed reference
for development display. The Godot host displays each requested position as
an emissive sphere using `RenderingServer` instances. `PlayerMoveToAndWait`
resolves a destination reference by editor id, queues a host move, and
suspends the Papyrus thread until the host installs the destination placement
and cell. The host resolves the wait with `0.0` after installation or `-1.0`
if the move cannot be applied. A missing destination logs an error and
returns `0.0`. The CK page defines a float return but does not describe its
value. `MoveToCell` resolves a cell by editor id and moves the player to its
origin. Failed destination lookups leave the player in place and write a
Papyrus log entry. The local source is
`ck-wiki/Debug_Script.wikitext`.

## ForceRefTo and Form contract audit

The SCR-59 inventory checks the registered `Form` methods against the local
Creation Kit pages. `Form_Script.wikitext` lists the base methods, while
`GetName` and `GetType` appear in its SKSE member section. `GetKeywords` has no
CK or SKSE declaration there. It is an engine extension that returns the form's
keyword array. The inventory test checks that all 21 methods remain registered.

| Binding | CK contract source |
|---|---|
| `Form.GetFormID()` | `GetFormID_-_Form.wikitext`, integer runtime form ID |
| `Form.GetName()` | `GetName_-_Form.wikitext`, string name, SKSE |
| `Form.GetType()` | `Form_Script.wikitext`, integer type code, SKSE |
| `Form.HasKeyword(Keyword)` | `HasKeyword_-_Form.wikitext`, boolean, false on unsupported forms |
| `Form.GetKeywords()` | No CK declaration, engine extension returning keyword forms |
| `Form.RegisterForSingleUpdate(Float)` | `RegisterForSingleUpdate_-_Form.wikitext`, one update after a real-time interval |
| `Form.RegisterForUpdate(Float)` | `RegisterForUpdate_-_Form.wikitext`, repeating real-time interval |
| `Form.RegisterForSingleUpdateGameTime(Float)` | `RegisterForSingleUpdateGameTime_-_Form.wikitext`, one update after a game-time interval |
| `Form.RegisterForUpdateGameTime(Float)` | `RegisterForUpdateGameTime_-_Form.wikitext`, repeating game-time interval |
| `Form.UnregisterForUpdate()` | `UnregisterForUpdate_-_Form.wikitext`, remove real-time updates |
| `Form.UnregisterForUpdateGameTime()` | `UnregisterForUpdateGameTime_-_Form.wikitext`, remove game-time updates |
| `Form.RegisterForSleep()` | `RegisterForSleep_-_Form.wikitext`, receive sleep events |
| `Form.UnregisterForSleep()` | `UnregisterForSleep_-_Form.wikitext`, stop receiving sleep events |
| `Form.RegisterForTrackedStatsEvent()` | `RegisterForTrackedStatsEvent_-_Form.wikitext`, receive tracked-stat events |
| `Form.UnregisterForTrackedStatsEvent()` | `UnregisterForTrackedStatsEvent_-_Form.wikitext`, stop receiving tracked-stat events |
| `Form.RegisterForAnimationEvent(ObjectReference, String)` | `RegisterForAnimationEvent_-_Form.wikitext`, subscribe to the sender's named event |
| `Form.UnregisterForAnimationEvent(ObjectReference, String)` | `UnregisterForAnimationEvent_-_Form.wikitext`, remove that subscription |
| `Form.RegisterForLOS(Actor, ObjectReference)` | `RegisterForLOS_-_Form.wikitext`, receive gain and lost events |
| `Form.RegisterForSingleLOSGain(Actor, ObjectReference)` | `RegisterForSingleLOSGain_-_Form.wikitext`, receive one gain event |
| `Form.RegisterForSingleLOSLost(Actor, ObjectReference)` | `RegisterForSingleLOSLost_-_Form.wikitext`, receive one lost event |
| `Form.UnregisterForLOS(Actor, ObjectReference)` | `UnregisterForLOS_-_Form.wikitext`, remove matching LOS subscriptions |
| `ReferenceAlias.ForceRefTo(ObjectReference)` | `ReferenceAlias_Script.wikitext`, force the alias to use the specified reference |

The comparison found that `ForceRefTo` accepted any form even though the CK
parameter is an `ObjectReference`. The binding now accepts `REFR` and `ACHR`
records and ignores other forms, retaining the existing alias target. The
SCR-59 tests cover both reference record kinds and check each `Form` method
against the VM registry. CK also specifies that a single LOS registration
can deliver immediately when current visibility already matches. The runtime
only dispatches on a later visibility transition. Initial LOS query and
delivery are assigned to SCR-10, which owns the actor LOS query behavior.

`LeveledItem.GetChanceGlobal` and `SetChanceGlobal` are SKSE methods and are
not registered. Their record and save-state support belongs to the leveled-item
selection work tracked by PRG-05.

## Faction and alias native contract audit

The SCR-58 inventory is checked against the registered methods. The local
`engine-refs/skse64/scripts/vanilla/Faction.psc` supplies the Faction native
signatures, defaults and comments. The CK wiki's `ReferenceAlias_Script`
documents `ForceRefIfEmpty` as a Boolean member of `ReferenceAlias`. The named
Faction function pages are not present in the local CK wiki corpus, so each
Faction contract below is compared to that shipped Papyrus interface and its
comments. No bindings in this inventory are globals.

| Binding | Source contract compared to the registration |
|---|---|
| Faction.SetAlly | Takes another faction and two false-by-default direction flags. A true flag stores Friend for that direction, otherwise Ally. |
| Faction.SetEnemy | Takes another faction and two false-by-default direction flags. A true flag stores Neutral for that direction, otherwise Enemy. |
| Faction.GetReaction | Takes another faction and returns this faction's reaction toward it. Missing relation defaults to Neutral. |
| Faction.SetReaction | Takes another faction and an integer reaction, then changes only this faction's direction. |
| Faction.GetCrimeGold | Returns the combined violent and non-violent bounty. |
| Faction.GetCrimeGoldViolent | Returns the violent bounty. |
| Faction.GetCrimeGoldNonViolent | Returns the non-violent bounty. |
| Faction.SetCrimeGold | Sets the non-violent bounty. |
| Faction.SetCrimeGoldViolent | Sets the violent bounty. |
| Faction.ModCrimeGold | Adds to the violent or non-violent bounty. The second argument defaults false. Positive additions grow matching accumulated infamy. |
| Faction.GetInfamy | Returns accumulated violent and non-violent infamy. |
| Faction.GetInfamyViolent | Returns accumulated violent infamy. |
| Faction.GetInfamyNonViolent | Returns accumulated non-violent infamy. |
| Faction.CanPayCrimeGold | Compares the player's Gold001 count with the faction's combined bounty. |
| Faction.PlayerPayCrimeGold | The source has `abRemoveStolenItems = true` and `abGoToJail = true`. Current code takes Gold001 and clears both bounty fields, but does not honor either flag or the unable-to-pay path. Assign those payment and arrest choices to PRG-12, with jail consequences in PRG-13. |
| Faction.IsPlayerExpelled | Returns this faction's player-expelled flag. |
| Faction.SetPlayerExpelled | Sets the player-expelled flag. The argument defaults true. |
| Faction.SetPlayerEnemy | Sets the player-enemy flag. The argument defaults true. |
| Faction.IsFactionInCrimeGroup | Takes another faction and checks it against this faction's `CRGR` shared-crime FLST. It now resolves the winning FACT record and the winning list entries. |
| Faction.SendAssaultAlarm | Selects the nearest built actor in the faction and records a player assault through the crime system. A missing witness or untracked faction is traced. |
| Faction.SendPlayerToJail | Serves the receiver faction's bounty through jail sentence accounting and marks the player arrested after a positive sentence. Jail travel and belongings storage are not implemented. |
| Faction.ModReaction | Takes another faction and a signed delta, then saturating-adds it to this faction's reaction toward it. |
| ReferenceAlias.ForceRefIfEmpty | Takes an ObjectReference, tries to fill only an empty alias and returns whether it filled it. It is registered on ReferenceAlias, not Alias, so LocationAlias does not inherit it. |

The shipped `ReferenceAlias.psc` also returns false when already filled and
otherwise delegates to `ForceRefTo`. `CRGR` is a form link to an FLST, as
declared by TES5Edit and the UESP FACT layout. Crime-group membership is
derived from those records and adds no saved state.

Current behavior and compatibility limits:

- `GetBudgetCount` reports the two engine categories, `Papyrus instructions per
  slice` and `Exterior cell loads per update`. `GetBudgetName` returns their
  names. `GetCurrentBudget` returns the measured instruction count from the
  most recent VM slice or the exterior cell load count from the most recent
  host streaming update. `OverBudget` compares that measurement with
  `PAPYRUS_INSTRUCTIONS_PER_SLICE` or `EXTERIOR_CELL_LOADS_PER_UPDATE`.
  Invalid scheduler indices return zero and false for integer and boolean
  queries. These runtime scheduler limits are separate from CK memory budget
  metadata. The CK Utility pages document the global signatures, and SKSE's
  vanilla `Utility.psc` declares these as global natives.
`Actor.GetCombatState` reads `ActorState::in_combat`, returning 1 while true
and 0 otherwise. Searching state 2 remains unsupported because the actor
state has no search state. `Actor.GetSleepState`
returns 3 while the AI driver marks the actor sleeping, 2 while its Sleep
procedure is active but not yet sleeping, and 0 otherwise. The driver does not
represent wants-to-wake state 4. These values follow the CK pages
`GetCombatState_-_Actor` and `GetSleepState_-_Actor` in the local `ck-wiki`
corpus.

`Actor.HasLOS` returns true for finite reference positions in the same cell,
and only the player can query a non-actor target. It does not query collision
geometry, clip checks to the player's camera or make the three documented
visibility picks. `Actor.PathToReference` returns true
only when a valid speed is supplied and the target is already at the actor's
position. Other requests need a latent path completion API. `Actor.IsFlying`
reads live actor flight flags or the Hover procedure.
Their CK contracts are described by
`HasLOS_-_Actor`, `PathToReference_-_Actor` and `IsFlying_-_Actor` in the local
`ck-wiki` corpus.

- `ShaderParticleGeometry.Apply(fadeSeconds)` validates the winning, non-deleted
  `SPGD` record and starts or restarts its transient layer. `Remove(fadeSeconds)`
  removes it immediately or fades it out. `World::shader_particle_layers`
  exposes decoded geometry fields and the current fade strength in form order.
  `soe-godot-host` consumes that snapshot and renders one server-side MultiMesh
  per active form, centered on the player. Particle positions are stable for a
  form and index, wrap through the authored box as gravity advances, and use
  the SPGD texture when it can be loaded. The host bounds density to 256 quads.
  Both methods reject malformed record data and non-finite or negative fade
  values before changing the active set. Game ticks advance the fades. These
  effects are transient and are not saved. The CK corpus has no named pages for
  these two methods. The DATA and ICON layout follows xEdit's
  `wbRecord(SPGD)`, UESP's `SPGD.wikitext`, Mutagen's Skyrim SPGD schema, and
  CommonLib's `BGSShaderParticleGeometryData`. UESP's
  `Raw_Function_Data.wikitext` identifies the Play and Stop functions and
  their float arguments.

### Utility runtime queries, SCR-50

`GetMinFrameRate` and `GetMaxFrameRate` read the active frame-rate capture fed
by `Game::tick`. Without captured frames, they return `None` and log that beta
capture data is unavailable. `GetCurrentMemory`, `GetBudgetCount`, and
`GetBudgetName` require memory tracking. The runtime has no game-owned memory
tracker, so `GetCurrentMemory` and `GetBudgetCount` return `None` and log an
explicit error. `GetBudgetName` validates its single integer index, returns an
empty string for invalid indices, and logs an error before returning an empty
string when tracking is unavailable. The scheduler methods `GetCurrentBudget`
and `OverBudget` read separate runtime counters for Papyrus instructions per
slice and exterior cell loads per update. `GetCurrentBudget` reads the
instruction count returned by the most recent VM scheduler slice or the number
of exterior cells committed by the most recent host streaming update. Before a
measurement, each value is zero. `OverBudget` compares that measured value with
its configured cap. The CK Utility page documents these signatures, and SKSE's
vanilla `Utility.psc` declares them as global natives.

The contracts and availability notes come from the local CK
`Utility_Script.wikitext` and shipped `skse64/scripts/vanilla/Utility.psc`.
The host's Godot performance monitors are not used as substitutes for
Creation Engine frame capture or memory budgets.

- `Spell.IsHostile` resolves every effect through the spell's winning plugin
  and returns true when any referenced MGEF has its hostile flag. Missing or
  malformed spell/effect records are logged and return false. `Spell.Cast` and
  `RemoteCast` use the direct-cast path described below. `Spell.Preload` and
  `Spell.Unload` validate the spell record and queue typed art requests. The
  Godot host does not consume them yet. `Topic.Add` validates and records known
  topics.
- The shipped `Topic.psc` declares `Topic.Add()` only. There is no
  `Topic.Remove` native or Spell/Topic global native in the local vanilla
  Papyrus sources.

- `SetStage` on a quest that is not running starts it (`OnInit`, then the
  stage), records the stage in the done set, updates the current and highest
  stage, applies the log entry flags (Complete Quest stops the quest), and
  queues every fragment registered for that stage on the quest's own scripts.
- `Start` delivers `OnInit` and then sets the start-up stage.
- `Activate` runs default processing unless the reference blocks it: a door
  with `XTEL` moves the activator to the far door's cell and landing
  transform, a container marks itself opened, then `OnActivate` goes to the
  target's scripts with the activator. `Activate(x, true)` skips the event.
- `GetStage` and `GetCurrentStageID` both return the highest stage set since
  reset. The shipped `Quest.psc` implements the first by calling the second.
  Internal `QuestState.current` still records the last stage set.
- `ActorBase.GetGiftFilter` reads the optional `NPC_.GNAM` link and returns
  its live `FLST`. A malformed link or one that does not resolve to a live
  FormList logs a native error and returns None. Its record layout follows
  `TES5Edit/Core/wbDefinitionsTES5.pas` and
  `uesp-modfileformat/NPC.wikitext`. The CK `ActorBase_Script.wikitext` page
  defines the return value as the actor base's gift-filter FormList.
- `DispelSpell` removes active applications from the named spell and reports
  whether anything was removed. Neither dispel native changes known or equipped
  spells. `DispelAllSpells` preserves diseases, abilities, addictions and constant
  enchantments. Worn enchantment application is not implemented yet.
- MGEF archetype 28 is labeled Cure Addiction by
  `TES5Edit/Core/wbDefinitionsTES5.pas`. UESP's `MGEF.wikitext` says it has no
  extra data, and CK's `DispelAllSpells_-_Actor.wikitext` only establishes
  that addictions are excluded from general dispelling. Neither source
  specifies the state a Cure Addiction effect changes. The MAG-49 installed
  survey found zero nondeleted winning archetype 28 MGEFs across Skyrim.esm,
  Update.esm, Dawnguard.esm, HearthFires.esm and Dragonborn.esm. The five
  masters therefore provide no execution example. Archetype 28 remains
  unsupported until a source or a mod record establishes its effect.
  enchantments. Equipping an ARMO with an EITM link applies the linked ENCH
  effect entries to the wearer as indefinite active effects. Unequipping the
  item or replacing it in an overlapping armor slot ends those effects and
  reverses their actor-value changes. Unsupported or malformed effects are
  logged by `Actor.EquipItem`. Weapon enchantments remain outside this path
  and are not applied during combat yet.
- `DoCombatSpellApply` invokes the shared spell executor with the calling actor
  as source and caster and the supplied actor as target. The executor logs a
  native error when the spell's delivery or effect is unsupported.
- `MoveToPackageLocation` moves an actor to its selected package location
  center and cell. It logs a native error if that location is unavailable.
- `WillIntimidateSucceed` compares player Speechcraft, after the player's
  eligible `Mod Player Intimidation` PERK entries, with the target's AIDT
  Confidence multiplied by 25. Value functions support Set, Add, Multiply,
  random-range Add, actor-value Add/Set/Multiply and actor-value-scaled
  Multiply. The entry point ID, function encodings and tab order follow UESP
  `PERK.wikitext`. Native name, arguments and result follow CK
  `WillIntimidateSucceed_-_Actor.wikitext`. The native evaluates perk
  conditions with the actor as owner and the subject as target, including
  conditional Papyrus variables from the calling VM.
- `GetWarmthRating` sums equipped ARMO warmth classifications. ARMO records
  with `Survival_ArmorWarm` contribute 54, those with `Survival_ArmorCold`
  contribute 17, and unclassified ARMO records contribute 27. Other equipped
  forms contribute zero. The total and no-argument signature follow CK
  `Actor_Script.wikitext`. The keyword classifications are read from the
  winning ARMO and KYWD records.
- `MusicType.Add` and `MusicType.Remove` validate the receiver as a live `MUSC`
  record and add or remove it from the world's transient active music type list.
  Duplicate additions and removing an inactive type leave the list unchanged.
  The list is not saved. The local CK wiki has no pages for these methods. Their
  no-argument, no-return signatures match the local Skyrim Platform
  `FunctionsDump.txt` entry for `MusicType`.

- `Spell.Preload` and `Spell.Unload` validate the receiver through the winning
  SPEL reader and queue typed art requests. The Godot host does not consume
  these requests yet, so art is not loaded or released. The shipped `Spell.psc`
  describes these as art-resource operations and warns against misuse. A host
  art resource provider remains a prerequisite for the observable effects.
- `VisualEffect.Play` adds a transient effect instance for the RFCT form and
  placed target. Calls stack, negative durations remain until stopped, and a
  finite duration expires on an unpaused game tick. The optional facing
  reference is retained. `VisualEffect.Stop` removes every matching instance
  on its target. Godot follows these instances for host presentation, with
  rendering details and the ARTO limit in [godot-host](godot-host.md#papyrus-visual-effects-scr-52).
- `Weather.SetActive` stores the selected `WTHR` form and its optional override
  and acceleration flags in `World::weather_request`. Both flags default to
  false. A non-weather receiver logs an error and leaves the previous request
  unchanged. The RND-18 world section saves this request and sky transition.
  The host image-space view uses the request's daytime WTHR `IMSP` entry unless
  an explicit image-space selection is active. The shipped `Weather.psc`
  defines the native defaults.
  The RND-18 host sky reads WTHR colors, clouds and transition timing.
- `tick_menu_mode(dt, true)` pauses world time and ordinary waits while
  `WaitMenuMode` continues. `tick(dt)` remains the unpaused entry point.
  The UI must supply the pause state when menus are implemented.
- `Game.SetCameraTarget` accepts an actor form and stores it as the host
  camera target. Non-actor values leave the current target unchanged.
  `Game.ShowFirstPersonGeometry` sets the host's first-person geometry
  visibility state, and `Game.SetSittingRotation` stores the camera offset in
  degrees. These presentation controls are transient and are not saved.
- `Utility.IsInMenuMode` reads the most recently supplied `tick_menu_mode`
  state. Frame-rate calls use valid host tick durations as transient samples.
  `StartFrameRateCapture` resets and starts the sample set, `EndFrameRateCapture`
  freezes it, and the average, minimum and maximum functions read those
  captured samples. An empty capture or invalid lifecycle call logs an error
  and returns zero or no value.
  `CaptureFrameRate` returns the requested recent samples as comma-separated
  frames per second, capped by the Creation Kit contract at 1,000 characters.
  Its frame history is limited to 512 samples and invalid requests log an
  error and return an empty string. These contracts follow the local CK
  `Utility_Script.wikitext` page and shipped `Utility.psc` declarations.
- Inventories start from the base record's `CNTO` list.
- `Actor.GetEquippedWeapon` reads the selected left or right weapon hand, with
  the right hand as its default. `Actor.GetEquippedShield` returns equipped
  inventory armor whose resolved BOD2 mask includes shield slot 9. Both read
  the actor's current equipment state.
- `Cell.IsAttached` reads whether host streaming currently holds the cell.
  `Cell.Reset` queues a record-backed rebuild of its temporary references for
  the next cell load. The pending cell ids are saved in the `SCR-19` world
  section and validated as live CELL forms when read. Prefetch does not attach
  a cell, and host attachment state is rebuilt after loading a save.
- Actor values keep base, permanent, temporary and damage components in one
  store. Set changes base, Mod changes permanent, Force adjusts permanent
  to reach the requested current value, Damage subtracts from the damage
  component and Restore heals that component without changing maximum.
  GetActorValueMax/GetAVMax includes permanent and temporary modifiers.
  GetActorValuePercentage uses that maximum and follows the CK exceptions
  for zero maximum and CarryWeight.
- Actor initialization resolves separate stats and traits template chains.
  Manual attributes use race starting values plus NPC offsets. Manual skills
  use their DNAM values and skill offsets without an additional race bonus.
  UESP's `CLAS.wikitext` race-bonus term is part of the auto-calculated skill
  formula. Fixed-level auto-calculated skills use class weights, actor level
  and race boosts. `PC_LEVEL_MULT`, missing class and zero-weight cases keep
  the authored DNAM skill cache. SOEW saves initialized actor values, so reload
  does not seed bonuses again. Leveled templates return an error pending spawn
  selection. Missing race DATA retains the synthetic-record fallback. See
  [actor values](actor-values.md).
- `Game.AdvanceSkill` records player skill use from the winning AVIF `AVSK`
  multipliers and offsets. The improve multiplier and offset plus
  `fSkillUseCurve` determine experience needed at each current skill level.
  Earned levels update the shared actor-value base and stop at 100. Partial
  skill experience is stored per actor in the `PRG-01` section of SOEW version
  37 actor records. This path applies only explicitly recorded skill use.
- Character experience advances actor levels using
  `fXPLevelUpBase + level * fXPLevelUpMult`. Each crossed threshold adds one
  available attribute choice. Choosing Health, Magicka or Stamina spends one
  choice and adds 10 to that base actor value. Current-level experience and
  unspent choices are stored in the `PRG-07` actor section. The world preserves
  its player-level cache in the `PRG-01.player-level` section. Older saves use
  the restored player actor level, or the existing cache when no player actor
  is present.
- PlayerRef keeps form 0x14 and binds to the Player NPC base when available.
  A base-less synthetic player starts at 100 for the three main values.
  Health reaching zero uses the shared death lifecycle, including essential
  checks, deferred requests and OnDying/OnDeath delivery. Bleedout remains
  unfinished. Healing alone does not resurrect.
- `ShowRaceMenu` queues a transient host request. UI-07 stores a confirmed
  playable race and sex in the player's saved actor-base state before first
  actor initialization. Reopening the menu does not reseed initialized actor
  values. Head-part editing is not implemented.
- `GetAngle*` return degrees, the record stores radians.
- Update timers run on real seconds, the game-time variants on game hours.
- `Game.GetForm` accepts the runtime id under the current load order, light
  plugins included. `GetFormFromFile` takes the low 24 bits and a file name.
- `FormList.AddForm` ignores `None`, suppresses duplicate members and places
  temporary forms before authored entries. Regular forms added by scripts
  follow the authored entries. The four query methods read this ordered list.
- A leveled list is its `LVLO` entries in record order plus what `AddForm`
  appended. `Revert` truncates back to the record's entries and chance.
  Actor templates roll LVLN during initialization. The PRG-05 follow-up rolls
  ordinary LVLI quantities through `World::roll_leveled_items`.
- Location keyword data is run-time only and starts at zero, the record has
  no field for it. `IsChild` walks `PNAM` upward from the candidate,
  `HasCommonParent` intersects the two ancestor chains and applies the filter
  keyword to the shared ancestor, `IsSameLocation` is identity or a filtered
  shared parent.
- `Location.IsLoaded` follows the active cell's `XLCN`. The location and its
  parent locations report loaded while that cell is active. `HasRefType`
  checks the location's `ACSR` and `LCSR` entries. The alive and dead count
  methods count matching `ACHR` entries and read each actor's runtime death
  flag. Wrong or absent `LocationRefType` arguments return false or zero.
  The LCTN array layout follows xEdit's `wbDefinitionsTES5.pas` and UESP's
  `LCTN.wikitext`. Signatures follow the shipped `Location.psc`. The CK
  corpus has no Location-specific pages for these four native behaviors.
- Faction reactions start from `XNAM` and are written in both directions by
  `SetAlly` and `SetEnemy` (friend and neutral through the two flags).
  `ModCrimeGold` adds to the violent or non-violent bounty and to the infamy
  of that kind, `PlayerPayCrimeGold` removes the bounty from the player's
  `Gold001` and clears it. Faction assault alarms and jail sentencing use the
  witnessed-crime and jail-accounting paths described above.
- Perks, the crime faction and the outfits are seeded from `PRKR`, `CRIF`,
  `DOFT` and `SOFT`. `SetCrimeFaction` also joins the faction, per its page.
  `StartSneaking` toggles, per its page. `SetRestrained` and
  `SetUnconscious` clear each other. `Kill(akKiller)` records the killer for
  `GetKiller`, `StartCombat(target)` the target for `GetCombatTarget`.
- `GetCurrentLocation` is the reference's `XLCN`, else its cell's.
- `FindClosestActor`, `FindRandomActor`, `FindClosestReferenceOfType` and
  `FindRandomReferenceOfType` search enabled, non-deleted placed records within
  the supplied three-dimensional radius. Reference searches require the exact
  base form. Already instantiated references use runtime position and enable
  state. Closest ties resolve by FormID, and random selection uses the world's
  deterministic random stream. `GetFormEx2` resolves its signed Papyrus
  integer as an unsigned runtime FormID, matching `GetFormEx`. The local CK
  page documents the four search signatures and radius behavior. The local
  SKSE `Game.psc` documents `GetFormEx` high-ID handling. No local source
  documents `GetFormEx2`, so it follows the same lookup behavior.
  `GetEditorLocation` reads the same from the record and ignores `MoveTo`.
  Ownership is one `XOWN` slot on the reference or cell, `GetActorOwner` and
  `GetFactionOwner` answer by the owner's record type.
- `Actor.IsChild` reads the Child bit from the actor's current race `DATA`.
  `Actor.IsDetectedBy` reads the target's current observer-specific awareness
  from the shared detection state and returns true for Alert or Combat. The
  query takes the observer actor as its argument and does not generate an
  observation or advance detection. Invalid race or actor forms are logged by
  the Papyrus native and return false. `Actor.IsCommandedActor` reads whether a
  live command effect owns the actor. `Actor.IsOnMount` reads the actor's
  `riding_mount` state, and `Actor.IsTrespassing` reads its trespass state.
  Those values are supplied by the shared world and magic systems. The five
  native signatures and query meanings follow the local CK pages
  `IsChild_-_Actor`, `IsCommandedActor_-_Actor`, `IsOnMount_-_Actor`,
  `IsTrespassing_-_Actor` and `IsDetectedBy_-_Actor`. `Actor.Dismount` clears
  the actor's mounted flag and reports whether it was set. `Actor.Trespass` sets
  the same trespassing flag read by `IsTrespassing`. These methods do not create
  host mount placement or crime witness events.
- `Cell.SetActorOwner` accepts an `NPC_` base or `None`, and
  `Cell.SetFactionOwner` accepts a `FACT` or `None`. Other record types leave
  the existing owner unchanged. The SCR-56 registry-derived CK comparison
  and remaining native gaps are listed in [reference-audit](reference-audit.md).
- The listener registrations (`RegisterForSleep`, `RegisterForTrackedStatsEvent`,
  `RegisterForAnimationEvent`, `RegisterForLOS` and the single variants) keep
  lists of VM objects. `Game.IncrementStat` fires `OnTrackedStatsEvent`,
  `PlayAnimation` queues an event for the host graph. Host graph output and
  `Debug.SendAnimationEvent` fire `OnAnimationEvent` to listeners on that
  sender and name, `World::sleep_start`, `sleep_stop` and
  `line_of_sight` are the host's hooks for the other two.
- `ObjectReference.WaitForAnimationEvent` suspends its Papyrus thread until a
  matching graph event fires, then returns true. Graph unload or disabling the
  reference completes the wait with false. Waiting latent IDs and completed
  results are stored in the world `ANM-58` save section.
- `ObjectReference.InterruptCast`, `ForceAddRagdollToWorld`,
  `ForceRemoveRagdollFromWorld` and `TetherToHorse` use their owning magic,
  combat and host placement APIs as described in their sections below.
- `Message.Show` queues the `DNAM` record text according to the `MESG`
  message-box flag in `message_boxes` or `notifications`. It does not format
  numeric placeholders or wait for a selected button. It returns button 0 for
  queued message boxes and -1 for notifications. `ShowAsHelpMessage` queues a
  notification for each nonempty input-event name until reset. The method and
  global `ResetHelpMessage` calls clear that event's shown status.
  Help-message duration, repetition limits and input-event integration are
  not implemented. The CK contract comparison and owning backlog rows are in
  [reference-audit.md](reference-audit.md#scr-66-locationalias-and-message-contracts).
  A scene starts at
  phase zero. `World::complete_scene_action` marks an action slot complete,
  and `World::advance_scene_phase` waits for every action ending in the current
  phase before advancing. `Scene.IsActionComplete(actionID)` resolves the SCEN
  `INAM` identifier and reports whether that action's slot completed in the
  playing scene. Negative or unknown identifiers and actions outside a playing
  scene return false. Reaching the last phase stops the scene and delivers
  `OnEnd`. `Scene.Start` and `Stop` deliver `OnBegin` and `OnEnd` to the
  scene's scripts. SOEW version 36 stores the current phase and completed
  action slots. Version 37 stores that runtime in the `DLG-09` world section
  and appends named state sections. The `DLG-19` world and actor sections store
  started action slots, timer deadlines, actor package overrides and prior
  package assignments. Package actions assign the first listed package to
  their actor alias and restore its previous package at the action's end phase.
  Timer actions expire on real time and complete their action slot. The
  `SAV-15` reference section stores each reference's persistent flag. Action
  slots use zero-based positions from the SCEN action list because two vanilla
  scenes reuse action numbers. The SCEN layout follows xEdit's `wbRecord(SCEN)`,
  `TES5Edit/Core/wbDefinitionsTES5.pas` and `uesp-modfileformat/SCEN.wikitext`.
  The local CK corpus's scene index covers scene and timer fragments but lacks
  detailed behavior pages. Timer dispatch is implemented. CTDA evaluation,
  automatic action dispatch and scene flags are not implemented. `Sound.Play`
  hands out instance ids. `Spell.Cast` records the cast, no effect applies.
  `Game::start_scene_dialogue_action` dispatches one active Dialogue action by
  its zero-based position in the SCEN action list. It resolves the speaker alias
  through the scene's parent quest and starts the action's assigned DIAL through
  the dialogue selector, including INFO conditions and begin fragments. The
  host presents each returned line and calls
  `Game::advance_scene_dialogue_action` after playback. That method advances
  response lines, runs INFO completion behavior, then marks the SCEN action slot
  complete and closes the conversation when the line ends. A completed action
  whose end phase is later remains recorded until that phase can advance. The
  action must be in its authored phase interval, and the alias must resolve to
  an actor. The ordered action slot preserves distinct actions even though UESP
  records overlapping action numbers in two vanilla scenes. xEdit's
  `wbRecord(SCEN)` defines the action alias, DIAL, and phase interval. The local
  CK `GetCurrentScene` page documents participant lookup only, so it supplies no
  action scheduling behavior. Dialogue dispatch happens when called. Automatic
  action scheduling and CTDA evaluation are not implemented.
- `SetGameSetting*` writes an overlay `GetGameSetting*` reads first.

## FormList and Game native audit, SCR-60

The FormList ordering and Game control/chargen contract corrections are
assigned to SCR-60.

The registry inventory test
`scr_60_registry_inventory_matches_formlist_and_game_contracts` checks each
registered binding in the audited classes, including quiet Game stubs. The CK
class inventories are `FormList_Script.wikitext` and `Game_Script.wikitext`.
FormList methods were compared with their named function pages where available.
The Game class page provides the signatures and short contracts used below.

| Binding | CK contract comparison |
|---|---|
| `FormList.GetSize` | Returns the number of entries. |
| `FormList.GetAt` | Returns the zero-based entry, or None outside the list. |
| `FormList.HasForm` | Tests membership. None returns false. |
| `FormList.Find` | Returns the matching index, or -1 when absent. |
| `FormList.AddForm` | Ignores None, forbids duplicates, places temporary forms first and appends regular additions after authored entries. `scr_60_formlist_additions_follow_creation_order_and_skip_duplicates` checks ordering and duplicates. |
| `Game.GetPlayer` | Returns the player actor. |
| `Game.GetForm` | Resolves a runtime FormID under the active load order. |
| `Game.GetFormEx` | Compatibility alias for `GetForm`. CK's Game class page has no separate entry. |
| `Game.GetFormFromFile` | Resolves the low 24-bit object id in the named plugin. |
| `Game.EnablePlayerControls` | CK takes eight enable flags and a POV mode. True flags enable only those controls. |
| `Game.DisablePlayerControls` | CK takes eight disable flags and a POV mode. True flags disable only those controls. |
| `Game.GetGameSettingFloat` | Returns a float game setting. |
| `Game.GetGameSettingInt` | Returns an integer game setting. |
| `Game.GetGameSettingString` | Returns a string game setting. |
| `Game.SetGameSettingFloat` | Stores a float setting. |
| `Game.SetGameSettingInt` | Stores an integer setting. |
| `Game.SetGameSettingBool` | Stores a boolean setting. |
| `Game.SetGameSettingString` | Stores a string setting. |
| `Game.GetRealHoursPassed` | Returns elapsed real time in hours. |
| `Game.IncrementStat` | Adds the requested amount, defaulting to one. |
| `Game.QueryStat` | Returns the named statistic, defaulting to zero. |
| `Game.IsFastTravelEnabled` | Returns the fast travel setting. |
| `Game.EnableFastTravel` | Sets fast travel, defaulting to enabled when omitted. |
| `Game.SetPlayerAIDriven` | Sets the player's AI-driven flag. |
| `Game.SetInChargen` | Stores CK's three flags for disabling saves, disabling waiting and showing the disabled-controls HUD notice. It does not set the separate `in_chargen` state. |
| `Game.SetBeastForm` | Sets the player's beast-form flag. |
| `Game.TeachWord` | Marks the word as taught. |
| `Game.UnlockWord` | Marks the word as taught and unlocked. |
| `Game.IsWordUnlocked` | Returns whether the word is unlocked. |
| `Game.AddAchievement` | Records the requested achievement id. |
| `Game.GetPlayersLastRiddenHorse` | Returns the remembered horse, or None. |
| `Game.AddPerkPoints` | Adds points and clamps the total to 255. |
| `Game.GetDialogueTarget` | Returns the current player conversation's speaker, or None. This engine extension is not in the CK Game class page. |
| `Game.IsPlayerSungazing` | Quiet false stub for the CK observation query. |
| `Game.GetSunPositionX` | Quiet zero stub for the X coordinate. |
| `Game.GetSunPositionY` | Quiet zero stub for the Y coordinate. |
| `Game.GetSunPositionZ` | Quiet zero stub for the Z coordinate. |
| `Game.ShakeCamera` | Queues a source reference, strength and duration for the host camera. |
| `Game.SendWereWolfTransformation` | Reports werewolf crime through a loaded high-process actor with current Alert or Combat detection. |
| `Game.ServeTime` | Serves jail time against the player's crime faction when one is set. |
| `Game.ClearPrison` | Clears the player's arrested flag and cumulative served-day counter. |
| `Game.SetSunGazeImageSpaceModifier` | Sets the saved IMAD modifier triggered when sungazing begins. |
| `Game.ShakeController` | Queues left and right rumble strengths plus duration for the Godot host. |
| `Game.FadeOutGame` | Quiet stub. CK takes fade direction, black fade, delay and duration. |
| `Game.RequestSave` | Quiet stub for a normal save request. |
| `Game.RequestAutoSave` | Quiet stub for an autosave request. |
| `Game.ForceFirstPerson` | Quiet stub for forcing first-person camera mode. |
| `Game.ForceThirdPerson` | Quiet stub for forcing third-person camera mode. |
| `Game.SetHudCartMode` | Quiet stub for setting cart HUD mode. |
| `Game.ShowRaceMenu` | Quiet stub for opening the race menu. |
| `Game.TriggerScreenBlood` | Quiet stub. The CK Game class page has no entry. |
| `Game.PlayBink` | Quiet stub. CK takes a filename and four playback flags. |
| `Game.AdvanceSkill` | Quiet stub. CK takes a skill name and usage magnitude. |
| `Game.PrecacheCharGen` | Queues the host to retain available race head-part models and chargen TRI assets. |
| `Game.PrecacheCharGenClear` | Queues the host to release retained race head-part and chargen TRI assets. |
| `Game.ShowTrainingMenu` | Queues a valid trainer actor for the host training presenter. |
| `Game.ShowFirstPersonGeometry` | Sets whether the player's first-person geometry is shown in first-person view. |
| `Game.SetCameraTarget` | Sets a valid actor as the camera anchor target. |
| `Game.QuitToMainMenu` | Quiet stub for returning to the main menu. |
| `Game.SetSittingRotation` | Quiet stub. CK takes the rotation value. |
| `Game.StartTitleSequence` | Quiet stub. CK takes a sequence name. |
| `Game.GetCurrentCrosshairRef` | Quiet stub. The CK Game class page has no entry. |
| `Game.GetPlayerGrabbedRef` | Quiet stub for the currently grabbed reference. |
| `Game.FindClosestActor` | Quiet stub. CK takes a position and radius. |
| `Game.FindRandomActor` | Quiet stub. CK takes a position and radius. |
| `Game.FindClosestReferenceOfType` | Quiet stub. CK takes a base form, position and radius. |
| `Game.FindRandomReferenceOfType` | Quiet stub. CK takes a base form, position and radius. |
| `Game.GetFormEx2` | Quiet stub. The CK Game class page has no entry. |

`scr_60_game_control_and_chargen_flags_follow_ck_and_save` checks selective
control changes, POV type, all three chargen flags and save/load through the
SCR-60 world section. `SetInChargen` maps its three arguments to those flags,
and leaves the separate `in_chargen` state unchanged. `player_controls` remains the aggregate all-enabled
state used by existing condition evaluation. Host input does not yet consume
each individual control flag. The `GetFormEx` alias and three Game bindings
absent from the CK class page are also recorded here as engine or extension
surface, not CK contracts. The quiet stub fixes belong to these existing native rows:

- SCR-26: `IsPlayerSungazing`, `GetSunPositionX`, `GetSunPositionY`,
  `GetSunPositionZ`, `ShakeCamera`.
- SCR-27: `SendWereWolfTransformation`, `ServeTime`, `ClearPrison`,
  `SetSunGazeImageSpaceModifier`, `ShakeController`.
- SCR-28: `FadeOutGame`, `RequestSave`, `RequestAutoSave`, `ForceFirstPerson`,
  `ForceThirdPerson`.
- SCR-29: `SetHudCartMode`, `ShowRaceMenu`, `TriggerScreenBlood`, `PlayBink`,
  `AdvanceSkill`.
- SCR-30: `PrecacheCharGen`, `PrecacheCharGenClear`, `ShowTrainingMenu`,
  `ShowFirstPersonGeometry`, `SetCameraTarget`.
- SCR-31: `QuitToMainMenu`, `SetSittingRotation`, `StartTitleSequence`,
  `GetCurrentCrosshairRef`, `GetPlayerGrabbedRef`.
- SCR-32: `FindClosestActor`, `FindRandomActor`,
  `FindClosestReferenceOfType`, `FindRandomReferenceOfType`, `GetFormEx2`.
- A quest start delivers `OnInit` to its alias objects too, the ones a
  script is attached to, since the alias kind (`ALST` or `ALLS`) now decides
  the class (`ReferenceAlias` or `LocationAlias`) of the VM object.
  Transitional QUST start and stop flags restore pending promotion tasks.
  `IsStarting` and `IsStopping` report the task direction, and `IsRunning` stays
  false until the task completes. SOEW version 37 retains those tasks in the
  `SAV-10` world section.
- `Game.FadeOutGame` validates finite, non-negative delay and duration values,
  then queues the flags and timings for the host overlay. `RequestSave` and
  `RequestAutoSave` queue distinct host save requests. `ForceFirstPerson` and
  `ForceThirdPerson` queue explicit player camera modes. The host drains these
  transient requests after a gameplay tick. They are not saved with `SOEW`.

### Scene and scroll native contracts, SCR-69

The registry-derived inventory contains seven methods and no globals. The local
vanilla `skse64/scripts/vanilla/Scene.psc` and `Scroll.psc` files declare the
signatures and summaries below. The matching CK named function pages are absent
from the local corpus. CK's `GetOwningQuest - Alias` page confirms that Alias
returns its owning quest, but it does not document Scene. The CK
`GetCurrentScene - ObjectReference` page documents a different binding.

| Binding | PSC contract | Current implementation and finding |
|---|---|---|
| `Scene.Start` | Start this scene. | Starts a valid inactive SCEN and sends `OnBegin`. |
| `Scene.ForceStart` | Start this scene and stop scenes running on any reference in it. | Resolves participant aliases, ends each active scene sharing a resolved reference, then starts this scene and sends `OnBegin`. |
| `Scene.Stop` | Stop this scene. | Stops a running scene and sends `OnEnd`. |
| `Scene.IsPlaying` | Return whether this scene is playing. | Reads the active scene run state. |
| `Scene.GetOwningQuest` | Return the quest that owns this scene. | Resolves the SCEN record's parent quest. |
| `Scene.IsActionComplete` | Return whether the specified action ID is complete. | Resolves the authored SCEN action ID to its action slot and checks that slot's completed state. Negative or unknown IDs and stopped scenes return false. |
| `Scroll.Cast` | Cast this scroll from a source reference, with an optional target. | Executes the owning SPEL scroll through the shared cast executor and preserves source, caster, and target attribution. The script contract does not specify inventory consumption. The selected-instance consume path belongs to MAG-18. |

`Scroll.Cast` accepts source and optional target references as declared. The
separate inventory selection API consumes one selected scroll after successful
effects and preserves it when a cast is rejected, as covered by MAG-18. The
Papyrus method does not select an inventory stack itself.

`Scene.Start` starts a valid, inactive scene and sends `OnBegin`. Its record
must have phase bounds the current runtime accepts. `Scene.Stop` ends a running
scene and sends `OnEnd`. `Scene.IsPlaying` reads the running-scene set.
`Scene.GetOwningQuest` resolves the owning quest from the scene record's parent
quest field. These behaviors match the local PSC summaries.

`Scene.IsActionComplete(actionID)` takes the authored SCEN action ID from `INAM`. The runtime maps that ID to its action-list slot, then reads the completed slot state that gates phase advancement. This keeps sparse IDs separate from their internal positions. Negative, unknown and removed action IDs return false. Completion in another scene with the same action ID does not affect the result. The local CK corpus has no named page for this method. The vanilla `Scene.psc` declares an integer action ID, while UESP identifies `INAM` as the action number and notes that IDs can be non-sequential and overlap between scene records. The `scr_44_` tests exercise sparse and invalid IDs through Papyrus dispatch, and a `SKYRIM_DATA`-gated check verifies overlapping IDs stay scene-local.

The comparison tests derive the methods and globals directly from the registry.
Papyrus dispatch behavior is covered by the `scr_69_` tests and the focused
Scene and Scroll tests in `tests/natives_2.rs` and `tests/magic.rs`. No installed
game records are named by this contract check.
`Scene.ForceStart` is currently the same path as `Start`. Vanilla PSC says it
must also end scenes running on references in the forced scene. Actor action
ownership is not yet dispatched, so DLG-18 and DLG-19 own the scene action
dispatch needed before that conflict behavior can be implemented.
`Scroll.Cast` remains a quiet stub. Its PSC contract takes a source and an
optional target and performs a scroll cast. MAG-18 owns scroll casting and
selected inventory instance consumption. The `scr_69_` tests call each listed
binding through Papyrus dispatch and verify `IsActionComplete` before and after
the action slot is marked complete.

## Tests

`Cargo.toml` disables automatic integration-test discovery and registers one
binary, `it`, rooted at `tests/it.rs`. Each top-level test file stays in place
and is registered there with `#[path]`. Shared installed-data, legacy-save
and magic fixtures are declared once under `crate::common` and
`crate::support`. Test files import these helpers through `crate::` paths.
Existing nested scenario reruns retain their test names below the new
top-level module prefix. Determinism child processes use that prefix when
selecting their exact test, as do feature-gated persistence child processes.
Rerun fixtures include their module path in temporary-directory names so
concurrent modules do not overwrite each other's inputs.

`tst_16_every_test_file_is_registered` compares the top-level Rust files with
the module declarations and reports the declaration to add for a missing
file. The registration file uses Git's union merge driver. New test files
need their module declaration in `tests/it.rs`.

`tst_16_test_names_match_the_pre_consolidation_inventory` compares committed
before and after test-name inventories from the consolidation run. It removes
the `unit::` prefix and TST-16's added guard tests before comparing the 2,289
remaining names. This checks historical name preservation, but does not compare
installed test results.

Installed integration tests use `tests/common/installed.rs`. Each test
process caches parsed plugin files and load orders by their installed master
set, then gives each `Game` its own mutable world over the shared immutable
load order. Parsed plugins use `read_plugin_cached` under the Cargo target
directory, or the directory named by `SOE_PLUGIN_CACHE`. Separate test
binaries can reuse those entries. `tst_14` checks in `combat_hits.rs` and
`dialogue.rs` verify in-process cache reuse and world isolation.

The `leveled-lists` follow-up adds `World::roll_leveled_items` for ordinary
LVLI quantities. Callers supply the selection level and requested count.
Highest eligible level, all eligible levels, per-count rolls, Use All,
duplicate entry weighting, nested counts and live LVLG chance globals feed
the saved world RNG. Use All selects every entry unless calculate-each is set.
Calculate-each takes precedence, so flags 6 and 7 select once per requested
count using highest-eligible or all-eligible levels respectively. Flag 5 uses
every entry regardless of level. Failed validation or
overflow preserves RNG state and does not change an inventory or queue events.
The graph is checked before selection, including unselected branches.
Expansion is bounded to 128 nested lists, 4,096 distinct lists and 65,536
list evaluations per call. Exceeding those bounds returns an error.

Sources are local xEdit LVLI/LVLO/LVLG definitions, UESP LVLI flag descriptions,
CommonLib TESLeveledList flags and the existing actor-template resolver.
CommonLib's byte-returning GetChanceNone truncates fractional LVLG values.
skymp LeveledListUtils supplied a recursion comparison. Its highest-level
omission, global chance fallback and loss of nested entry counts were not
copied. Existing list state, live globals and save/RNG paths are reused.
APPA is accepted alongside the other inventory base signatures named by xEdit.
No package or host node was added.

The 19 leveled-item checks include a five-master survey of 3,835 LVLI records
at levels 1, 20 and 50. It resolves 10,878 cases and reports 627 cases with
unsupported flags. Special-loot selection is tracked by PRG-14 and COED
ownership/condition propagation by PRG-15. This quantity API rejects those cases
before rolling. PRG-16 adds six mixed-flag checks, replacing the prior rejection
check. The survey encountered no COED rejection. AddItem dispatch

unsupported flags. Special-loot selection is tracked by PRG-14. COED
ownership and condition support is described below. Mixed Use All flags require
the precedence evidence tracked by PRG-16. Unsupported flags are rejected
before rolling. The original survey encountered no COED rejection. AddItem dispatch
and outfit initialization consume these results in their own backlog items.
The fixtures also cover owner-relative override links, script list edits,
save/load reproducibility, cycle rejection, overflow and the expansion bound.

Mixed-flag precedence comes from read-only inspection of the installed
SkyrimSE 1.7.104.0 executable, SHA256
`846efccf0c1374d71f892907f46549560f2fcb0a75cb87a3eed438baa0f1402f`.
`CalculateCurrentFormList` at `0x1401e90f0` checks calculate-each at
`0x1401e91f1` before checking Use All at `0x1401e92d4`. The per-count branch
returns without visiting Use All, including for nested lists. Tests cover
flags 5, 6 and 7, eligibility, chance-none rolls and nested count multiplication.
No executable bytes or proprietary code were copied.
No comparison of the RNG sequence against Bethesda's runtime was performed.

The leveled-item checks include owner-relative override links, script list
edits, save/load reproducibility, cycle rejection, overflow and the expansion
bound. COED ownership/condition propagation is tracked by PRG-15. Mixed Use
All flags require the precedence implementation tracked by PRG-16. This API
rejects those cases before rolling. AddItem dispatch and outfit initialization
consume these results in their own backlog items. No comparison of the RNG
sequence against Bethesda's runtime was performed.

### Special Loot selection

PRG-14 extends the same quantity API to LVLF bit 3. The caller supplies the
input level, and the formula independently reads `World::player_level`.
The nine live GMST values come through `World::game_setting`, including its
script overlay and winning record lookup. Executable defaults supply settings
absent from plugins. No duplicate player, inventory or record state is added.

The calculation uses single-precision operations in this order:

```text
minimum = player * MinPCLevelMult + MinPCLevelBase
        + MinZoneLevelBase + input * MinZoneLevelMult
maximum = player * MaxPCLevelMult + MaxPCLevelBase
        + MaxZoneLevelBase + input * MaxZoneLevelMult
unit = float(unsigned_random_32) * 2^-32
used = truncate_to_signed_32(minimum + (maximum - minimum) * powf(unit, Weighting))
```

Every abbreviated setting has the `fSpecialLoot` prefix. The defaults are:

| Setting suffix | Default |
|---|---|
| MinPCLevelBase, MinZoneLevelBase, MaxPCLevelBase, MaxZoneLevelBase | 0 |
| MinPCLevelMult, MinZoneLevelMult | 0.5 |
| MaxPCLevelMult | 1 |
| MaxZoneLevelMult | 1.5 |
| Weighting | 4 |

The unsigned draw becomes a float before scaling, so float rounding can
produce exactly 1. Fractional weighting never receives a negative base.
The result truncates toward zero and is compared as a signed level, without
a lower clamp. An out-of-range or nonfinite result has the runtime's INT_MIN
conversion result and selects nothing. Nonfinite or wrongly typed settings
are rejected during graph preparation, before RNG changes.

Chance-none runs before the formula. Calculate for each item rerolls the
formula for every requested count. Without it, one selection is multiplied.
Calculate from all levels caps the formula's input at the list's highest
entry level. Nested lists receive the original caller input, and apply their
own flags and formula. Use All bypasses the formula. Mixed Use All flags stay
with PRG-16 and COED propagation with PRG-15.

Sources first checked were local xEdit `wbDefinitionsTES5.pas` LVLI,
UESP `LVLI.wikitext`, CommonLibSSE-NG `TESLeveledList.h/.cpp`, skymp and the
existing item resolver. They describe the fields or call the original engine,
without implementing this formula. The public
[LeveledItem page](https://ck.uesp.net/wiki/LeveledItem) and
[DavidJCobb's July 2024 disassembly notes](https://ck.uesp.net/wiki/Talk:LeveledItem)
identify the nine settings and separate player and context-dependent input
levels. Their signed random range differs from the installed Special Edition
executable. Its directly inspected instructions supply the behavior above.

The inspected file is the installed `Skyrim Special Edition/SkyrimSE.exe`,
with fixed file/product version `1.7.104.0` and SHA-256
`846efccf0c1374d71f892907f46549560f2fcb0a75cb87a3eed438baa0f1402f`.
Addresses below are virtual addresses at image base `0x140000000`:

- `0x1401e8d20` is the single-choice resolver. Chance-none precedes the flag
  check at `0x1401e8d95`. The formula occupies `0x1401e8dc4` through
  `0x1401e8ea0`, ending in `CVTTSS2SI`. Signed entry comparison is at
  `0x1401e8f43`.
- The scaling constant at `0x1417f7cf0` is exactly `2^-32`. The call at
  `0x1401e8e7d` resolves through the import table to `powf`.
- The nine labeled default GMST objects hold values at `0x142078f48` through
  `0x142079008`, spaced by 24 bytes.
- `0x1401e90f0` is the count/list resolver. All-level input capping is at
  `0x1401e914c` through `0x1401e919b`. Per-count calls reach the single-choice
  resolver at `0x1401e9243`. Use All branches before that calculation at
  `0x1401e92d4`. Nested calls at `0x1401e9289`, `0x1401e9397` and
  `0x1401e944e` retain the original input.

Validation adds ten public-API fixtures and one numerical unit test to the
existing list checks. All 29 integration tests pass with installed data.
The five-master survey resolves all 11,505 cases across 3,835 lists at levels
1, 20 and 50, including 324 cases rooted at 108 Special Loot lists. No case
reached the pending COED or mixed-flag boundary. Fixed-setting fixtures cover
all eight bound terms, fractional weighting, signed conversion, nested input,
per-count calculation, live overlays, save reproducibility and RNG rollback.

Only behavioral notes are included here, with no executable bytes. The
engine continues using its saved xorshift stream, taking a 32-bit draw for
this formula. It does not claim Bethesda's exact random sequence or identical
platform `powf` rounding. Encounter-zone input selection belongs to callers,
including future inventory initialization. No new package or host node is
added. Static instruction inspection establishes these rules, without a
running original-game comparison.

- Unit tests per module: VMAD round trip, object format 1, version 3, cuts at
  every offset and a hostile array count. PACK begin, end and change
  fragments decoded and re-encoded from hand-authored bytes, one flag at a
  time and skipping absent entries, plus unsupported flags and truncation.
  `tests/packages.rs` round-trips every winning PACK record's `VMAD` in
  Skyrim.esm, Update.esm and the three DLC masters against SKYRIM_DATA,
  covering object format 1 and 2 and versions 4 and 5 among installed
  records. QUST view. Clock. Scenario grammar and the log diff. The native
  registry count against the hand-kept constant.
- `tests/fmt_18_scene_vmad.rs` checks SCEN begin, end and phase fragments
  against independently authored VMAD bytes, all begin/end flag combinations,
  truncation, unknown flags and trailing data. With `SKYRIM_DATA`, it checks
  every winning SCEN VMAD in Skyrim.esm, Update.esm and the three DLC masters
  and requires each fragment kind to occur. The decoder retains phase order,
  phase flags, 32-bit indices and marker bytes. xEdit's
  `wbVMADFragmentedSCEN` defines the fields and FallrimTools' `FragmentScen`
  confirms the counted phase list. Champollion describes PEX functions but
  does not define this record tail, so its fragment names did not supply a
  reusable VMAD reader. Scene execution remains in DLG-09.
- `tests/fmt_17_perk_vmad.rs` checks PERK fragment entry-point identity,
  authored-byte round trips, truncation and trailing data. xEdit's
  `wbVMADFragmentedPERK` defines the signed extra-bind version, host file,
  counted fragments, 32-bit indices and marker byte. Champollion describes
  PEX functions but does not define the PERK record tail.
- `tests/quest_flow.rs`: a synthetic master with a start-game-enabled quest,
  a lever whose script sets stage 10, a fragment on stage 10 that completes the
  objective, gives the player gold and sets stage 20 which completes the quest,
  a door pair with `XTEL`, a container seeded by `CNTO`, and a script that
  registers a two second update. Checks binding counts, stage and objective
  state, the notification, the disabled lever, the teleport, the container,
  the timer firing on the clock and not before, scriptless forms answering
  natives, a second provider losing a collision, and the scenario harness
  running the same flow against a captured log.
- `tests/real_files.rs`: `the_installed_game_binds_its_vanilla_scripts`, gated
  on `SKYRIM_DATA`, calls `Game::load_scripts_from_data` before `bind` so the
  `BindReport` reflects the vanilla scripts read out of the install's archives
  and loose files, not the record data alone. See "Live run" below. It also
  prints the unresolved calls of `start` grouped by `Class.Function`, the
  list the next natives round starts from.
- `tests/natives_2.rs`: one scenario per native group of the natives round,
  each driven through a probe script assembled per call (`CallMethod` on the
  target, the same path a vanilla script takes) against a synthetic master
  with leveled lists, a location tree, two factions, an actor base with
  perks, a message pair, a scene, a sound and a quest with a location alias
  and a scripted reference alias. The doc comment of each test quotes the
  wiki page behaviour it asserts. The last test saves and loads the new
  state.

```bash
cargo test -p soe-game
```

## Live run (2026-09-14)

The old NPC attribute measurements are superseded by the 2026-09-20 survey.
They used DNAM fields read two bytes late and omitted race starting values.
With the corrected layout, the five-master inputs resolve Player to health,
magicka and stamina 100, Hulda to 75/60/60, and MQ101Alduin to 2355/50/80.
These are engine results from authored data, not comparison with a running
vanilla game. `Prisoner` is a `CLAS` record, not an actor base.

A gated test, `the_installed_game_binds_its_vanilla_scripts` in
`tests/real_files.rs`, loads the five base masters through `soe_esm`, builds a
`Game`, loads every `.pex` under `Data/Scripts` when that folder exists, then calls
`bind` and prints the `BindReport`. This install ships its scripts packed inside
`Skyrim - Misc.bsa` with no loose `Data/Scripts` folder, so the test found none to
load and bound against the record data alone. 23443 forms carry a `VMAD`, 0 scripts
attached, 28152 scripts missing since nothing was loaded, 0 properties set, 0
properties missing, 0 quests started. The first 20 missing scripts are magic effect
and quest scripts such as `DragonPriestUltraMaskEffect`, `magicAbsorbFXScript` and
`DA08EbonyBladeMagicEffectScript`. This run exercises the VMAD scan and form
resolution across the full load order but not the script attachment path itself.

## Live run (2026-09-17): scripts from the archive

`Game::load_scripts_from_data` closed the gap above. For every plugin in the
load order it looks for `<stem> - Misc.bsa` and `<stem>.bsa` in `data_dir`
(case-insensitively), adds every one it finds to a `soe_bsa::Vfs` in load
order, adds `data_dir` itself as a loose root on top, then loads every `.pex`
the view answers under `scripts/`. Loose files win over archived ones and a
later archive wins over an earlier one, the same rule `Vfs` always applies, so
a loose `Data/Scripts` folder (a mod's override, or a manual extraction) still
takes priority over the packed vanilla copy without the caller doing anything
special.

`tests/real_files.rs` now calls it before `bind`. Against the installed game
(`SKYRIM_DATA` pointing at Steam's `Skyrim Special Edition/Data`) it finds one
archive, `Skyrim - Misc.bsa` for `Skyrim.esm`: this install is the Anniversary
Edition patch, which folded every DLC's assets into the base archives, so
`Update.esm`, `Dawnguard.esm`, `HearthFires.esm` and `Dragonborn.esm` have no
`.bsa` of their own and none is expected. Loading all 14026 scripts the archive
holds takes 0.3 seconds. `bind` then attaches 28143 of the 28152 scripts the
load order's `VMAD` data names in 0.2 seconds, tens of thousands of instances
and nowhere near slow enough to worry about. 71318 properties are set, 97
missing, 0 quests started (this test calls `bind`, not `start`, so no Start
Game Enabled quest runs). The first run of this test reported 349 missing
properties, before `set_property` learned to run a compiled setter (below).

The 9 remaining missing scripts, `DLC1testPhilVortexSCRIPT`,
`dlc1testPhilVortexTrigSCRIPT`, `DLC1TestPhilAtronach`,
`DLC01SoulCairnSkullPuzzleScript` (twice), `DragurEmergeScript`,
`LokirsDraugrResurrection` (twice) and `DLC2BenthicLurkerFXSCRIPT`, are not in
`Skyrim - Misc.bsa` under any spelling checked (a probe against the archive's
own file list found none of them, nor a close respelling such as
`DraugrEmergeScript`). This is a vanilla data quirk, not an engine bug: these
forms still carry `VMAD` entries for developer test scripts and a renamed
quest script (the shipped file is `MQ101Lokir.pex`) that Bethesda never
cleaned out of the masters, and the real game hits the same lookup failure at
runtime.

The first run reported 349 missing properties, 252 of them one pattern:
`TrapTriggerBase.psc` (the parent `TrapLever`, `SoundTrap` and
`TrapTriggerTimeInterval` extend) declares `TriggerType` and `TriggerCount`
as full properties, a hand-written `Get` and `Set` pair backed by a separate
`StoredTriggerType` auto var. The Creation Kit lets a form's `VMAD` carry a
value for a full property and the real engine applies it by calling the
compiled `Set` function. `Vm::set_property` now does the same: when the
property has no auto var it runs the setter's bytecode on a fresh thread with
the value as its argument and drives it to completion before returning (the
setter may not wait or call a latent native, that case is logged). The 97
that remain name properties the loaded script does not declare at all, such
as `TG06QuestScript.TG05KarliahInvisibilitySpell`,
`DLC1CrystalDrainHealthCheckScript.vampLordRace` and the
`DLC1SeranaLevelingScript` perk properties: `VMAD` entries left behind after
the script source dropped the property, which the real engine cannot fill
either. They are vanilla data, not an engine gap.

## Live run (2026-09-18): the first start

The gated test now calls `start` after `bind`. Before the identifier-operand
fix in the VM (see `docs/papyrus.md`, name operands) no native reached the
game crate. After it, on the installed game in release: 423 quests started,
63 distinct natives reached through 1,095 calls, 300 calls to functions the
engine does not implement, 432 log lines, in well under a second.

The twenty most called natives during start, by `class.function`:

| Calls | Native |
|---|---|
| 93 | GlobalVariable.GetValue |
| 86 | Game.GetPlayer |
| 77 | Debug.OpenUserLog |
| 77 | Game.GetForm |
| 60 | Quest.IsStageDone |
| 56 | ObjectReference.GetLinkedRef |
| 42 | Quest.SetObjectiveDisplayed |
| 36 | ObjectReference.IsDisabled |
| 34 | Form.HasKeyword |
| 34 | FormList.AddForm |
| 28 | Actor.AddToFaction |
| 28 | Actor.GetFactionRank |
| 28 | ObjectReference.GetCurrentLocation |
| 26 | Actor.RemoveSpell |
| 22 | ObjectReference.AddItem |
| 22 | ObjectReference.Disable |
| 22 | Quest.IsRunning |
| 19 | Alias.GetReference |
| 19 | ObjectReference.Enable |
| 18 | Alias.GetActorRef |

The 300 unresolved calls name the next natives to write and one dispatch
gap: `LeveledItem.AddForm` and the other leveled list functions do not exist
yet, and `Alias.RegisterForUpdate` fails although `Form.RegisterForUpdate` is
registered, because the loaded `Alias.pex` declares the native on `Alias` and
`call_method` stops at the declaring script instead of walking up to the
registry's `Form` entry. Both are queued for the natives round.

## Packages

`crates/game/src/package.rs` is the `PACK` view, `crates/game/src/ai.rs`
the evaluation and the actor driver. Layout from `wbRecord(PACK` and
`wbConditions` in `wbDefinitionsTES5.pas` and the UESP `PACK` page, the
stack semantics from the Creation Kit wiki (`EvaluatePackage - Actor`).

**The view.** A package is a template plus data. The template (`PKCU`
names it) carries the procedure tree, `ANAM` branches with `PNAM`
procedure names and `PKC2` input indexes, and the data input names
(`UNAM`, `BNAM`). A package built on it carries values for those inputs in
its public data: `ANAM` type, then `CNAM` (bool, int, float), `PLDT`
(location kind, form, radius), `PTDA` (target), `PDTO` or `TPIC` (topic),
paired with the trailing `UNAM` list in order. `PKDT` general flags and
preferred speed, `PSDT` schedule, header `CTDA` conditions with `CIS1` and
`CIS2` ignored, `QNAM` owner quest, `CNAM` combat style, `IDLA` idles and
the three `POBA`, `POEA`, `POCA` actions are read. The same tag means
different things per section, so the reader is a state machine like the
`QUST` one. `Quest::Alias` now reads `ALPC`, the alias packages.

**The stack.** `ai::candidates` lists the alias packages of every running
quest that holds the actor in an alias, higher quest priority first, then
the `PKID` list of the actor's base (through one level of `TPLT` when the
template flags say so), with SPID package entries inserted at their configured
zero-based positions in that base list. `ai::select` walks it and the first package whose
schedule and conditions hold wins. `Once Per Day` skips a package that ran
to completion on the current game day. `ai::evaluate` switches the actor
when the winner changed and queues `OnPackageEnd`, `OnPackageChange` and
`OnPackageStart` for the actor's scripts. A new winner calls its PACK
`OnBegin` fragment with `akActor`. A replaced package calls its `OnChange`
fragment, while a package with no successor calls its `OnEnd` fragment.
These named fragments run once at the package boundary on the package's VM
object. `Actor.EvaluatePackage` calls it at
once, `Actor.GetCurrentPackage` returns the running package, and the
driver re-evaluates every actor once a real second and whenever it has no
package. The natives table rows above still list both natives as stubbed,
they moved to the implemented column with this section.

The `POBA`, `POCA` and `POEA` actions also start their authored idle or
dialogue topic at the matching boundary. Idle delivery uses the actor's
animation event path. A topic opens a dialogue session with the player as
listener and makes that topic the session's initial choice. Dead and
unconscious actors do not start boundary actions. Topic actions also require
an available actor who can speak to the player and no active dialogue session.

An unfinished package with `Must Complete` stays selected ahead of higher
priority candidates until its procedure tree finishes. Combat can interrupt
it when `Interrupt Override` is Combat or `Observe combat behavior` is set in
the PKDT interrupt flags. `Ignore Combat` leaves package movement in control
instead of acquiring or pursuing a combat target. `No Combat Alert` records
combat engagement without setting the actor's alerted state. `At Package Start
Unlock Doors` unlocks locked doors owned by the actor in its current cell when
that package starts. `On Package Change Unlock Doors` does the same when leaving
the flagged package. Unlocking uses the shared reference lock state and emits
the ordinary lock state event.

Built actors retain their distributed package entries and insertion positions
in SOEW version 37, so package selection uses the same stack after load.

The `POBA`, `POCA` and `POEA` actions also start their authored idle or
dialogue topic at the matching boundary. Idle delivery uses the actor's
animation event path. A topic opens a dialogue session with the player as
listener and makes that topic the session's initial choice. Dead and
unconscious actors do not start boundary actions. Topic actions also require
an available actor who can speak to the player and no active dialogue session.

**Schedules.** `hour` `-1` is any time. A window starts at `hour:minute`
and lasts `duration` minutes, wrapping past midnight, a zero duration
lasts until midnight. Day of week 0 is Sundas, the day the game starts on,
7 to 10 are the Creation Kit's weekday groups. Live date, month and hour
checks read the calendar globals. Standalone test clocks use the default
17 Last Seed 201 start date and CommonLib's month lengths.

**Conditions.** `CTDA` is 32 bytes: comparison in the high three bits of
the type byte and the OR, use aliases, use global, use pack data and swap
flags in the low five, a float or a `GLOB` to compare against, the
function index from xEdit's `wbConditionFunctions`, two parameters, the
run-on target, a reference and a third parameter. Conditions combine with
AND, an OR flag joins a condition with the next into a group that passes
when any member does. Run on subject, reference and quest alias resolve,
the other targets fail. The functions with an implementation: GetDistance,
GetActorValue (by the actor value index), GetCurrentTime, GetInSameCell,
GetDead, GetItemCount, GetSleeping, GetQuestRunning, GetStage,
GetStageDone, GetInCell, GetIsRace, GetIsSex, GetInFaction, GetIsID,
GetFactionRank, GetGlobalValue, GetRandomPercent, GetLevel,
GetPlayerControlsDisabled, GetIsReference, GetIsCurrentPackage,
GetDayOfWeek, IsSneaking (always false, no sneak state yet), IsInCombat,
IsInInterior, GetQuestCompleted, HasKeyword, GetIsAliasRef,
GetActorValuePercent, IsBleedingOut, GetNoBleedoutRecovery and IsStaggered.
Bleedout reads the actor's unconscious and dead flags, the recovery query
reads `no_bleedout_recovery`, and stagger reads the active real-time deadline.
IsAttacking, IsPowerAttacking, IsLastHostileActor, ShouldAttackKill and
IsAttackType return a typed condition error: local xEdit metadata gives their
identities and parameter types, but the CK corpus has no usable condition
contract and the runtime lacks their live attack or hostile-state producers.
GetCombatTargetHasKeyword and IsIgnoringCombat. The
combat-target keyword condition resolves the subject actor's current combat
target and checks that target's winning base keywords. It is false when the
subject has no combat target. IsIgnoringCombat reads the active package's
`Ignore Combat` flag. Both conditions reject a non-actor subject, and the
keyword condition also rejects a missing combat target form or invalid
keyword. IsRecoiling, GetShouldHelp, GetCombatGroupMemberCount and
IsHostileToActor return per-function diagnostics because the runtime does not
model recoil animation state, actor-to-actor assistance, combat group
membership or the faction and crime rules that determine hostility.

The motion condition increment also adds GetHeadingAngle, GetWalkSpeed,
GetSitting, IsCurrentFurnitureRef, IsCurrentFurnitureObj and IsPCSleeping. It
reads reference transforms, race movement records and current furniture marker
reservations. IsIdlePlaying and GetWindSpeed return explicit unsupported
function errors because this runtime has no idle playback or wind state.
AI-37 adds EPModSkillUsage_AdvanceObjectHasKeyword and
EPModSkillUsage_IsAdvanceAction when the caller supplies a matching skill-use
event to the PERK entry-point context, GetLightLevel from a caller-supplied
sample in the CK range 0 to 150, and IsOverEncumbered when InventoryWeight is
greater than CarryWeight. GetActorWarmth returns an explicit unavailable
contract error because xEdit names it but the local CK corpus defines no usable
semantics. Missing event or light context is an error.

**AI-71 reference conditions.** `GetIsAlerted` reads the actor's runtime
alert flag, `IsWaiting` checks whether its active package procedure is `Wait`,
and `IsGuard` checks membership in the Skyrim `GFAC` default object faction.
All three require an actor context and no parameters. xEdit supplies the
function indices and the two integer parameters on `GetPlayerControlsDisabled`.
The local CK pages define `Actor.IsAlerted` and `Actor.IsGuard` as boolean actor
queries, while UESP's raw function table describes the eight optional player
control groups and links `HasBeenEaten` to `StartCannibal`. The other five
functions return `Unavailable`: the runtime has no per-group player control
state, facing-up or knocked animation state, native package identity result,
or cannibalism completion state. The CK corpus has no usable contract pages
for those conditional functions. Tests `ai_71_` cover true and false actor
fixtures, parameter and actor-context errors, and diagnostics for untracked
queries. The fixtures use synthetic records and do not need installed game
data.

AI-52 implements condition 309, `IsXBox`, as false for this PC runtime and
rejects either nonzero parameter. Conditions 224 `GetVATSMode`, 235
`GetVatsTargetHeight`, 381 `GetHasNote`, 407 `GetVATSValue`, 427
`GetPlantedExplosive`, 473 `GetIsAlignment` and 515 `GetVATSRightAreaFree`
remain unsupported and return explicit condition errors. The AI package
evaluator counts those functions as unsupported. xEdit's
`wbDefinitionsTES5.pas` supplies their IDs and parameter types. No individual
CK condition pages for these functions exist in the local CK corpus, so the
engine does not infer their runtime values.
AI-72 adds these queries to both condition evaluators: `GetOpenState` for
placed `DOOR` references (0 for other references, 1 open, 2 opening, 3 closed
and 4 closing),
`GetIsCurrentPackage` against the subject actor's selected package,
`GetDestroyed` from reference state, and `GetDefaultOpen` from a placed
reference's `ONAM` marker. `ONAM` also initializes the runtime open state.
`World::begin_door_opening` and `World::begin_door_closing` record transitional
states and their settled destination, while `World::finish_door_transition`
clears the transition. The marker is transient, so loading a save restores the
settled destination. Papyrus `GetOpenState` and CTDA 157 share this mapping.
HST-21 routes Papyrus `SetOpen` through `World::set_reference_open`. It sets
the existing open destination and clears a transition marker. Opening a door
clears its lock and lock level, following local CK
`SetOpen_-_ObjectReference.wikitext`. Closing does not lock it. Missing and
deleted references return false through the World API and remain unchanged.
The `hst_21_` native fixture checks repeated opens and closes, the default
open argument and the lock contract. No saved fields or section table entries are added.

`GetCurrentAIProcedure`, `IsContinuingPackagePCNear`, `GetVampireFeed` and
`GetCannibal` remain unsupported with a function diagnostic because the local
sources name them without defining query results.
AI-66 adds `GetWithinDistance` (639), using the run-on subject's position,
the parameter reference's position and the parameter float as an inclusive
distance threshold. xEdit's `wbConditionFunctions` and UESP's
`Raw_Function_Data.wikitext` define its reference and float parameters.
Furniture entry state (614) and path target angle and speed queries (620 to
622) remain explicit unsupported-function diagnostics. Path target offset
(619) and movement speed (623) validate their actor context and xEdit
parameters, then return a contract diagnostic because the local CK corpus
defines no usable condition semantics. The CK
`GetFlyingState` page defines states 0 to 4. Function 633 reads an optional
live actor flying state, rejects values outside that range, and reports
unavailable when movement has not supplied a state. That live state is not
saved and defaults to unavailable after load.

**Known-form conditions, AI-76.** `PlayerKnows` (493) reads the player's
known-form state for `SPEL`, `ENCH`, `MGEF` and `WOOP` records. Its knowable
record types follow xEdit's `ptKnowable`. A `SHOU` record is not a word of
power and returns a typed state-contract error. Spell knowledge remains in the
player spell set, while other supported form types use the saved known-form
set.

The MIT CommonLib shim exposes these fields through `CONDITION_ITEM_DATA` and
delegates condition calls through `ConditionRuntime`. `Game` implements that
boundary by resolving supplied runtime handles and running the condition
against its current world and Papyrus VM. Condition copy hooks return an
explicit unsupported-hook error.

Condition function 170, `GetDayOfWeek`, reads the live calendar and returns
zero for Sundas, with the starting date 17 Last Seed 4E 201 as day zero.
Functions 12 `GetSecondsPassed`, 62 `IsRaining`, 75 `IsSnowing`, 148
`GetCurrentWeatherPercent`, 153 `CanHaveFlames` and
154 `HasFlames` return explicit unsupported-function diagnostics. The local
xEdit table supplies their identities and parameter types, but the CK corpus
has no usable condition contracts for these queries. The runtime also has no
condition state for elapsed evaluation time or reference flames. The evaluator
rejects nonzero unused parameter slots on the parameterless functions.

AUD-13 implements function 149, `GetIsCurrentWeather`, against the shared
sky's selected WTHR identity. It is false before the sky selects a weather.
A transition reports its destination weather, rather than its outgoing blend
sources. The evaluator resolves the WTHR in the condition owner's winning
file and rejects missing, deleted or wrong-type forms and unused second
parameters. xEdit supplies the function identity and weather parameter type,
and CommonLibSSE-NG `include/RE/S/Sky.h` distinguishes currentWeather from
lastWeather. This reads existing RND-18 state without adding a save section.

**Procedures.** AI-02 walks sequential branches in authored preorder. The
actor retains the active child and its completion state. A completed Travel
starts the next child on the next driver tick. Nested `Sequence` branches
finish after their children, and `Repeat when Complete` restarts only the
branch carrying that flag. `Success Completes Package` stops the whole tree,
including enclosing repeats. Once Per Day is recorded after the tree finishes,
not at every Travel arrival. Conditions are checked when entering each branch.
Unsupported group types hold with an explicit `Procedure::Unsupported` value.
The tooling helper `procedure_of` reports the first procedure before conditions.
**Quest reference alias searches.** At quest start, a reference alias with
Match Conditions and no `ALFR` forced reference searches the winning `ACHR`
and `REFR` records in sorted FormID order. The scan stops before evaluation if
it would inspect more than 4,096 candidates. Each candidate must be enabled
unless the alias has Allow Disabled, and a dead actor must have Allow Dead.
Destroyed candidates require Allow Destroyed. In Loaded Area requires a
resident cell, not merely a reference built by an unrelated form lookup.
Every authored condition must pass with the candidate as subject and the quest
as owner. Unsupported conditions reject that candidate. The alias fills only
when exactly one candidate passes. Existing alias fills prevent reuse unless
the new alias has Allow Reuse in Quest, and reserved references require Allow
Reserved. An ambiguous or missing result leaves an Optional alias empty. A
required searched alias prevents quest start when it cannot be filled. This
uses the QUST alias flags and condition block defined in
`TES5Edit/Core/wbDefinitionsTES5.pas`. Other alias fill types, including
Create Reference to Object (`ALCO`), are not created by this path, and aliases
with `ALCO` are excluded from this placed-reference search. Closest search
remains unsupported because local format sources do not establish the runtime
distance origin or tie rule.

PRF-33 skips candidate construction when a quest has no searched reference
aliases. The first search derives a candidate index from the immutable load
order's sorted winners. The World retains at most 4,096 FormIDs, or an overflow
marker when the bound is exceeded, and reuses that result on later starts.
Enabled, dead, destroyed and resident state, conditions and alias reuse are
evaluated on each start. Runtime-created references remain outside this
authored search, as before. The index is not saved and adds no save section.
The implementation reuses `LoadOrder::winners` rather than introducing another
record database. `prf_33_quest_startup.rs` covers repeated starts, live
eligibility, winning overrides, search bounds, aliases and startup stages.
`startup_timing` reports startup phases through optional `soe-trace` spans.

**Specific location aliases, AI-18.** `Quest.Start` resolves a location
alias's `ALFL` through the quest plugin's load-order namespace and fills it
only when the winning form is a live `LCTN`. A missing or wrong-type target
leaves an Optional alias empty and prevents a required alias from starting
the quest. The location identity uses the existing quest alias state, which
the `SOEW` writer already saves and restores. This fill follows the Specific
Location rule in xEdit's `wbRecord(QUST)` and UESP's QUST format page.
Reference alias locations and external alias locations remain AI-19 work.

**Procedures.** AI-02 walks sequential branches in authored preorder. The actor
retains the active child and its completion state. A completed Travel applies
`Success Completes Package` and the following branch rules. Sleep walks to the
bed named by its location, or the nearest bed in the cell, and holds with
`sleeping` set. `Sit` reserves the first active, unreserved Sit marker on its
furniture reference, walks to that reference's placement transform, and releases
the reservation when it arrives. Furniture marker transforms are not read from
the model. The procedure state is stored in the `AI-20` actor section.
`Wait` holds for its real-time duration, with zero meaning indefinite. Walking
`Patrol` follows linked markers, including their arrival radius and idle time.
`Flee` chooses a reachable navmesh triangle whose centroid increases distance
from its live combat target, runs there at the race's run speed, then succeeds
the active tree branch. When the target leaves combat, Flee succeeds immediately.
If no reachable point increases distance, the actor holds position until the
target or navmesh changes.

`Shout` casts the equipped `SHOU`'s highest nonzero `SNAM` spell through
`World::cast_spell` and holds its tree branch until that word's authored recovery
expires. An actor already in voice recovery waits before casting. Failed shout
data or delivery leaves the procedure explicitly unsupported. Voice recovery is
an absolute deadline in the world's real-seconds clock, exposed through
`GetVoiceRecoveryTime` and `SetVoiceRecoveryTime`. The `AI-29` actor section
stores that deadline and the active Shout branch deadline under the version 37
section table. The CK package procedure page is absent from the local wiki
snapshot. The execution uses the local `EquipShout_-_Actor` and
`GetVoiceRecoveryTime_-_Actor` contracts, UESP `SHOU.wikitext`'s three `SNAM`
entries, and the existing xEdit PACK and shared spell APIs.

`Actor.GetVoiceRecoveryTime` reads the remaining duration from that actor's
real-seconds deadline. `Actor.PlayIdleWithTarget` queues the idle's editor ID
on the calling actor and retains the explicit paired reference in
`AnimationRequest.partner`. `None` leaves the partner override unset. It
returns false for a missing idle or invalid partner reference. The host sends
the event to the caller's graph. Partner alignment is not performed by this
bridge. The local CK `PlayIdleWithTarget_-_Actor.wikitext` defines the caller
and partner roles.
`Actor.GetEquippedSpell` reads sources 0 and 1 from the casting
hands, source 2 from the actor's other-spell slot, and source 3 from its
instant-spell slot. The latter slots persist in the actor section named
`SCR-15`. `Actor.Dismount` returns true only when it clears a set mounted flag.
`Actor.Trespass` sets the shared actor flag read by `IsTrespassing`. The local
CK pages consulted for these bindings are `GetEquippedSpell_-_Actor`,
`GetVoiceRecoveryTime_-_Actor`, `PlayIdleWithTarget_-_Actor` and
`Dismount_-_Actor`. The CK snapshot has no `Trespass_-_Actor` page. The
host mount placement is not provided, and these calls do not create crime
witness events.

Nested `Sequence` branches finish after their children, and `Repeat when
Complete` restarts only the branch carrying that flag. `Success Completes
Package` stops the whole tree, including enclosing repeats. Once Per Day is
recorded after the tree finishes, not at every Travel arrival. Conditions are
checked when entering each branch. Unsupported group types hold with an explicit
`Procedure::Unsupported` value. The tooling helper `procedure_of` reports the
first procedure before conditions. The parser's `ai_87` paired `Simultaneous`
fixtures keep `FNAM` clear and set as separate policies, including an unknown
bit case. Its `SKYRIM_DATA` survey checks that authored Skyrim.esm
`Simultaneous` procedure records include both policies.
`ForceGreet` is retained as an active package procedure. Without bound inputs,
the actor follows the package driver's navmesh path toward the player while no
greeting conversation is open. When the actor is within dialogue distance and
has an eligible Player-category ForceGreet topic, the ordinary dialogue
selector opens it. Closing that conversation completes the active procedure
branch. Bound target and topic inputs use the assigned greeting path described
below. Missing bound inputs remain unsupported. The `DLG-14` actor section
retains the procedure identity in SOEW version 37. The tooling helper
`procedure_of` reports the first procedure before conditions.

Input locations retain their defining package's namespace when inherited from
a template. SOEW version 16 saves the tree cursor alongside the existing path.
SOEW version 37 also saves the UseWeapon procedure tag. Versions 4 through 36
remain readable. Saves before version 16 have no tree cursor, so their
selected package starts its tree from the beginning on its first driver tick.
Empty repeating branches yield each tick without running later siblings.

An immediate `Simultaneous` group of condition-eligible `Wait` procedure
children starts every timer against the same non-menu clock value. Without a
successful child, the group completes after its longest finite timer and
records package completion once. A finite child with `Success Completes
Package` ends the tree when its own timer expires, even if another child has a
longer or indefinite timer. A zero duration keeps the group active when no
finite successful child completes the package. Changing the winning package
clears the group cursor and deadline without recording completion. Other
simultaneous child procedures remain unsupported until the completion policy
in AI-87 is established from paired authored records.

The layout follows local xEdit `wbRecord(PACK)`, `PRCB`, `FNAM`, and `PKC2`,
plus the local UESP PACK page. xEdit names the PKDT interrupt override values
and interrupt bits, including `Observe combat behavior`. The CK
`EvaluatePackage` page documents explicit package-stack reevaluation.
CommonLib's sequence header supplies a type
but no implementation. The nested-repeat behavior follows the mod author's
[repeat experiment](https://forums.nexusmods.com/topic/11591393-ai-package-wont-repeat/).
No reusable interpreter exists in those sources, so this interpreter uses the
existing world and procedure driver without a new dependency.

`Travel` walks to its input location and completes on arrival.
`Find` selects the nearest enabled placed reference matching its authored
Object ID, or the known Object Type value for any weapon, in its search cell.
An authored location radius limits the search. Equal distances choose the
lower resolved FormID. The selected identity is available as the actor's
`AiState::found_reference`.
`Sandbox` picks an idle marker or a piece of furniture (beds excluded) within
the location radius, walks there, dwells 0.1 to 0.4 game hours, then picks the
next marker or a random point when the cell has none. `Sleep` walks to the bed
the location names, or the nearest furniture in that cell whose base has a bed
keyword, and holds with `sleeping` set. `Sit` reserves the first active,
unreserved Sit marker on its furniture reference, walks to that reference's
placement transform, and releases the reservation when it arrives. Furniture
marker transforms are not read from the model. Its procedure tag is stored in
the `AI-20` actor section. `Wait` holds for its real-time duration, with zero
meaning indefinite. Walking `Patrol` follows linked markers, including their
arrival radius and idle time.

`Flee` chooses a reachable navmesh triangle whose centroid increases distance
from its live combat target, runs there at the race's run speed, then succeeds
the active tree branch. When the target leaves combat, Flee succeeds
immediately. The existing tree cursor applies `Success Completes Package` and
the following branch rules. If no reachable point increases distance, the actor
holds position until the target or navmesh changes.

`Shout` casts the equipped `SHOU`'s highest nonzero `SNAM` spell through
`World::cast_spell` and holds its tree branch until that word's authored recovery
expires. An actor already in voice recovery waits before casting. Failed shout
data or delivery leaves the procedure explicitly unsupported. Voice recovery is
an absolute deadline in the world's real-seconds clock, exposed through
`GetVoiceRecoveryTime` and `SetVoiceRecoveryTime`. The `AI-29` actor section
stores that deadline under the version 37 section table.

`UseMagic` takes its target from the procedure's `PTDA` input and accepts a
reference, self, linked reference or selected alias reference. It reads the
actor's assigned right hand spell, then left hand spell, and casts the first
known type-0 spell through `World::cast_charged_spell`. The shared path validates
effects and target, charges magicka, and applies the effects before the package
branch completes. An unresolved target or an ineligible or unaffordable spell
leaves the branch active as `Procedure::Unsupported`. It adds no saved cast
state. `UseWeapon` sends one ordinary melee contact through `World::melee_hit`
against the actor's current combat target, preferring its right hand weapon and
then its left. It completes its tree branch after the attempt. The shared combat
path validates equipment and combat modifiers and queues hit events. The
procedure state is stored in the `AI-36` actor section under the version 37
table.

Location kinds near reference, in cell, near package start, near editor
location, near linked reference, alias reference, alias location and near self
resolve. An alias location uses its filled `LCTN` form's `MNAM` world marker as
its destination. Location alias fill rules remain separate quest work. Other
procedure names remain `Procedure::Unsupported` and leave the actor standing.
The local CK package procedure page is absent from the local wiki snapshot. The
execution uses the local `EquipShout_-_Actor` and
`GetVoiceRecoveryTime_-_Actor` contracts, UESP `SHOU.wikitext`'s three `SNAM`
entries, and xEdit's PACK layout and shared spell APIs. The local CK
`EvaluatePackage - Actor` and `GetCurrentPackage - Actor` pages define package
evaluation and observation, not Flee movement. PACK branch completion uses
xEdit's `PRCB` flags and the UESP PACK layout. Route selection uses the shared
combat target and vanilla NAVM graph already used by combat movement. No
procedure-specific source in the local corpus defines Flee's destination
choice.

`Follow` resolves its `PTDA` target as a specific reference, alias, linked
reference or self. Its first two float inputs set catch-up and follow radii,
defaulting to 20 and 5 units when omitted. Invalid radii or unresolved targets
leave an unsupported procedure. Each driver tick recalculates a route to a
point at the follow radius from the target, stops inside that radius, and
selects run speed outside the catch-up radius. Routes use the same navmesh and
cell updates described below. The CK wiki's `KeepOffsetFromActor - Actor`
page defines the catch-up and follow thresholds. PACK input layout follows
local xEdit `wbRecord(PACK)` and Mutagen Skyrim `Package.xml`. The `AI-21`
actor section in SOEW version 37 saves the Follow procedure tag.

`UseIdleMarker` selects an enabled IDLM reference in the procedure location's
cell and radius, skips deleted and destroyed markers, walks the actor to it,
and holds that marker. When it arrives, the driver resolves the first valid
`IDLA` link on the IDLM base to an `IDLE` record and queues that record's
`ENAM` animation event. An actor with an active
`UseIdleMarker` procedure reserves its selected marker. Other actors skip that
reservation when choosing a marker. Changing or ending the package clears the
reservation with the procedure state. This uses the PACK input layout from
local xEdit `wbRecord(PACK)` and the CK wiki's `IsFurnitureMarkerInUse`
reservation semantics. IDLM and its idle animation list follow local xEdit
`wbRecord(IDLM)` and `wbIdleAnimation`. The `AI-21` actor section in SOEW
version 37 saves the selected marker and procedure tag.

`HoldPosition` resolves the branch location and holds within the driver's four
unit arrival tolerance of that position. Without a location input, it holds at
the position where the branch began. If the actor moves outside the tolerance,
the driver routes it back. The procedure stays active until its package stops
winning. The local CK wiki corpus has no HoldPosition procedure page. CommonLib
names the procedure type, while the package location layout comes from xEdit's
`wbRecord(PACK)`. The `AI-21` actor section in SOEW version 37 saves the tag.

`ForceGreet` resolves its branch target and dialogue topic, walks to the target,
then opens the assigned eligible topic when it arrives. It attempts the greeting
once per branch. An actor below full alpha cannot start the dialogue unless it
is a ghost, matching the CK wiki's `SetAlpha - Actor` rule. The attempt state and
procedure tag are saved in the `AI-21` actor section in SOEW version 37. The package remains active after the
greeting, and a later branch or package selection resets the attempt state.

`Activate` resolves the branch's target input to a placed reference, walks the
actor to it, and calls the shared game activation path once on arrival. The
actor is the activator, so default processing and `OnActivate` use the same
behavior as other game activations. The attempt flag is saved in the `AI-21`
actor section in SOEW version 37 and resets when the package or procedure branch changes. The local CK wiki
`Activate - ObjectReference` page defines the default-processing and event
rules. The target's event uses the local `OnActivate - ObjectReference`
contract.

`DialogueActivate` resolves the branch target through the package template
chain, walks toward it and starts a dialogue session with the package actor as
speaker once both actors are in the same space and within
`fAIInDialogueModeWithPlayerDistance`. The ordinary dialogue choice path
selects eligible topics. A busy conversation leaves the procedure ready to
retry. The attempt state and procedure tag are saved in the `AI-21` actor
section in SOEW version 37.
Changing the package or procedure branch clears the attempt state. The local CK
corpus has no DialogueActivate procedure page. Package target inputs follow
`wbRecord(PACK)` in xEdit. Range follows the existing dialogue distance rule
in `dialogue.rs`.

`ForceGreet`, `Activate` and `DialogueActivate` recalculate the assigned
target's position while approaching it, so movement before the attempt does
not leave the actor heading for an old position.

`Find` reads the branch's target input through its package template chain.
Object ID matches a placed reference's resolved base form. Specific Reference
selects that placed reference, Ref Alias resolves through the actor's selected
quest alias or the package's owning quest, and Linked Reference selects the
actor's first link or the link matching the authored keyword. These selectors
still require the selected reference to pass the search eligibility checks.
Object Type values 1 to 15 match their corresponding base record types. Values
14 and 15 distinguish potions from food through the `ALCH.ENIT` Food Item flag.
Values 16 and 17 match combat wearable armor and any armor. Combat wearable
uses the `ARMO.BOD2` or `BODT` armor type, where 0 is Light, 1 is Heavy, and 2
is Clothing. Values 18 to 20 select unarmed, melee, and ranged weapons by
`WEAP.DNAM`. Values 21 to 24 select spells by delivery, 25 selects placed
actors, 26 and 27 select beds and chairs by furniture animation, and 28
selects shouts. Unknown Object Type values and other target selectors are
unsupported. A negative authored radius is unsupported. The search uses the
authored location's cell and optional radius,
or the actor's current cell when there is no location. Disabled, deleted,
destroyed, invalid-position and self references are skipped. An unresolved
authored location does not fall back to the actor's cell. The nearest match wins, with
resolved FormID as the deterministic tie break. The local CK corpus has no
Find procedure page. Target kinds and Object Type labels follow xEdit's
`wbDefinitionsTES5.pas` `PTDA` and `wbObjectTypeEnum` definitions. Its `ARMO`
`BOD2` and `BODT` definitions supply the armor subtype offsets and enum. The UESP
PACK page documents value 11 as any weapon, and xEdit's `wbRecord(ALCH)`
defines the Food Item flag. Package alias resolution uses the actor's selected
quest alias or the package owner quest. Linked references use the actor's
`XLKR` links. No reusable local runtime implementation covers Find.

The `AI-21` actor section in SOEW version 37 saves the selected reference with
the active Find procedure.

`Hover` reads a location and finite height from its branch inputs, moves to
the location's X and Y at its Z plus that height, and holds there. The actor
reports flying state 3 and `IsFlying` true while Hover runs. Replacing the
package clears its target and height, ending the flight state. The height and
procedure state are saved in the `AI-21` actor section in SOEW version 37.
xEdit's `wbRecord(PACK)` defines package location and float inputs. The CK
wiki's `GetFlyingState - Actor` page defines 3 as Hovering, and its
`IsFlying - Actor` page defines the Boolean query. The corpus has no Hover
procedure contract page.

`Orbit` resolves its `TargetLocation` input on every driver tick, then moves
around that point at the authored `Height`. It keeps the actor between the
authored `InnerRadius` and `OuterRadius`, and advances only when the sampled
arc stays inside both bounds and over a navmesh. The arc and direct movement
segment sample at intervals no longer than 0.05 world units, so a direct step
cannot cut across an off-nav gap when a route exists around it.
Missing inputs, non-finite values, negative inner radii or an outer radius
below the inner radius leave an unsupported procedure. The local CK wiki dump
has no Orbit procedure page. `CommonLibSSE-NG/include/RE/Offsets_RTTI.h`
confirms the Orbit procedure and execution-state types but does not describe
their behavior. The `AI-21` actor section under SOEW version 37 saves the
active Orbit height.

`Escort` resolves its assigned placed-reference target and the branch's
authored location. The escorting actor routes to that location while keeping
within the target's authored distance when the target is behind. It waits when
the target falls outside that distance, and can keep advancing when the target
is closer to the destination. It completes when both the actor and target are
within the driver's four unit arrival tolerance of the destination. Missing or
unresolved target and location inputs, negative distances and non-reference
target selectors leave an unsupported procedure. Object search, pickup and
carrying are not implemented. Local CK package pages do not describe Escort
procedure behavior. The PACK target and location input layouts follow xEdit's
`wbRecord(PACK)`. The destination, target distance and actor escort behavior
follow the GECK Escort package page, since the local CK dump has no Escort
procedure page. No existing local implementation covered this procedure. The
`AI-21` actor section under SOEW version 37 saves the Escort procedure tag.

`tests/ai_29_shout.rs` checks authored self-target spell delivery, the selected
word's recovery duration, waiting through an existing recovery, save/load, and
rejection of a SHOU that does not contain exactly three 12-byte `SNAM` entries.

**Walking.** `ai::nav_graph` builds `soe_navmesh::vanilla` over every
winning `NAVM` on first use and remembers which cell each mesh sits in.
A route is `find_path_between` from the actor to the place. The actor follows
the waypoints at its race's forward walk or run rate: `WKMV` supplies walk and
fast walk, `RNMV` supplies jog and run, and a matching `MTYP`/`SPED` pair on
the race overrides the selected movement type. Missing or unusable data falls
back to 110 units per real second (walk), 160 (fast walk), 180 (jog) or 375
(run). Each pulled waypoint carries its triangle owner. Reaching a portal into another
mesh moves the actor into that mesh's cell, which is how travel across a cell
link changes `RefState::cell`. Explicit gap departures retain the source cell
until the arrival waypoint. Off the graph the route is a straight line. The driver
skips the player, the dead and the unconscious.

**Combat movement.** An Aggressive, Very Aggressive or Frenzied actor acquires
the nearest live actor whose awareness reaches Combat and retains that target
while it remains live. A package with `Ignore Combat` leaves package movement
in control. Combat movement uses the same NAVM graph but requires a route,
stops within four units of the target, and does not use the package driver's
straight-line fallback. The driver clears combat and requests package
reevaluation after the target dies. Combat target selection is simpler than
the retail selector and does not compare ranged loadouts, group attacker
counts, fleeing state or encounter zones. The `cmb_17_combat_engagement` tests cover acquisition,
unaggressive rejection, the package flag, navmesh pursuit and death cleanup.

**Live run (2026-09-18).** Every one of the 5,961 `PACK` records in
`Skyrim.esm` parses (104 templates, 2,384 with conditions, 1,545 with a
scheduled hour) in 7.9 s including the plugin parse. An earlier survey had
4,635 supported packages: 2,467 `Travel`, 1,841 `Sandbox` and 327 `Sleep`.
The updated driver supports 5,013: those procedures plus 300 `Sit`, 52
`UseMagic` and 26 `Shout`. Remaining template procedures: Patrol 378,
HoldPosition 141, Follow 128, UseIdleMarker 61, Orbit 47, Escort 45, Activate
40, ForceGreet 31, Guard 24, Hover 14, Flee13, Find 11, Wait 11, DialogueActivate 2, UseWeapon 2. The `Prisoner`
base carries no `PKID` list at all, so the innkeeper `Hulda` stands in:
her stack is `WhiterunInnkeeperPackage`, which resolves to `Sandbox` at
every hour tried, and no condition function on the way was unsupported.
The synthetic tests (`tests/packages.rs`) walk a guard across two
navmeshed cells on a travel package once a quest stage unlocks it, sandbox
to an idle marker, sleep on schedule and wake at 08:00, take an alias
package over the base list, switch at once from a script's
`EvaluatePackage`, and keep walking after a save and load.

AI-14 evaluates eligible simultaneous children in tree order. Wait children
share one real-time deadline, using the longest active Wait unless a child
marked to complete the package supplies an earlier deadline. UseMagic children
cast through the actor's assigned spell path while that Wait group runs. A
UseMagic child marked to complete the package ends the group after its cast.
Async procedure children remain unsupported.

**Not done.** Simultaneous and stacked branches (AI-14 and AI-15),
the six remaining named procedures and the Patrol modes tracked by AI-88 through AI-91,
idles and topics, once per day for
packages that end without arriving, `Must Complete`, interrupt flags, combat
interrupt overrides, door unlocking flags, the `Package Data` and `Event Data`
condition targets, and every condition function outside the list above.

## Natives round two (2026-09-18)

The round started from what the first `start` could not dispatch. Grouped by
function the 344 unresolved calls (300 "no function" plus 44 "no
implementation", which the earlier count had left out) were
`Location.SetKeywordData` 149, `LeveledItem.AddForm` and the other two lists
55, `Faction.SetAlly` and `SetEnemy` 43, `ReferenceAlias.GetReference` 26,
`Location.GetKeywordData` 14, `Alias.RegisterForUpdate` 8, `Actor.AddPerk` 8,
`ObjectReference.AddToMap` 6, `Keyword.SendStoryEvent` 4, `Scene.Start` 3,
and a tail of one to two calls each (`Message.Show`,
`ActorBase.SetInvulnerable`, `ObjectReference.SetDestroyed`,
`LocationAlias.GetLocation` and `ForceLocationTo`, `Form.RegisterForSleep`,
`RegisterForTrackedStatsEvent`, `ReferenceAlias.AddInventoryEventFilter`).

Two dispatch fixes came first. `call_method` now retries a native its
declaring class does not implement on every class above it (see
`docs/papyrus.md`, redeclared natives), which resolved the 34 alias calls
without a new registration. And a VM object for a quest alias takes its
class from the alias kind, `LocationAlias` for `ALLS` and `ReferenceAlias`
for `ALST`, with `start_quest` delivering `OnInit` to the alias objects that
carry a script, the way the game does.

Then 153 natives with state behind them, listed in the conformance table
above: the three leveled list classes, `Location`, `Faction`, `ActorBase`,
perks and fourteen actor flags and relations, location and ownership on
references and cells, the map marker and destroyed flags, inventory event
filters, the sleep, tracked stats, animation and line-of-sight
registrations on `Form` with the events they deliver, the objective global
natives on `Quest`, `Message`, `Scene`, `Sound`, `Spell`, and the `Game`
flags, words, achievements and setting overrides. The `NPC_` view gained
`PRKR`, `CRIF`, `DOFT` and `SOFT`, references and cells read `XOWN` and
`XLCN`. The world save format is version 2 (`SOEW`) and carries all of it.

Against the installed game after the round: 423 quests started, 85 distinct
natives reached through 1,517 calls, 1 unresolved call
(`Actor.SendVampirismStateChanged`, which the DLC round below registered),
87 log lines, in 0.3 seconds in a debug build. The most called natives are
`Location.SetKeywordData` 149, `Game.GetPlayer` 100,
`GlobalVariable.GetValue` 93, `Debug.OpenUserLog` and `Game.GetForm` 77 each,
`Quest.IsStageDone` 60, `ObjectReference.GetLinkedRef` 56,
`LeveledItem.AddForm` 54, `Alias.GetReference` 45,
`ObjectReference.GetCurrentLocation` and `Quest.SetObjectiveDisplayed` 42
each, `Faction.SetAlly` 37. The remaining log lines are calls on `None`
objects (`cannot call RegisterForUpdate() on a None object` from
`QF_MS14` and the caravan fragments), which is the game's own behaviour for
an alias or property that is empty at start.

## The DLC natives (2026-09-18)

Dawnguard, HearthFires and Dragonborn sit in every gated load order, so what
their scripts call is engine work. The gated `real_files` test now reads the
native flag off every loaded `PEX` function and prints what the registry does
not cover, with a static call-site count per name and the share of those
sites that sit in a `DLC1`, `DLC2` or `BYOH` script:
`Vm::declared_natives`, `Vm::unregistered_natives` and
`Vm::call_sites_by_script` in the papyrus crate do the counting. On the
installed game the five masters declare 686 natives, of which 299 had no
registration and 163 were called from a DLC script.

Thirty of them are now real, ordered by how often the DLC scripts reach
them. The two state-change senders are the interesting pair:
`SendVampirismStateChanged` and `SendLycanthropyStateChanged` "tell anyone
who cares" by pushing `OnVampirismStateChanged` and
`OnLycanthropyStateChanged` onto the pending queue for the scripts attached
to that actor, the same delivery `OnActivate` takes. Around them sit the
dragon riding flags (`SetAllowFlying`, `SetAllowFlyingEx`, `IsAllowedToFly`,
and `CanFlyHere`, which checks worldspace max-height data), the
deferred kill pair with `KillSilent`, shouts, the spell dispel and equip
functions, `SetRace`, `SetEyeTexture`, `SetCriticalStage`, `AttachAshPile`,
the bleedout and AI flags, `WornHasKeyword`, `IsPlayersLastRiddenHorse` and
`Game.AddPerkPoints`. Sixteen more went in as quiet stubs because the system
behind them does not exist yet: flight, active magic effects, the ragdoll,
werewolf crime, jail time and the sun. SCR-17 later connected the warmth query
to equipped ARMO keywords.

Decisions taken rather than read, since no wiki page states them:

- The senders record the announced state on the actor (`vampire`,
  `werewolf`). The game keeps it in quest variables instead, but a recorded
  flag is what lets a scenario check that the call landed.
- `Kill` now honours the deferred kill flag, so `StartDeferredKill` holds a
  death that `EndDeferredKill` releases. Vanilla raises a kill event that
  the VM does not send yet, which is the only thing separating `Kill` from
  `KillSilent` here.
- `PlaySubGraphAnimation` routes to the animation event path, so a script
  registered for that event name hears it. The behavior graph has no sub
  graph concept yet.
- `CanFlyHere` returns whether the actor's worldspace has max-height data.

A second pass added the reference animation graph (`SetAnimationVariable`
and `GetAnimationVariable` in all three types, one store per type on the
reference), `SetAlpha`, `PlayIdle`, `GetNthLinkedRef` and `PlaceActorAtMe`,
with quiet stubs for the shader, image space modifier, music and translation
calls the DLC scripts lean on hardest. MAG-24 now stores and releases
transient VisualEffect target attachments. It also fixed a real gap next
door: the `ACHR` reader ignored `XLKR`, so an actor reference had no linked
references at all. Actor references carry them the same way a `REFR` does,
and the DLC scripts use them to find a coffin or a cart.

with quiet stubs for visual effects, image space, music and translation. It
also fixed a real
gap next door: the `ACHR` reader ignored `XLKR`, so an actor reference had
no linked references at all. Actor references carry them the same way a
`REFR` does, and the DLC scripts use them to find a coffin or a cart.

`ObjectReference.TranslateTo`, `SplineTranslateTo` and `StopTranslation`
advance from the unpaused game tick using absolute world targets. Linear and
cubic spline paths move at their configured units per second. Rotations are
stored in radians, use the Papyrus degree-per-second limit, and do not finish
before the position. A spline snaps to its authored endpoint when complete.
At 90 percent positional progress the runtime queues
`OnTranslationAlmostComplete`, even if rotation is still in progress.
Completion queues `OnTranslationComplete`. Stopping, blocking, replacing an
active request, or advancing a disabled or deleted reference queues
`OnTranslationFailed` and leaves its current placement in place. The in-flight
action is saved in the reference section `HST-16` and included in the
deterministic world hash. These paths update the shared world placement. The
host reflects that placement through the existing `ref_placement` contract.

Against the installed game after the round: 423 quests started, 81 distinct
natives reached through 1,388 calls, **zero unresolved calls**, 87 log lines,
in 0.1 seconds in a release build. The gated test asserts the zero, so a new
vanilla call that reaches nothing fails the run instead of printing a line.
The call count fell from 1,560 with the same 423 quests and the same 87 log
lines, and reading `XLKR` on actor references is the whole of the
difference, measured by turning that one read off and watching the numbers
go back. Scripts that used to get `None` from `GetLinkedRef` on an actor now
get the reference the record names and take the shorter branch, which is why
`Game.GetPlayer` falls from 99 calls to 57. The remaining log lines are
calls on `None` objects, which is the game's own behaviour for an alias or
property that is empty at start.

A third pass took the two biggest remaining groups. `GetOwningQuest` on
`Scene` and `Package` reads the record: a scene names its parent quest in
`PNAM`, a package its owner quest in `QNAM`, which is what 1,815 DLC call
sites were asking for. `TopicInfo.GetOwningQuest` follows the INFO's dialogue
group to its DIAL and reads that topic's owning quest. The
`ActiveMagicEffect` registration set went in next: an active effect is not a
`Form`, so the registrations the registry holds on `Form` were out of its
reach, and since they key off the calling object rather than a form the same
behaviour registers cleanly on the effect class. The later active-effect
implementation below supplies its getters and `Dispel` using a separate
object for each effect application.

### SCR-75 native-form property execution

The SCR-75 installed-data test loads the five masters and their scripts, then
executes mixed-case property reads and writes on `GameDaysPassed.Value`.
The VM resolves the loaded `GlobalVariable` PEX accessors on the native form.
They call `GetValue` and `SetValue` against the same shared world global used
by the five methods below. This follows the shipped
`skse64/scripts/vanilla/GlobalVariable.psc` and CK's
`GlobalVariable_Script.wikitext` read-write float property contract.
`crates/game/tests/scr_75_walkthrough.gd` extends the installed walkthrough,
runs 20 seconds after gameplay is ready and quits. Startup that has not reached
ready after 120 seconds fails the check. Set `SOE_SCR_75_LOG` to the captured
console log. The check fails if that log is missing, empty or contains
`has no property value`, compared without case.

### SCR-62 native contract audit

The native registry contains `GetOwningQuest` methods for `Alias`, `Scene`,
`Package` and `TopicInfo`. `Alias` returns the quest that owns the alias.
`Scene` resolves `PNAM`, `Package` resolves `QNAM`, and `TopicInfo` follows
its dialogue group to the owning `DIAL` quest. CK's Alias page documents the
alias contract. The fragment class method pages are absent from the local
CK corpus, so their record relationships are checked against the records
read by `World::owning_quest` and synthetic tests.

The five `GlobalVariable` methods are `GetValue`, `GetValueInt`, `SetValue`,
`SetValueInt` and `Mod`. They read and write the shared world global value.
The CK class page lists their signatures and behavior, and its `GetValue`
page documents the float getter. The shipped `GlobalVariable.psc` supplies
the integer casts and describes `Mod` as the thread-safe update path. The
registry and native tests cover all nine bindings and the getter, setter,
cast and modification behavior.

172 declared natives are still unregistered, 56 of them called from a DLC
script, the largest single group being the rest of `ActiveMagicEffect` and
the casting functions. Two earlier readings of that number were wrong and
are worth keeping straight: the check now walks the class chain the way
dispatch does, so a script redeclaring a parent's native no longer counts as
missing, and the scheduler's own `Utility.Wait` family no longer counts
either.

## Save and load

`World::save_state(&self) -> Vec<u8>` and `World::load_state(&mut self,
bytes) -> Result<(), VmError>` write and read the run-time half of the form
table, magic `SOEW`, version 36. Versions 4 to 6 use legacy actor values,

table, magic `SOEW`, version 37. Versions 4 to 6 use legacy actor values,
version 5 adds active magic, version 6 adds dialogue, version 7 adds actor-value
components, version 8 adds dialogue to those components, and version 9 adds
active magic. Version 10 adds per-reference leveled template choices.
Version 11 adds quest journal instances and mapped runtime metadata.
Versions 12 to 15 add distinct inventory stacks, record overrides, temporary
NPC bases and deferred deaths. Version 16 contains difficulty alone, version
17 weapon hands alone, and version 18 both. Version 19 separates created
identities from plugin load-order position 255 while retaining both combat fields.
Version 20 appends restored reference ownership and lock metadata after those fields.
Versions 21 to 23 append the package procedure cursor, the Wait tag and the
walking Patrol cursor. Version 24 adds an actor's exclusive dialogue branch,
version 25 the image-space stack, versions 26 and 27 loose item stacks and
locked encounter-zone levels, version 28 the do-all dialogue cycles, and
version 29 an actor's stagger end time. Version 30 appends shared stealth-point
pools with each observer's detection level and timers, written only when a pool
exists. Version 31 adds the set of perpetrators an actor witnessed committing a
tracked crime.
Version 32 stores assigned hand spells, version 33 retains summoned actors,
version 34 retains encounter-zone visit days, version 35 retains list change
metadata, version 36 retains scene progress, and version 37 retains PACK and
NAVM change markers in record overrides. Version 37 is the last version bump. It
adds named world, actor and reference section tables. New actor procedure state,
from Hover through DialogueActivate, is stored in the `AI-21` actor section.
The provenance-aware native game save envelope and Fallout 4 struct values in
saved Papyrus state also use version 37.

Versions 32 to 36 add assigned hand spells, summoned references, encounter-zone
visit times, saved list flags and chances, active scene phase state, and BOS
record flags on instantiated references. Version
37 persists INFO said-once markers, SMQN last-run float bit patterns, and SCEN
active markers and exact runtime bytes in record overrides. The save crate
validates SCEN action widths, counts and field boundaries before this import.
The `World::restore_vanilla_dialogue_scene_changes` path validates all targets
before applying them. INFO state updates dialogue history, SCEN state updates
the playing-scene set at the saved phase, and the record accessors expose the
saved values. Saved `CHANGE_FORM_FLAGS` bits toggle the matching live record
flags. SCEN action progress is not restored. INFO has no saved timestamp, so
its said-once marker uses the current game day. Version 37 also retains PACK
and NAVM change markers in record overrides.

SAV-58 stores opaque unresolved record payloads and plugin-relative links in a
world section. These records are not installed into the live form table.
Papyrus SOEV version 4 carries script source fingerprints so missing or
changed definitions keep saved VM state suspended.

SOEW versions 21 and later retain each driven actor's package procedure
cursor, including the owning procedure tree, branch index and active or
finished state. Loading an engine save lets the driver continue that branch.
The `sav_13_engine_save_resumes_saved_sequence_procedure_without_restart`
test checks package identity, cursor restoration and continued movement.
Vanilla ESS package state is not connected to this cursor: named Sequence and
Stacked procedure payloads are not decoded, so a saved actor cannot yet resume
its vanilla procedure.
`sav_13_saved_actor_resumes_at_its_saved_package_procedure` checks that an
engine save restores the running package, procedure and tree cursor before
both actors continue to the same route result. CK documents
`GetCurrentPackage` as returning the package the actor is running, while
`EvaluatePackage` forces its stack to be evaluated again. This check covers
engine-save continuation. It does not apply ESS `PackageData` to live actors.
Older formats load with empty state for absent subsystems.
Legacy current and base values must agree. Otherwise their modifier domains
are ambiguous and restoration returns an explicit error.
The state uses the same `Writer`/`Reader` encoding as
`soe_papyrus::vm::state`, an `ArrayPool` and `ArrayTable` of its own ahead of
the body for the `Value` arguments a pending event or call is carrying.

Created references and temporary bases use `DYNAMIC_FILE`, an alias of
`soe_esm::CREATED_FILE` (`u16::MAX`). Their runtime FormIDs retain `FF` packing.
Regular and light files can occupy internal position 255 without aliasing
created forms. `SavedFormMap` accepts the full 254 regular and 4096 light
plugin lists, remaps each by name, and keeps created RefIDs in this namespace.

SOEW versions 4 to 18 used position 255 for created forms. Their typed readers
now migrate that position across handles, references, AI, dialogue, quests,
inventory extras, template choices, record links, active effects and weapon
hands. Scalar values and opaque bytes remain unchanged. Legacy saves require
at most 255 loaded plugins. At 256 or more, the old namespace is ambiguous
and loading fails before world or VM mutation. Engine saves still require
the identical load order used to write them. This change does not infer a
missing or reordered plugin list from the legacy format.

`World::retain_unresolved_record` stores opaque bytes under a normalized plugin
filename, regular/light kind and local object number. Link identities use the
same fields. The SAV-58 world section preserves these values without resolving them
through the current load order, so removed plugins and compacted slots cannot
alias these retained records. This storage API does not populate itself from
ESS change forms. Native `Game::load_state` accepts plugins missing only from
the end of their regular or light slot sequence. Every remaining plugin must
retain its kind, slot and fingerprint, and the current load order cannot add a
plugin. Missing plugins before a still-loaded plugin remain an error, so saved
numeric form slots cannot silently retarget. This permits opaque SAV-58 records
to survive native save and reload while removed plugins remain absent. It does
not remap ordinary world identities or import opaque records from ESS.

The local UESP `Save_File_Format.wikitext` created RefID rule and Mutagen
`Plugins/FormID.cs` and `docs/plugins/Compaction.md` informed this change.
The existing Rust cursors and form table are reused, with a game-side reader
carrying namespace migration through nested structures. No dependency or
reference-source code was added. The public standalone AI reader retains
its existing signature and reads current identities.

The `sav_07` tests cover all 4,350 plugin positions with position 255 occupied
by either plugin kind, created references sharing plugin object numbers,
runtime packing and lookup, all saved regular/light slots and created RefIDs,
engine round trips and allocation afterward. Rich synthetic saves cover
versions 15 to 18 including difficulty and hand variants, nested created
links and unchanged scalar bytes. The exact 255/256 plugin boundary and
unchanged whole-game bytes after ambiguous legacy loads are asserted.

A form's record-derived data is not stored twice. A quest's definition (its
stages, objectives and aliases as `QUST` and `VMAD` wrote them) is read back
from the load order through a pure definition lookup. A failed load does not
create lazy quest state. `load_state` keeps the mutable half a running game changes:
whether it is running, completed, failed or active, the current and highest
stage, the done set, per-objective displayed and completed and failed flags,
where each alias currently points, journal instances and runtime metadata.
Everything with no record behind it,
every reference's position, cell, enable state, inventory and links, a door's
own teleport, every actor value, faction and spell, every global's current
value, the VM object and handle tables, the dynamic id counter, the player,
the RNG state, the clock, and whatever is queued (pending events and calls,
update timers, notifications, message boxes, animation events, the INI
overlay), is written and read in full since there is nothing to recompute it
from.

Actor values retain base, permanent, temporary and damage components. The
SAV-06 `sav_06_actor_value_modifiers` tests verify current and maximum values
through `Game::save_state` and `Game::load_state`.

`Game::save_state(&self) -> Result<Vec<u8>, VmError>` and
`Game::load_state(&mut self, bytes) -> Result<(), VmError>`
(`crates/game/src/save.rs`) wrap `World::save_state`/`load_state` and
`soe_papyrus::Vm::save_state`/`load_state` behind one envelope, magic
`SOEG`, version 2. The envelope stores sorted plugin names, regular/light
kind, load-order slot and SHA-256 plugin fingerprints, followed by loaded
script names and optional SHA-256 PEX fingerprints. `Game::saved_provenance`
reads this evidence. `load_state` rejects missing or changed plugins and slot
movement before replacing state, preventing a reordered plugin from silently
retargeting saved form IDs. Script provenance is reported by
`preflight_dependencies`, while the VM's saved script fingerprints validate
the definitions that saved instances and frames actually use. Missing or
changed definitions keep the whole VM payload suspended until compatible PEX
loads. World and VM state are both validated in scratch state before either is
replaced. Programmatically loaded scripts have unknown source fingerprints.
Each world and VM blob remains length-prefixed. The caller loads definitions
before restoring into a game that has not been bound or started.

The provenance test `tests/sav_56_provenance.rs` checks that a same-name
replacement changes the plugin or script fingerprint while a plugin slot move
leaves its content fingerprint unchanged. This extends the local save-format
precedent: OpenMW's `apps/openmw/mwstate/statemanagerimp.cpp` writes active
content file names and maps saved content indices on load, while the vanilla
ESS plugin table records plugin names and regular/light membership without
content fingerprints. The native envelope records the load-order slot and
loaded definition fingerprints so these cases remain distinguishable.

`Game::preflight_dependencies` compares native save provenance with the
current load order and scripts. Its deterministic findings identify missing
or added plugins and scripts, plugin kind or slot changes, changed
fingerprints, and fingerprints that cannot be compared because source bytes
are unknown. Each finding identifies whether it affects world or VM state.
The method reads the save envelope and does not mutate the game.

`Game::preflight_vanilla_dependencies` compares ESS plugin names and Papyrus
script names from definitions, inherited parent definitions, instances,
references, active frames, queued messages and suspended calls with the current
definitions. It reports missing plugins and scripts, regular/light kind
changes and unknown provenance for each saved plugin or script because ESS
stores no content fingerprints. A malformed Papyrus block returns an error.
Findings have a stable ordering.
The field boundary follows UESP `Save_File_Format.wikitext` and FallrimTools
`PluginInfo.java` and `papyrus/Script.java`, which expose saved plugin and
script identities without content fingerprints. `tests/sav_57_preflight.rs`
checks native and synthetic ESS differences, byte-preserving preflight, and a
`SKYRIM_DATA`-gated survey using installed masters and a loaded PEX script.

`Game::write_file` serializes and validates native state before replacing an
existing file. `Game::write_state_file` accepts previously serialized bytes and
checks them against the current load order and scripts in scratch state. Both
use the [atomic file protocol](save-resilience.md#sav-62-file-replacement),
including read-back validation, file and directory synchronization, a prior
link during replacement and explicit error paths. Validation executes no
callbacks and does not mutate the running game.

The `tst_06_process_termination_at_every_boundary_preserves_a_readable_generation`
test in `tests/atomic_save.rs` kills separate native and ESS writers at each
fault-injection boundary. The destination and any retained prior link must
remain readable as a complete generation. Run it with
`cargo test -p soe-game --features save-fault-tests --test atomic_save`.

`GenerationStore` adds [verified native generations](save-resilience.md#sav-63-verified-generations).
`Game::write_generation` validates a new file before bounded rotation.
`select_generation` reports damaged or incompatible candidates and returns the
newest one loadable under the current definitions. `recover_generation` loads
those verified bytes. Retention preserves loadable fallbacks before intact
incompatible files, with a minimum configured limit of two generations.
The `sav_63_` tests also corrupt a newly published generation after an
interrupted rotation, then recover the older verified state and finish a
bounded retry.

The `sav_61_` tests cover the available recovery boundary: an accepted native
generation lives in a separate directory and leaves the source save bytes
unchanged, while a generation written against a different script schema is
rejected without changing the live game. Dependency preflight, unresolved
record and script preservation, and reconnection are not implemented in this
tree, so these checks do not establish missing-mod recovery play or a host
recovery scenario.

A test in `tests/quest_flow.rs` calls the new `Game::activate_no_settle`
(the part of `Game::activate` before `run_until_idle`) on the lever so the
`OnActivate` event is queued but not yet run, saves, loads the bytes into a
freshly built `Game` with the same scripts and no binding done, then runs
until idle and checks it reaches the same end state as
`a_quest_advances_on_activation_end_to_end`: the quest completed at stage 20,
the objective completed, the gold given, the lever disabled, the
notification fired, and the alias resolved to the lever.

### Save sections

Version 37 is the last `SAVE_VERSION` bump, and appends a named section
table (`crates/game/src/world/save_sections.rs`) after the world's own
saved fields, after each actor record and after each reference record.
A table is a `u16` count then, per section, a name and a length-prefixed
payload, read through the same `FormReader` context (so a section that
stores a FormId resolves it the way the rest of that record does). A row
that adds saved state registers a `Section` in
`crates/game/src/world/save_section_registry.rs`, at the level that owns
the field, named after the row (for example `"MAG-39"`), instead of
raising `SAVE_VERSION` again. The registry file carries `.gitattributes`
`merge=union` so two branches adding sections in parallel merge without a
conflict. An absent section leaves its field at the default. An unknown
section name, a truncated payload or a payload a section's reader does not
fully consume are all load errors naming the section and the level.
Versions before 37 have no section table at any level, and the reader
skips looking for one below version 37.

The fixed world, actor, reference and faction bodies retain the layout at
`72add3f8`. Version 34 stores encounter-zone visits, version 35 stores list
metadata, and version 36 stores scene phase and completed action slots.
AI-80 faction attacks, MAG-43 Reanimate owners and CRF-03 learned effects
use world sections. MAG-72 weather and time image-space selection and MAG-19
active-effect taper values also use world sections. RND-18 stores weather
requests, climate or region identity and weighted transitions in a world section. An
absent RND-18 section restores the default weather state. Missing sections
restore an empty selection and untapered effects. AI-64 movement flags and AI-15
stacked package frames use actor sections. PRF-36 stores the coarse movement
clock remainder and next update in an actor section. MAG-64 pending reference bindings use a world section,
after AI-16 script calls, to preserve their combined queue order. The source
for this engine-owned layout is the frozen repository writer, rather than
the external vanilla ESS references.
SCR-74 prevented equipment removal is stored in an actor section.

Legacy test fixtures measure trailing cuts with `version_tail_len` or
`truncate_to_version` in `tests/common/save_tail.rs`, using a fresh world and
the requested version's reader. Invalid fixtures reuse a cut measured on a
valid counterpart. Corruption fixtures locate serialized field markers or
parse their envelope instead of counting back from the save's end. The
`tst_15_` tests cover empty worlds, actor and reference section removal,
source-state preservation and the absence of literal tail offsets with two
or more digits. These helpers reuse the layouts in `world/save.rs` and
`world/save_sections.rs` and do not change the save format.

### Player appearance editing, UI-14

The player actor stores selected HDPT forms by part type and normalized face
morph values by name. The race menu validates a head part's DATA playability
and sex flags and its PNAM part type before changing the actor. Morph names
must be nonempty and values must be finite and between -1 and 1. The `UI-14`
actor save section stores these form links and values. Missing sections leave
the appearance empty.

## Not done, and known gaps

- Race starting attributes are read. Live class/level calculations, racial
  skill bonuses and XLCM actor difficulty selection remain unfinished. Runtime
  template groups share persistent leveled choices, with the remaining
  inheritance boundaries documented in [actor-values.md](actor-values.md).
- Weapon combat remains unfinished. Dialogue and magic have the limited
  execution paths documented below. Packages run three
  procedures, see the Packages section for the rest. Scene phase advancement
  is action-gated. Package and timer actions execute, but phase and scene CTDA
  evaluation, dialogue action dispatch, and repeat-while-conditions behavior
  are not implemented.
  Perks are a set on the
  actor, no entry point applies. Leveled lists are never rolled. Spell casts
  are recorded, no effect applies. Crime has bounty and infamy ledgers, no
  arrest implementation. Arrest choice accounting and jail sentences exist,
  but guard movement, jail placement and resistance combat are not implemented.
- `Weapon.Fire` validates its WEAP, source reference and optional AMMO through
  the shared form table. Invalid Papyrus arguments and invalid forms are logged
  as typed errors. A valid request logs an unsupported error until ranged
  projectile execution exists, without changing inventory or queuing events.
  See [combat](combat.md).
- Faction relations from `XNAM`, location keyword data, perks and the actor
  flags were tested on the synthetic master and reached by the vanilla
  scripts at start, not checked against the game's runtime values.
- Quest aliases are filled only from `ALFR`. Conditions, `ALCO`, location and
  reference fill types are not evaluated.
- `SetStage` does not display the log entry or play the quest sound.
- Missing localized entries come back as `<string N>`. Selected-language
  string tables now load through the VFS, as described below.
- `bind` attaches non-reference scripts at start. Placed references attach
  from the cell index when their cell is loaded. `Game::run_until_idle` binds
  the focused cell before running queued VM work. The CK `OnCellAttach` page
  says the event fires for disabled references and precedes `OnCellLoad`.
  Cell scripts are bound after the `World` transition has queued its events,
  so this binding step does not synthesize cell events for references that
  did not already have VM objects.
- No native has been checked against its CK wiki page yet, see above.

## Cell transitions (2026-09-18, branch `transitions`)

The SCR-01 attach-binding repair preserves lazy binding for unattached cells.
World includes unbound VMAD carriers in its cell-event audience and queues
`Pending::ReferenceEvent` when a recipient has no VM object. Game binds all
attached cells, including streaming neighbors, and resolves those recipients
before dispatch. The Game transition methods expose resolved `Pending::Event`
entries to host callers. Direct World transitions from natives use the same
binding path at the next pump, including coc and doorway commits. Prefetching
alone attaches no scripts and queues no events.

The named SCR-01 world save section preserves unresolved event positions.
Loading state clears the binding caches, and binding retains existing saved
script instances and properties. Startup binding does not replay attach events.
The source rules remain local CK `OnCellAttach_-_ObjectReference.wikitext`
and `OnLoad_-_ObjectReference.wikitext`, including disabled-reference audiences.
These pages specify events rather than reusable runtime code, so this repair
uses the existing World queue and Game binder. The `cell_attach_binding` tests
cover dispatch, streaming, disabled references, prefetch and save restoration.
The installed `no_load_screen_live` comparison uses Game transitions because
the real host now resolves bindings through those calls before exposing events.
The repaired comparison queues 55 identical events on each path. Validation
passes 150 game library tests, 102 installed transition tests and 629 host tests,
including nine `cell_attach_binding` checks. Installed checks used test optimization
level 1 and the five-master load order.

Walking through a load door. `crates/game/src/world/transition.rs`.

A door is a placed reference whose base is a `DOOR`. What makes it a load
door is `XTEL`, 32 bytes confirmed against both
`TES5Edit/Core/wbDefinitionsTES5.pas` and `uesp-modfileformat/REFR.wikitext`:
the destination reference, then a position and a rotation to arrive at, then
a flag word. A door without it swings instead, and both kinds ship. The
`Teleport` view in `crates/esm` already read all of it, so nothing new was
parsed here.

`World::activate` already moved the activator. What it lacked was the world
changing around them, so references accumulated forever. Now a door the
player activates calls `change_cell`, which unloads the cell being left and
loads the one being entered. An actor walking through a door moves without
dragging the world after it.

### The unload rule

Vanilla can drop a temporary reference because a save writes change forms for
anything that moved. There is no save in the middle of a transition, so the
rule here is stricter and needs no bookkeeping: **a reference is dropped only
when rebuilding it from its record reproduces exactly the state it holds.**
`RefState` derives `PartialEq`, so this is a comparison rather than a guess,
and anything a script disabled, moved, unlocked or emptied fails it and stays
resident. Unloading can therefore never lose a change.

Four things are never dropped whatever the comparison says: the player, an
actor (its state lives in a second map, so dropping half would split it), a
dynamically placed reference (it has no record to rebuild from), and any
persistent child of the cell.

### Measured on the installed game

`tests/real_transition.rs`, gated on `SKYRIM_DATA`, walks out of the Bannered
Mare.

| | |
|---|---|
| References built for the cell | 697 in 0.54 ms |
| Load doors found in the cell | 2 |
| The move itself | 0.50 ms |
| References resident, before and after | 698, then 531 |

**A load door is a persistent child**, which the first version of this test
assumed the opposite of and failed on. It has to be persistent: the cell on
the far side names it in its own teleport data, so it must resolve whether or
not this cell is resident. The test now asserts that and says why.

### Determinism

Loading and unloading rewrites the reference set, so the resident cell is
hashed alongside it, in the `world.refs` part rather than a part of its own
because it describes that same set. `tests/determinism.rs` walks out and back
twice from one seed and compares every tick, in one process and across two.
The save carries the resident cell as well, so a loaded game knows what it is
holding.

`World::new_with_seed` derives independent xorshift streams for AI, behavior
trees, dialogue, mod data, actor templates, leveled items and leveled spells.
Those callers use `random_*_for`, so adding a draw in one system does not move
another system's sequence. `random_*` remains the general Papyrus stream.
SOEW version 37 stores the subsystem stream states, and `world.rng` hashes all
of them. Older world saves derive the new streams from their saved general RNG
state.

`TickHash::with_host_builder_hash` adds or replaces a `host.builders` part
and recomputes the total. A host caller can attach its per-tick builder result
hash to the game hash, then use the same comparison and text log format.

### Not done

The renderer half. Nothing tells the host to stream the new cell or drop the
old one, which is the `godot-host` side of the same item. Exterior cells load
as a single cell rather than a ring around the player, so walking out of a
door into Whiterun loads the one cell the door lands in. No load screen, no
fade, and no unloading by distance as the player walks.

### No loading screen: streaming and attaching are two different things

`crates/game/src/world/doorway.rs`, and the split in `transition.rs`.

The loading screen exists because building a cell and attaching it used to be
one call. They are not the same kind of work. Building is slow and no script
can observe it. Attaching is instant and is the entire script contract. Once
they are separate the screen has nothing to cover.

| | |
|---|---|
| `prefetch_cell` | Builds every reference the cell owns. Queues nothing, moves `loaded_cell` nowhere. |
| `attach_cell` | Fires the full event sequence and attaches. Leaves the old cell resident. |
| `settle_unload` | Drops the old cell, on whatever later frame suits. |
| `arm_doorway` | Reads the far side of a door, notes whether it is cold, prefetches it. |
| `commit_doorway` | Moves the player and attaches, once they cross. |

`change_cell` still does all three in one call and behaves exactly as it did,
so a script calling `Activate()` on a door gets the old synchronous move. The
split is for the player walking through under the host's control.

### The events, and where their order comes from

These did not exist before. Order and audience are read from the Creation Kit
wiki pages in `../engine-refs/ck-wiki/`, one page per event, not guessed:

1. `OnCellDetach`, to every scripted reference of the cell being left.
2. `OnUnload`, to the enabled ones.
3. `OnCellAttach`, to every scripted reference of the cell entered. The page
   states outright that this fires for disabled references and that it
   reliably precedes `OnCellLoad`.
4. `OnLoad`, to the enabled ones. Its page states it does **not** fire for
   disabled references, which is the asymmetry with `OnCellAttach`.
5. `OnCellLoad`, only when the destination was not already streamed.

Only references that already carry a VM object are told, which is both what
vanilla does and what stops a cell change allocating an object for each of a
cell's several hundred references.

The order between `OnCellDetach` and `OnUnload` is not stated on any page the
corpus carries. This takes the symmetric reading, cell event before 3D event
in both directions, and the test pins the choice rather than claiming it is
vanilla.

### The one flag that has to be carried, not recomputed

`OnCellLoad` fires only for a cell that was not already in memory. Read that
after a prefetch and the answer is always "in memory", so the event silently
stops firing and a mod that waits on it hangs. `change_cell_with` takes the
answer captured before the prefetch instead, and
`tests/no_load_screen.rs` has a test that does it the wrong way round on
purpose to keep the trap documented.

Residency is tracked as a set of streamed cells rather than asked of the
reference map. Those differ, and the difference was a real defect: any lookup
that resolves a form builds that one reference, so an unrelated
`object_of_form` marked the cell in memory and suppressed the event.
`ObjectReference.Is3DLoaded` uses that host-fed cell set for an instantiated
reference. It returns false for disabled references and after their cell
streams out, and does not depend on room-bound rendering culling.
This matches the local CK `Is3DLoaded_-_ObjectReference` page, which says a
reference in another room bound still reports loaded 3D when it otherwise
would have been rendered.
Cell attachment adds its reference IDs to a derived loaded-3D set, and cell
release removes those IDs. Prefetch alone does not mark 3D loaded. The set is
rebuilt from streaming events and is not saved.

`GetTriggerObjectCount` reads the actor occupants tracked for supported XPRM
box and sphere references. It returns zero for an empty supported trigger,
and logs an error and returns zero for a reference without a supported
trigger primitive. These behaviors use the trigger movement API described
below.

`IsFurnitureInUse` reads HST-09 reservations on any marker for the reference.
`IsFurnitureMarkerInUse` checks one zero-based marker index. Their optional
ignore-reservations arguments exclude the modeled reservations. HST-09 does
not yet track transient animation occupancy, so the queries cannot report an
occupant that has no saved seat reservation.
`GetCurrentDestructionStage` reads CMB-37 reference state and returns `-1`
before the first DEST threshold, then the zero-based DSTD stage. The SCR-38
probes exercise all four methods through Papyrus `CallMethod`.

### Exterior attachment deltas

`World::update_streaming_cells` changes the player focus while retaining neighboring
cells whose host resources remain loaded. It fires attach and load events only for
arriving cells, and detach and unload events only for departing cells. Invalid or
overlapping deltas fail before mutation. The host supplies each resource transition
once, separately from prefetching records.

`attach_streaming_cells` and `detach_streaming_cells` handle neighbors when a load
door replaces the host location. They preserve the focused cell. When the departed
door cell becomes a neighbor of the new location, its deferred unload completes
before it is attached again, so the next tick cannot drop its new resources.
Tests in `no_load_screen.rs` cover retained-cell events, invalid deltas, neighbor
validation and this short door move. Event audiences follow the local CK wiki's
`OnCellAttach` and `OnCellDetach` pages, as for the single-cell path.

### Door transitions measured on the installed game

`tests/no_load_screen_live.rs`, gated on `SKYRIM_DATA`, release build. The
budgets were written into the test before it was first run. A door animation
is roughly half a second, and the crossing has to fit in a frame.

| interior | refs | cold move | prefetch | commit | settle |
|---|---|---|---|---|---|
| WhiterunBanneredMare | 495 | 548 µs | 388 µs | **7.96 µs** | 420 µs |
| WhiterunJorrvaskr | 495 | 549 µs | 422 µs | **8.12 µs** | 433 µs |
| RiftenBeeandBarb | 280 | 406 µs | 310 µs | **5.25 µs** | 373 µs |
| WhiterunDragonsreach | 495 | 762 µs | 492 µs | **9.25 µs** | 768 µs |
| SolitudeBluePalace | 443 | 791 µs | 341 µs | **9.81 µs** | 1.17 ms |

The prefetch was only half the job, and measuring said so. The first version
of this left the unload on the crossing and the Bannered Mare still cost
464 µs, because proving a reference unchanged means rebuilding it from its
record and that is the expensive half of leaving a cell. Deferring the drop
took the worst crossing from 1.22 ms to 9.81 µs.

Whiterun's event list is 55 events and is identical whether the far side was
streamed early or not, which is the compatibility claim stated as a test.

### Not done here

The host half. `crates/godot-host` has no door handling at all yet, so
nothing drives `arm_doorway` from a key press or calls `settle_unload` on a
later frame. That work needs the activate binding from the `input` item and
would collide with it, so it is left rather than half done.

Cold textures are the case that can still overrun the animation window, and
they live in the host, so the fade fallback the plan asks for cannot be
measured from here. The numbers above are the record and reference side
alone.

`unload_cell` re-parsing every reference to prove it pristine is the reason
settling costs what it does. A dirty flag set at each mutation site would
make it a walk instead of a rebuild, and would help every unload rather than
only this path.

## Gift menu requests, UI-15

`Actor.ShowGiftMenu` queues one host request and suspends its Papyrus caller
until the menu closes. Giving moves items from the player to the actor, while
taking moves items from the actor to the player. The optional FormList filter
matches item forms and item keywords. Successful selections use the ordinary
notified inventory transaction. When favor points are enabled, the request
adds the selected item's authored value times the moved count. Value offsets
come from `TES5Edit/Core/wbDefinitionsTES5.pas` for WEAP, ARMO, MISC, INGR,
AMMO, BOOK and ALCH. Closing resumes the latent call with the nonnegative
accumulated value. A second request while one is pending returns zero. The local Creation Kit page
`ck-wiki/ShowGiftMenu_-_Actor.wikitext` supplies the direction, filter,
favor and latent-call contract.

## Crafting execution, first increment

`Game::check_craft` and `Game::craft` read the winning COBJ recipe, resolve its
links in that winning file's master order, match the placed workbench keyword
and evaluate conditions against current world and VM state. FURN WBDT must
select object creation. ACTI benches can provide a creation keyword. The
canonical armor and weapon improvement keywords cannot create new items.

Ingredient counts are aggregated with checked arithmetic. The transaction
requires the full input quantity even if an input is also the output. It
checks output capacity before committing the complete inventory once.
Rejected crafts preserve inventory quantities and queued events, although
lazy record reads can materialize other world state. A preview is rechecked
at execution. Distance, menu ownership and host activation remain caller work.

Successful transactions queue OnItemRemoved for consumed stacks and
OnItemAdded for the output. The next game pump executes handlers. Reference,
running quest alias and active-effect recipients targeting the changed holder
have independent filters, applied when events are queued. Filter lists inspect
immediate members only. Empty lists suppress events, and later filter changes
do not cancel pending calls. Effect expiry and dispel remove its filters before
later inventory changes are queued.

Successful COBJ recipes whose workbench keyword editor id starts with
`CraftingSmithing` also award Smithing skill use to the crafting actor. The
amount is `(3 * value^0.65 + 25) * created_count`, using the created item's
record value, then passed once through `World::advance_skill` so AVIF `AVSK`
use multipliers, offsets, the skill curve and saved partial experience apply.
This follows the UESP Smithing crafting formula and the COBJ output/count
fields in local xEdit. A rejected transaction and other workbench keywords
award no Smithing use. Outputs without a supported value field receive no
award. This transaction path does not dispatch Story Manager events.

The script-facing AddItem, RemoveItem and RemoveAllItems stack paths now
queue these events too. Silent removal still notifies scripts. RemoveAllItems
currently ignores `abKeepOwnership` and `abRemoveQuestItems`, so it does not
preserve ownership when transferring items or retain quest objects when that
flag is false. Quest-object extra data is not yet classified by this native.
AddItem expands
a FormList into one count change and OnItemAdded event for every supported
base form, matching the CK AddItem page. It flattens nested FormLists in
depth-first member order, aggregates duplicate members into one base-form event,
and rejects a cycle or a nesting depth above 64 before changing inventory. CK
does not specify nested AddItem semantics, so this matches the bounded recursive
FormList walk used by GetItemCount. CK FormLists cannot contain duplicates, and
OnItemAdded documents bulk additions as one queued call per base form. A
concrete-reference member follows direct AddItem's move-once rule. Repeated
references and a reference count other than one reject the whole call before
any base form changes. Each leveled-list member rolls with the receiver's
encounter level and player context. Its selected stacks retain COED metadata.
The combined transaction checks capacity for ordinary, selected and reference
additions before moving a reference or queueing events. It restores RNG when a
later malformed member, reference or capacity check fails. Preflight also checks
held reference identity against its materialized or authored state before an
earlier member can change inventory or queue events.
Active-effect recipients remain pending with magic execution.

The condition evaluator implements functions 47, 56, 58, 59, 72, 74, 136,
182, 244, 264, 277, 289, 359, 448, 543, 560, 629, 630, 659 and 716. This is a bounded set,
not the entire CTDA contract. It supports ordered AND/OR groups, numeric or
global comparisons, explicit references and caller-supplied quest aliases.
Reserved PlayerRef resolves to the runtime player even without a placed record.

The condition evaluator supports these implemented CTDA function IDs across the
integrated branches: 1, 14, 18, 46, 47, 53, 56, 58, 59, 67, 69, 70, 71, 72,
73, 74, 79, 80, 91, 98, 106, 107, 110, 111, 116, 123, 125, 127, 128, 136,
182, 185, 214, 219, 223, 225, 230, 248, 249, 250, 264, 277, 280, 286, 287,
289, 310, 313, 315, 318, 325, 327, 339, 359, 360, 378, 403, 408, 430, 434,
435, 444, 445, 446, 447, 448, 470, 491, 494, 500, 501, 528, 543, 550, 560,
561, 562, 563, 565, 566, 567, 568, 569, 570, 591, 592, 598, 600, 601, 603,
604, 605, 606, 610, 625, 626, 629, 630, 635, 636, 639, 640, 651, 655, 659,
660, 675, 683, 691, 692, 693, 696, 698, 699, 704, 706, 711, 713, 718, 721,
723, 724, 730, 731, 734 and 735. This is a bounded set, not the full CTDA
contract. Evaluation supports ordered AND/OR groups, numeric or global
comparisons, explicit references and caller-supplied quest aliases.
The integrated condition evaluator implements these CTDA function IDs across its branches:
1, 14, 18, 24, 35, 45, 46, 47, 50, 53, 56, 58, 59, 61
67, 69, 70, 71, 72, 73, 74, 77, 79, 80, 91, 98, 99, 106
107, 110, 111, 116, 123, 125, 127, 128, 136, 142, 182, 185, 214, 219
223, 225, 230, 248, 249, 250, 263, 264, 277, 280, 286, 287, 289, 306
310, 312, 313, 314, 315, 318, 325, 327, 329, 339, 353, 354, 359, 360
362, 372, 378, 398, 403, 408, 414, 426, 430, 432, 434, 435, 444, 445
446, 447, 448, 465, 470, 491, 494, 500, 501, 519, 528, 543, 550, 560
561, 562, 563, 565, 566, 567, 568, 569, 570, 591, 592, 597, 598, 600
601, 603, 604, 605, 606, 610, 624, 625, 626, 629, 630, 635, 636, 639
640, 650, 651, 655, 659, 660, 675, 680, 682, 683, 691, 692, 693, 696
698, 699, 700, 704, 706, 711, 713, 718, 720, 721, 722, 723, 724, 725
726, 730, 731, 734, 735. This is a bounded set, not the full CTDA contract. Evaluation supports ordered AND/OR groups, numeric or global comparisons, explicit references and caller-supplied quest aliases. Reserved PlayerRef resolves to the runtime player even without a placed record.

GetScale reads reference scale with the race and NPC height
multipliers for actors, and GetDisabled reads reference state. GetDetected
checks whether its actor argument has current Combat-level awareness of the
subject, and GetAlarmed reads the subject actor's alert flag. GetRandomPercent
draws an integer from 0 to 99 from the world's saved random stream. Functions
27, 36 and 39 return an explicit unavailable-contract error because the local
CK corpus does not define their runtime behavior. Functions 101
(`IsWeaponMagicOut`) and 552 (`GetSpellUsageNum`) return typed
contract diagnostics because local xEdit and CK sources name their parameters
but do not define a usable runtime result. Functions 644, 684, 714 and 730
report unavailable runtime state. Functions 683, 698 and 704 read current
actor package and flying state. Reserved PlayerRef resolves to the runtime
player even without a placed record.

New runtime players retain reserved FormID 00000014 and use the Player NPC
base when present, so forced player aliases and inventory notifications agree.
Conditional Papyrus variables for script and quest variable conditions come
from current attached instances, with ambiguous or nonnumeric values reported
as errors. Destruction stage defaults to -1 and is saved per reference.
Critical-stage conditions match the actor's current critical-stage value.
GetEventData, GetIsCrashLandRequest and GetIsHastyLandRequest have no usable
condition contract in the local CK sources and return unsupported-function
diagnostics. xEdit supplies their parameter layouts but not result semantics.
SOEW version 37 stores destruction-stage state in the `CMB-37` reference
section. Older saves without the section load references at stage -1. These
parameter slots follow xEdit's `wbDefinitionsTES5.pas`. The stage rules follow
CK's `GetCurrentDestructionStage` and `SetCriticalStage` pages. The local CK
corpus does not define usable result contracts for event data or either landing
request query, so those functions return diagnostics. Unavailable targets,
unsupported flags/functions, nonzero HasPerk parameter 2 and missing tempering
context are explicit errors. Crafting does not invent target, quest, skill-use
or light-level context.

`GetPathingCurrentSpeed` reads the actor package speed while waypoints remain
and the actor has not arrived. `IsPathing` uses that same pending waypoint
state. `IsAllowedToFly` reads the actor's `SetAllowFlying` flag. `IsBeingRidden`
reads the mount actor flag. `World::set_mount_rider` updates that flag together
with the rider's `riding_mount` flag when gameplay reports a mount or dismount.
`IsInFurnitureState`, `GetPathingCurrentSpeedAngle` and `IsOnFlyingMount` return
an unavailable-state diagnostic. xEdit supplies their function and parameter
definitions, but the engine has no furniture animation, axis-angle or flying
mount state. CK semantic pages for those three functions were absent from the
local corpus, so their numeric results are not inferred from the function names.
These conditions are not included in `supports_function`.
Float comparisons are exact and have not been compared against vanilla
near-equality behavior. `ObjectReference.GetItemCount` totals an exact base
form, each FormList member or every carried item whose base has the supplied
keyword. An item reference selects only stacks whose ExtraOriginalReference
identity matches it, ignores nonpositive leveled bookkeeping and cannot exceed
the base form's canonical total. Cyclic FormLists return zero from the native
and an explicit condition error. The CK page records keyword failures on actor
inventories and CK-defined reference inventories. The engine uses its shared
inventory totals for both actors and containers, so keyword queries have the
same defined result for either holder.

Condition functions 244 and 716 implement `GetRestrained` and
`GetRealHoursPassed`. `GetRestrained` reads actor state, rejects either parameter,
and reports false when an actor is unconscious because the CK's `SetRestrained`
contract excludes the restrained and unconscious states. `GetRealHoursPassed`
returns `GameClock.real_seconds` in hours and rejects either parameter. xEdit's
`wbConditionFunctions` declares both functions parameterless. The UESP raw function
table describes real hours as elapsed real-life play time and identifies
`IsWaterObject` as a drinking-animation query. Function 304 checks whether the
tested reference's base record is `WATR`. No usable local CK behavior contract
was found for functions 255, 265, 266, 267 or 332. Those function IDs remain
unsupported and return `conditions::Error::Function`, rather than inventing vendor
or weather state. Sources: `TES5Edit/Core/wbDefinitionsTES5.pas`,
`uesp-modfileformat/Raw_Function_Data.wikitext`, CK
`SetRestrained_-_Actor.wikitext`, `SetUnconscious_-_Actor.wikitext` and
`Game_Script.wikitext`.

`HasMagicEffect` matches an active MGEF on its subject. `IsSpellTarget` matches
an active application from its magic item parameter. `HasShout` checks known
shout membership. `HasEquippedSpell` accepts casting source 0 for left, 1 for
right and 2 for voice. Alchemy entry-point checks read transient caller context:
poison selection and the MGEF currently checked for keyword membership. The
brewing path supplies potion mode and each shared effect while checking its
ingredient conditions. Other callers receive a typed missing-context error.
Complex perk entry-point dispatch accepts the same alchemy context and passes
it to each tab condition.
The spell-effect and alchemy behavior follows xEdit
`wbDefinitionsTES5.pas` parameter metadata. CK pages for these CTDA functions
are absent from the local `ck-wiki` corpus.

Sources consulted were local xEdit `wbDefinitionsTES5.pas` for CTDA, including
the six AI-37 function identities and parameter types, CK `GetActorValuePercentage`,
`GetLightLevel` and `IsOverEncumbered` pages, and Mutagen's `PlayerAction.cs`
for the skill action numbers. No CK page defines GetActorWarmth's condition
contract. The evaluator uses caller-supplied light and skill-use context because
the runtime does not calculate either value.

`CanFlyHere` (731) follows the CK no-argument actor query and CommonLib's
`Actor::CanFlyHere`: it is true when the actor's current worldspace has `MHDT`
max-height data. Missing, deleted, non-actor and cell-less subjects return a
form error. A nonzero unused parameter is rejected. The worldspace `MHDT`
layout is documented in UESP `WRLD.wikitext`.
xEdit and CommonLib name `IsFlyingMountPatrolQueud` (732) and
`IsFlyingMountFastTravelling` (733), but the local CK pages do not define their
behavior. They remain unsupported and return an explicit function error. The
local sources used were CK `CanFlyHere_-_Actor.wikitext`, CommonLib
`src/RE/A/Actor.cpp` and UESP `WRLD.wikitext`. `ai_68_flying_conditions` covers positive
and negative CanFlyHere comparisons, invalid actor context and explicit
diagnostics for both undocumented mount queries. It does not use installed
Skyrim records.

AI-55 adds `GetInCellParam`, `GetLocationCleared`, `IsCellOwner`,
`GetInWorldspace`, `GetWithinPackageLocation`, `GetInCurrentLocAlias` and
`GetInCurrentLocFormList`. `GetInCellParam` reads the explicit reference
parameter's current cell. Worldspace tests read the subject's current reference
placement. Cell ownership compares the cell's explicit `XOWN` owner,
and a zero owner parameter matches an unowned cell. Location tests include the
named location and its descendants. Location aliases must be filled `ALLS`
aliases. Form lists match any listed location and its descendants. Package
location tests require an active package and its location input. `InCell`
inputs compare cell identity, and other location inputs use their package
radius around the resolved destination. Single-parameter functions reject a
nonzero second parameter. `GetHitLocation` remains an explicit
unsupported-function diagnostic because the local CK corpus supplies no usable
result contract. xEdit `wbConditionFunctions` supplies each function's
parameter types. Related CK behavior comes from `IsInLocation`,
`GetWorldSpace`, and `GetActorOwner - Cell`.

AI-56 adds `GetInZone`, `GetVelocity`, `GetMapMarkerVisible`,
`GetLockIsBroken`, `HasRefType`, `LocationHasKeyword`, `LocationHasRefType` and
`GetIsEditorLocation`. Encounter-zone checks compare the subject's resolved
zone with an `ECZN` parameter. Map-marker visibility reads the reference's
`AddToMap` state. Reference-type checks read `XLRT`, and location ref-type checks
read the location's added and master special-reference lists. Location keyword
checks use the subject's current location. Editor-location checks use the
reference's authored location, including its cell fallback. `GetVelocity`
validates its axis parameter and `GetLockIsBroken` validates its unused
parameters, then both report unavailable runtime state because the world does
not model velocity or broken locks. xEdit defines the parameter types. CK's
`HasRefType` and `IsMapMarkerVisible` pages supply the available query
contracts. CK's `HasKeyword`, `GetEditorLocation`, `Lock` and `IsLocked` pages
describe the related form queries and state. The local CK corpus has no usable
individual condition contract for the other functions.

AI-57 adds `GetIsEditorLocAlias`, `IsBlocking`, `GetRefTypeDeadCount`,
`GetRefTypeAliveCount`, `GetLocationAliasCleared`,
`GetLocAliasRefTypeDeadCount`, `GetLocAliasRefTypeAliveCount` and
`IsInSameCurrentLocAsRef`. Editor-location alias tests compare the subject's
authored location with a filled location alias. Blocking reads the actor's
current host-reported blocking flag. Ref-type counts read matching entries in
the location's added and master special-reference lists, then count actor
references by their current dead state, excluding references in the location's
removal list. Alias count and cleared checks require
a filled location alias. Same-current-location checks compare both references'
current locations and require the supplied keyword on that location. xEdit's
condition table and the local UESP raw function table provide names, parameter
types and signatures. The CK corpus has no individual condition pages for this
batch, so the result behavior follows the function names and the referenced
LCTN and live actor state models. Missing aliases and current locations return
explicit context errors.

AI-58 adds `IsInSameCurrentLocAsRefAlias`, `LocAliasIsLocation`,
`LocAliasHasKeyword`, `GetKeywordDataForCurrentLocation` and
`IsUnlockedDoor`. The alias location predicates require a filled alias of the
declared kind. Same-current-location compares the subject and filled reference
alias locations, then checks the keyword on that location. Location keyword
data reads the subject's current location and returns zero for an unset key.
`IsUnlockedDoor` reads the subject reference's lock bit and requires a `DOOR`
base. `IsLocationLoaded` and `IsLocAliasLoaded` validate their location alias
inputs, then report unavailable because local CK condition sources define no
usable loaded-state result contract and the world does not model loaded state
for locations or aliases. xEdit `wbConditionFunctions` supplies IDs and
parameter types, and the local UESP raw function table supplies the matching
Papyrus signatures. The CK corpus has no individual condition pages for this
batch.
`IsInList` tests whether the subject reference's base form is a member of the
resolved FLST. `Exists` accepts a placed ACHR or REFR and returns false for a
deleted reference. `GetIsVoiceType` compares the actor's traits-source NPC_ VTCK
with a resolved VTYP. `GetIsObjectType` compares the subject base record's
CommonLib form-type code. `IsPC1stPerson` reads the host POV state, while
`IsPlayerGrabbedRef` compares its resolved REFR parameter with the grabbed
reference. Group member and target counts walk live actor combat-target links:
actors are grouped when they target the same combatant or target another member.
An actor without a combat target has zero group counts. These transient values
are not saved. Local xEdit `wbDefinitionsTES5.pas` and UESP
`Raw_Function_Data.wikitext` supply the function indices and parameter types.
Mutagen's Skyrim condition data types confirm the typed FLST, placed-reference,
VTYP and form-type links. The CK corpus has no individual condition pages for
these functions. UESP describes 391 as the active player POV query. CK's
`OnGrab_-_ObjectReference.wikitext` describes the player-grab event, but not
the condition result. The runtime reads its POV and grab state and derives
group counts from shared combat-target state.

AI-44 implements crime CTDA functions 193, 375, 376 and 402. `GetPCExpelled`
reads the referenced faction's player-expelled flag. The crime-gold functions
read that faction's violent or nonviolent bounty, and accept a zero faction
parameter as the subject's crime faction. `IsBribedbyPlayer` reads the
subject's `LastBribedIntimidated` actor value, the state currently shared with
intimidation. The xEdit function table defines the form and unused-parameter
slots. The CK `SetBribed` page defines the actor outcome, and the evaluator
rejects nonzero unused parameters and unavailable actor or faction context.
Functions 195, 282, 366 and 373 return `conditions::Error::Contract`: the CK
corpus has no usable query contract for faction-murder counts, stolen-horse
state or stolen-item value, and the runtime does not store those totals or
flags. They are not inferred from bounty or inventory totals. See
`crates/game/tests/ai_44_crime_conditions.rs` for positive, negative and
invalid-context fixtures.
AI-50 adds `IsWeaponOut` from actor draw state, `IsActorUsingATorch` from an
equipped carryable `LIGH`, `IsWeaponInList` from assigned weapon hands,
`GetKeywordItemCount` from the shared inventory totals, `GetEquippedItemType`
from the CK hand values, and `GetInContainer` from an item's base-form count in
the referenced container. The CK `GetEquippedItemType_-_Actor.wikitext` page
defines hand arguments 0 and 1 and return values 0 through 12. xEdit's
`wbDefinitionsTES5.pas` supplies the CTDA function ids and parameter types.
The CK corpus has no usable semantic contract for conditions 479
`GetIsUsedItemEquipType` and 487 `IsCarryable`, so evaluation returns a typed
condition contract diagnostic for those ids. AI-50 fixtures cover true, false,
and invalid form or hand contexts for the implemented functions, and the
diagnostic paths for both source gaps.

AI-74 adds `GetPCMiscStat` (312), `IsActorAVictim` (314), `IsFleeing` (329),
`IsActor` (353), `IsEssential` (354) and `HasLinkedRef` (362). Misc-stat
parameter values and names follow xEdit's `wbMiscStatEnum`, and values read
the same tracked-stat table as `Game.QueryStat`. Victim state uses the saved
`witnessed_crimes` relation: an actor is reported as a victim when any actor
has recorded witnessing a crime by that actor. Fleeing reads the active package
procedure, essential reads live actor state, and linked-reference checks match
the requested keyword. The xEdit and UESP tables identify 367 and 368 as
player-action queries, but the local CK references do not define their result
or state semantics, so evaluation returns an explicit unsupported-function
error for both.

AI-74 also consults local xEdit `TES5Edit/Core/wbDefinitionsTES5.pas` for the
condition function IDs, parameter types, miscellaneous-stat enum and player
action enum, UESP `Raw_Function_Data.wikitext` for function names, and CK
`ActorBase_Script.wikitext`, `Actor_Script.wikitext` and
`ObjectReference_Script.wikitext` for the related essential and linked-ref
contracts. Existing witnessed-crime state comes from
`World::record_witnessed_crime`.

The later condition batch checks `IsLinkedTo` against the subject reference's
linked-reference keyword pairs, `WornHasKeyword` and
`WornApparelHasKeywordCount` against worn ARMO forms, `IsCommandedActor` against
live command effects. `DoesNotExist` is supported for declared quest aliases
run on that alias and returns true when it is unfilled. The batch returns
`conditions::Error::Contract` for `GetActivatorHeight` and
`GetKnockedStateEnum`, since the local CK corpus provides no usable runtime
contract or state representation for them. `GetTargetHeight` returns the
signed Z difference from the subject reference to its parameter reference,
as defined by UESP's local raw function table. Invalid actors, forms,
parameters, and undeclared aliases return explicit errors. xEdit's local
`wbDefinitionsTES5.pas` supplies the function IDs and parameter types. CK's
`IsCommandedActor_-_Actor.wikitext`, `WornHasKeyword_-_Actor.wikitext` and
`GetLinkedRef_-_ObjectReference.wikitext` supply the behavior used for those
queries. The other condition IDs appear in xEdit's function table, but that
schema does not establish their runtime behavior.

Sources consulted were local xEdit `wbDefinitionsTES5.pas` for CTDA, COBJ and
FURN WBDT, CK `GetItemCount_-_ObjectReference.wikitext` for the query forms,
UESP COBJ notes for missing outputs, skymp `CraftService.cpp` and
`unit/CraftTest.cpp` for bench matching and DLC cases, Mutagen
`Skyrim/Constants.cs` for PlayerRef, and CK inventory event/filter pages.
CRF-08 also uses CK `Game_Script.wikitext` for `AdvanceSkill`, CK
`Quest_Script.wikitext` for the separate `OnStoryCraftItem` event contract,
and xEdit COBJ output/count fields. The Smithing experience equation follows
the UESP Skyrim Smithing crafting section.

For AI-67, xEdit `wbConditionFunctions` supplies the function indexes and
parameter types. The local CK corpus supplies
`IsBeingRidden_-_Actor.wikitext`, which defines it as a zero-parameter actor
query. `IsBeingRidden` (714) reads `ActorState::being_ridden`. The named
`AI-67` actor save section retains that flag. Gameplay reports mount and
dismount transitions through `World::set_mount_rider`, which updates the rider
and mount flags together. GetGraphVariableInt (675) reads
a named integer in the actor's live graph state, case-insensitively, and
reports missing names and non-actor subjects. Its integer state is transient
host input and is not saved.
`IsInFurnitureState` (644), `GetPathingCurrentSpeedAngle` (684), and
`IsOnFlyingMount` (730) remain unavailable. xEdit supplies their condition
parameter types, but the local CK corpus has no usable result contract for
these condition functions, and the current world has no matching animation,
axis-angle or flying-mount state.
The skymp evaluator's unknown-function success and unresolved target mapping
were not adopted. Mutagen labels HasPerk's second integer unused, while xEdit
exposes it, so nonzero values need further evidence. Caprica's property
compiler confirms conditional auto-property flags on the backing variable.

AI-65 implements CTDA 447, `GetGraphVariableFloat`, from the actor's
behavior-graph float map. The condition's `CIS1` name is matched without case
sensitivity. Missing names, non-actor subjects and an unexpected second
parameter return explicit errors. CTDA 568 reads the transient sprint state
set by the host movement driver when sprint input and movement are active and
the player is not sneaking. CTDA 594, `GetIsFlying`, reads a transient actor
flight state supplied by runtime movement. It does not use `allow_flying`,
which records permission to fly rather than current flight.

CTDA 358, `IsPlayerMovingIntoNewSpace`, 450, `IsLastIdlePlayed`, 584,
`GetRelativeAngle`, 589, `GetMovementDirection`, and 613,
`IsFurnitureAnimType`, return `UnavailableContract`. Local xEdit
`wbDefinitionsTES5.pas` supplies their identities and parameter types. The
local CK function corpus has usable state-query descriptions for sprinting and
flying, but no usable behavior contract for the other five functions. Tests
exercise graph float, sprint and flight comparisons, malformed parameters,
missing graph variables, invalid actor subjects and explicit diagnostics.

The AI driver's `Hover` procedure signals the actor's transient flying flag
while it moves to and holds its authored elevated location. Ending Hover or
stopping actor movement clears the flag. CTDA 594 reads this movement state.
`allow_flying` remains a separate permission flag. Other actor flight modes
and host-controlled flight are not represented by this path.

Twelve synthetic crafting scenarios cover duplicate inputs, gross versus net
counts, overflow, bench modes, winning overrides, condition rechecks,
conditional quest variables, alias filters and script callbacks. Two Papyrus
tests cover conditional flags, inheritance, ambiguity and saved variables.
An engine save made before event dispatch resumes to the same inventory,
handler variables and canonical state hash. Save bytes are not required to
match because existing world hash maps serialize in arbitrary order.

The installed five-master run surveyed 1,385 recipes and executed
RecipeWeaponIronDagger and BYOHRecipeNails at forge reference 0001AD6E.
The dagger consumed one iron ingot and one leather strip. Nails consumed one
iron ingot and produced ten nails. Explicit PlayerRef inventory conditions
also passed. Predicate presence across the survey does not establish that
every recipe is executable or that all predicate contexts match vanilla.

`Game::temper` executes COBJ improvement recipes at grindstones and workbenches.
The recipe output must be a weapon or armor form also listed as one input
instance. Other COBJ inputs are consumed together with that instance quality
update, and removal events are queued only after the inventory transaction
commits. The item keeps its other extra data. Quality is selected from the
actor's Smithing value, with a minimum increase from 1.0 and a 1.6 cap. Recipes
must use the improvement bench keyword matching their station. The CK
`SetItemHealthPercent` page describes 1.0 as untempered, values below 1.0 as
having no effect, and 1.6 as appearing legendary. The engine writes only
tempering qualities at or above 1.0. It does not implement perk or equipment
bonuses to the quality calculation.

The local xEdit `wbRecord(COBJ)` definition establishes the recipe fields, and
the UESP COBJ description states that the bench keyword distinguishes item
creation from improvement. CK `SetItemHealthPercent` and
`ObjectReference_Script` describe the quality range used above. The xEdit
definitions contain no `SMIT` record declaration.

Alchemy combines exactly two INGR forms at a FURN alchemy station. It selects
shared effects whose ingredient-level CTDAs pass, creates a dynamic ALCH
inventory form with the first ingredient's effect data, and consumes one of
each input in the same inventory transaction. Brewed potion definitions and
resolved MGEF links persist in a named world save section. A CK wiki Ingredient
page is absent. The shipped `Ingredient.psc` defines `LearnEffect`,
`LearnNextEffect` and `LearnAllEffects`, which remain outside this transaction.
Ingredient and effect layouts follow xEdit `wbDefinitionsTES5.pas` and UESP
ALCH/INGR format pages. CommonLibSSE-NG `TESFurniture.h` defines WBDT value 5
as alchemy, value 4 as enchanting experiment, and value 6 as alchemy experiment.

The tempering station check follows xEdit's `wbDefinitionsTES5.pas` WBDT
enumeration: value 2 is weapon smithing and value 7 is armor smithing. The
station keyword must match that type. Enchanting reads the ENCH ENIT enchant
type at bytes 20 to 23, as xEdit defines it. Type 0x06 is ordinary enchanting
and type 0x0C is staff enchanting. Staff enchantments can be applied to weapon
forms, while armor accepts only ordinary enchantments.

Enchanting, skill advancement, item instance state and host menus remain
unfinished. The full requested execution scope is tracked in
[execution-plan.md](execution-plan.md).

## Story Manager quest routing

`Game::send_story_event` accepts a typed story event and routes it through
winning `SMEN`, `SMBN` and `SMQN` records. Parent links and previous-node links
determine the authored tree and sibling order. Node conditions use the game
condition evaluator. Random nodes draw from the world's saved random stream.
Quest nodes read their `NNAM`, `FNAM` and `RNAM` quest entries, respect their
concurrent-quest limit, `MNAM` run count, run-once quest flag, reset deadline,
and do-all-before-repeating cursor. Nodes with the warning flag append a
notification when none of their child quests starts.

A started quest records the event code and ordered arguments in its existing
quest run data, starts through the normal `Quest.Start` path, then queues its
mapped `OnStory...` event after `OnInit`. `PRG-14` world save state retains
per-quest start counts and reset deadlines plus the selected quest set for
each do-all quest node. The implementation reads record fields described in
`TES5Edit/Core/wbDefinitionsTES5.pas` and event signatures listed in
`ck-wiki/Quest_Script.wikitext` and node field descriptions in
`uesp-modfileformat/SMEN.wikitext`, `SMBN.wikitext` and `SMQN.wikitext` under
`../engine-refs`. This state is included in the quest part of the world
determinism hash.

`Game::send_story_event` is an explicit runtime API. Papyrus
`Keyword.SendStoryEvent` queues a `SCPT` event with the keyword, location, two
references and two integer arguments in CK order. The game scheduler routes it
after the native returns. `SendStoryEventAndWait` routes the same event before
returning whether a quest started.
Gameplay actions do not generally emit story events automatically. The AI-69
draft was not present in this worktree. Its condition functions
`GetScriptVariable`, `GetQuestVariable`,
`GetDestructionStage`, `IsInCriticalStage`, `GetEventData`,
`GetIsCrashLandRequest` and `GetIsHastyLandRequest` remain a dependency of
conditioned trees. Unsupported or unavailable predicates return a typed
routing error. Unknown event codes queue `OnStoryEvent` because this branch
has no authored Papyrus event name for them.

Successful `Game::craft` transactions enqueue one `CraftItem` Story Manager
input with bench, current location and created item arguments. Rejected
transactions enqueue none. The next `Game::pump` converts each queued crafting
input to the `CRFT` event and routes it through the authored Story Manager tree.
The event carries the CK argument kinds for bench, location and created item,
in that order. A routed quest retains the event and queues `OnStoryCraftItem`
after `OnInit`. Routing errors are written to the VM log after the committed
craft remains successful. `World::take_story_manager_events` transfers queued
inputs out of the world for an external scheduler instead. Other gameplay
actions still use the explicit Story Manager API. The COBJ recipe fields
follow `TES5Edit/Core/wbDefinitionsTES5.pas`. The event name and argument
signature follow `ck-wiki/Quest_Script.wikitext` in the local reference corpus.

Tempering, alchemy, enchanting, skill advancement, item instance state and host
menus remain unfinished. The full requested execution scope is tracked in
[execution-plan.md](execution-plan.md).

`Keyword.SendStoryEvent` and `SendStoryEventAndWait` submit the `SCPT` event
with the keyword, location, two references and two integer arguments in CK
order, using reference kinds 1 for the keyword and 2 for the location.
The event code follows local `uesp-modfileformat/SMEN.wikitext`, and the native
return contract follows `ck-wiki/Keyword_Script.wikitext`. The waiting native
evaluates direct world conditions without Papyrus variable lookup. Conditions
that require Papyrus state return a routing error, and the waiting native
logs it and returns false. Deferred routing errors retain the
`Keyword.SendStoryEvent` caller name in the VM log.

The game crate now directly uses the workspace's existing thiserror 2.0.20
for typed execution errors. `cargo tree -p soe-game -i thiserror` confirms
the dependency was already present through esm, bsa, navmesh and profile.
No package versions or new transitive dependencies were added.

## Dialogue execution, 2026-09-20

`Game::begin_dialogue`, `dialogue_choices`, `choose_dialogue`,
`dialogue_line`, `advance_dialogue` and `close_dialogue` provide a conversation
lifecycle over winning DIAL, INFO, DLBR and QUST records. Normal top-level
branches supply the initial choices. An eligible blocking branch, with or
without Top-Level, suppresses normal topics. An active exclusive branch enters
the same blocking arbitration. The highest quest priority wins between
blockers, and an equal priority uses the resolved DLBR FormID so the result
stays deterministic. Quest priority and topic priority sort non-blocking choices.
Inactive quests and failed quest/INFO conditions suppress them.
Selection checks conditions again, so a stale menu cannot start its fragment.
Unknown conditions return diagnostics instead of becoming true.

`Topic.Add()` records a valid DIAL in the world-level known-topic set. The
contract follows `engine-refs/skse64/scripts/vanilla/Topic.psc`, since the local
CK corpus has no `Topic.Add` function page. During a conversation, known topics
are considered alongside authored top-level branch topics and pass through the
same owning-quest, INFO and condition checks. `World::add_dialogue_topic`
reports missing, deleted, malformed or unowned topics as typed errors. The
void Papyrus native returns `None` and logs a typed API error for an invalid
self form. The known-topic set is saved in the `SCR-48` world section.

INFO ownership comes from the winning TopicChildren group, then the owning
DIAL's QNAM. TPIC is a previous-topic link. PNAM determines INFO ordering,
including deleted anchors and inserted overrides. Shared INFO supplies response
data without transferring conditions, fragment scripts or quest ownership.
TCLT supplies follow-up DIAL links. Goodbye closes after response completion.
Random groups draw from the saved world RNG only during selection. Say Once
and reset hours use saved history, with reset time measured in game hours.
Do All Before Repeating is the first byte of the DIAL `DATA` struct. xEdit's
`wbRecord(DIAL)` declares it as a boolean there, CommonLib names the same bit
`DIALOGUE_DATA::TopicFlag::kDoAllBeforeRepeating` at `1 << 0`, and UESP's
`DIAL.wikitext` agrees on the position. UESP reports the byte only on Custom
and Scene topics and names the behaviour for Scene alone, so the Custom
subtype's meaning is not established by any of the three. This engine applies
the same rule wherever the byte is set, which is what xEdit and CommonLib
describe, and no capture from the game contradicts it.

When no accepted INFO in a topic passes its conditions, the selector can use
an eligible INFO marked Info Refusal (INFO response flag bit 4). The refusal
INFO's own conditions still apply. It is a fallback and does not replace an
accepted INFO whose conditions pass.

When an INFO has Invisible Continue (response flag bit 6) and its response
finishes, the conversation automatically selects the first ranked eligible
linked topic and starts its response. Conditions and other INFO eligibility
rules still apply. The host receives the next response directly, without a
choice prompt. Each INFO's begin and end fragments run once for its own
selection and completion. The flag and linked-topic fields follow xEdit
`wbDefinitionsTES5.pas` and UESP `INFO.wikitext`. The available CK dialogue
pages document dialogue target state, but not this continuation lifecycle.

When a DIAL sets Do All Before Repeating, selection first applies those INFO
eligibility rules, then suppresses the INFO records already selected for that
DIAL's current cycle. When every currently eligible INFO has been selected,
the next committed choice clears that DIAL cycle and starts another. This
history is shared by all speakers and listeners because the setting belongs to
the DIAL record.
Original-engine comparison of random-group selection remains open.

INFO VMAD begin/end fragments run on their named script attachment through
`Vm::call_script`. Both methods are checked before starting a response. The
speaker reference is the fragment argument. Begin runs before the first line,
and end is dispatched after the host acknowledges the final response or a
range exit interrupts the INFO. Explicit `close_dialogue` cancellation does
not dispatch its end fragment. A
failed end dispatch retains the current response cursor. The VM remains
responsible for scheduling latent calls and logging runtime script failures.
The host must not interpret successful dispatch as successful quest completion.

`TopicInfo.GetOwningQuest`, `Actor.GetDialogueTarget`,
`ObjectReference.IsInDialogueWithPlayer` and `Game.GetDialogueTarget` read this
state. The actor getter retains the documented player exception. PlayerRef
initialization reuses the canonical 0:000014 change from the crafting and host
worktrees, including NPC Player base data when present.

The shared condition evaluator reuses the crafting implementation and adds
speaker/listener and owning-quest context. Supported function IDs are 1, 18,
46, 47, 56, 58, 59, 67, 69, 70, 71, 72, 73, 74, 80, 136, 182, 264, 277, 289, 359,
99, 142, 159, 162, 163, 175, 403, 448, 543, 560, 566, 580, 606, 629, 630,
659 and 694. Function 99
measures the signed horizontal angle from a placed reference's heading to the
target reference, with clockwise angles positive and counterclockwise angles
negative. Function 142 reads the race's resolved walk movement speed
and falls back to the package driver's walk speed when race movement data is
absent. Function 159 checks whether the subject has reserved a Sit marker.
Functions 162 and 163 compare that reservation with a furniture reference or
FURN base. Function 175 checks whether the player has reserved a Sleep marker.
The furniture predicates use shared occupied-marker state. Function 112
(`IsIdlePlaying`) and 147 (`GetWindSpeed`) return typed unsupported function
errors because the runtime has no idle playback or weather wind state. The xEdit
table and UESP `Raw_Function_Data.wikitext` establish these function identities
and parameters. CK's `GetHeadingAngle` and `GetSitState` pages define the
heading and sit-state contracts. The local CK dump does not define usable
behavior for idle playback or wind speed. Run-on subject, target,
explicit reference and quest alias are supported when their context exists.
Conditions use current shared actor, quest, inventory and location state.
Papyrus variable queries require conditional flags and reject ambiguous names.
The combat predicates 672, 673, 674, 678 and 697 return `UnavailableState`
diagnostics because their attack-state producers or usable condition contracts
are absent. The evaluator does not complete missing actor-value, alias or
package systems.

AI-40 follows local `TES5Edit/Core/wbDefinitionsTES5.pas` and
`uesp-modfileformat/Raw_Function_Data.wikitext` for condition ids and parameter
types. CK `IsBleedingOut` and `GetNoBleedoutRecovery` define their actor flags.
The CK corpus has no usable contract for the attack-state predicates or
`IsStaggered`. Stagger uses the existing `World::is_staggered` deadline from
CMB-14. Unsupported attack-state reads diagnose instead of inferring a result
from melee contacts, actor values or target selection.

SOEW version 28 stores the active do-all DIAL cycles. SOEW version 24 stores
an actor's active exclusive DLBR. Speaking any topic
owned by an exclusive branch retains that branch after the conversation
closes. It enters blocking arbitration on the next root choices, including
when the prior INFO has no links. Speaking a topic from another exclusive
branch updates the retained branch. Speaking a topic from a non-exclusive
branch clears it. An unbranched topic does not clear it. Unknown DLBR flag bits
are rejected. Version 15 and older saves load with no retained exclusive branch.
SOEW version 9 stores the conversation cursor, sorted INFO history and active
magic effects alongside actor-value components. Formats without dialogue load
an empty conversation state. Dialogue state contributes to the
simulation hash. The derived record index is rebuilt from the load order.
This is engine-save support, not vanilla ESS conversation restoration.

Sources are local xEdit DIAL/INFO/DLBR/QUST and wbConditionFunctions, UESP
DLBR and VMAD_Field, CommonLibSSE-NG `BGSDialogueBranch` and
`TESObjectREFR` exclusive-branch accessors, CK GetDialogueTarget and
IsInDialogueWithPlayer, shipped
Location.psc, and Mutagen's DialogTopic reader. The PNAM ordering algorithm
adapts GPL Wrye Bash `Mopy/bash/brec/record_groups.py`,
`_DialChildren._sort_group`, with explicit cycle rejection. It remains in the
GPL game crate. The schema sources do not provide a reusable Rust interpreter.
The archived Creation Kit Dialogue Branch lifecycle page could not be fetched
again on 2026-09-20. The blocking and exclusive lifecycle above follows the
prior successful retrieval report for oldid 33340, while xEdit, UESP and
CommonLib independently establish the record flags and actor-scoped exclusive
branch state.

The installed five-master survey sees 3,933 non-deleted DLBR winners: flags
0 (224), 1 (2,708), 2 (970), 4 (13), 5 (1) and 6 (17). This confirms that
blocking without Top-Level and exclusive combinations occur in shipped data.

The gated five-master test checks all 40,678 live INFO records for ownership,
7,799 fragment declarations and 19,368 ordered topic groups. INFO fragment
bytes survive re-encoding exactly. Script headers follow the existing writer's
version 5/object format 2 normalization. The survey also exercises an authored
Hulda topic through the conversation API. That topic has no VMAD fragment,
so this run does not verify shipped fragment execution. Synthetic tests
verify named fragments changing quest stages. DLG-12 now resolves the localized
text through the conversation API. Subtitle widgets and audible lines remain
outside this test.

Remaining dialogue work includes missing conditions, ForceGreet target and
assigned-topic inputs, walk-away topic selection, scene action execution and
timers,
barter transactions, complete alias behavior and wider interruption rules. Godot activation,
menus, voice assets, LIP and idle playback are not connected to this API. This increment does not establish playable dialogue
or complete the broader gameplay goal.

DLG-02 starts a greeting when a driven, living actor enters the current
`fAIInDialogueModeWithPlayerDistance` range of the player in the same cell or
exterior worldspace. The DIAL must use the Player category and ForceGreet
subtype, encoded as legacy DATA subtype 1 or SNAM `PFGT`. AUD-11 also selects
Misc Hello and Idle topics when no conversation is open. The normal dialogue
choice path evaluates its quest and INFO conditions and starts the highest
priority greeting among actors entering range on that tick. A per-Game actor
set records which actors were already in range, so another gameplay tick does
not start the line again.
Leaving range clears that actor's entry and allows a later greeting. Actors
that are dead, in combat, unconscious, disallowed from PC dialogue, disabled,
or translucent without the ghost flag are skipped. The range latch is runtime
state and is not part of the engine save.

CTDA function 123, `IsGreetingPlayer`, takes no parameters and returns true for
the current actor while automatic ForceGreet choices are evaluated. Ordinary
conversation choices return false for that predicate.

CTDA function 248, `IsScenePlaying`, takes a SCEN parameter and checks the
world's active scene set. Function 249, `IsInDialogueWithPlayer`, takes no
parameters and is true for an actor paired with PlayerRef in the active
conversation. Other references return false, as the CK page specifies for
non-actors and non-talking-activators. The engine does not track a speaking
activator. Functions 225 `GetPersuasionNumber` and 315
`GetTotalPersuasionNumber` use the current Speechcraft value, following the
existing DLG-07 dialogue check behavior. The local CK corpus does not document
the returned encoding, so this remains an approximation. Functions 50
`GetTalkedToPC`, 141 `IsTalking` and 172 `GetTalkedToPCParam` return explicit
unsupported-function errors because the local CK corpus has no usable runtime
contract for them.

The DIAL category and legacy subtype fields follow xEdit
`wbDefinitionsTES5.pas`. The SNAM subtype names and the fact that SNAM replaces
the legacy subtype follow UESP `DIAL.wikitext`. The alpha and ghost eligibility
rule follows CK `SetAlpha_-_Actor.wikitext`. CK `MoveTo_-_ObjectReference`
describes ForceGreet as an approach procedure. This increment selects dialogue
after an actor is already in range and does not move actors toward the player.

DLG-21 lets an eligible ForceGreet topic interrupt an open conversation when
its owning quest has strictly higher priority than the current topic's quest.
The incoming topic uses the ordinary quest and INFO condition checks. An
active INFO's end fragment runs before its conversation closes and the new
greeting starts. If it has no end fragment, closure still proceeds. Equal or
lower priority and ineligible greetings leave the conversation in place. The
range-entry latch prevents the same actor from interrupting again until it
leaves and re-enters range. xEdit `wbRecord(QUST)` and UESP `QUST.wikitext`
place quest priority in `DNAM` and describe its range as 0 to 100. The local
CK dialogue corpus contains actor dialogue-state pages, but no page describing
the interruption lifecycle. The callback sequence here reuses the existing
INFO fragment lifecycle.

DLG-07 adds IsIntimidatedbyPlayer (116) and GetIntimidateSuccess (655). Both
take no parameter in `wbDefinitionsTES5.pas` and
`Raw_Function_Data.wikitext`, so a nonzero value in either parameter slot is
rejected with `Error::Parameter`. The intimidated flag reads the
LastBribedIntimidated actor value that also backs SetBribed. The success
check returns `Error::StateContract(655)` when the subject has no Confidence
actor value and `Error::Unavailable(655)` otherwise. The local CK
`WillIntimidateSucceed_-_Actor.wikitext` points to the "Mod Player Intimidation"
perk entry point formula, which the engine does not run yet. Speechcraft and
Confidence alone cannot supply the CTDA result. The Papyrus native
`WillIntimidateSucceed` separately uses the Speechcraft and Confidence
approximation and returns false when that check has invalid input. No engine
state sets LastBribedIntimidated yet either, so `IsIntimidatedbyPlayer` reads
false until
a bribe or intimidate native writes it.

AI-46 covers parameterless functions `GetBribeSuccess` (654),
`GetIntimidateSuccess` (655), `GetArrestedState` (656) and
`GetArrestingActor` (657). Bribe success reads the subject's
`LastBribedIntimidated` actor
value, consistent with CK's `SetBribed` status contract and the paired
intimidation status functions `SetIntimidated` and `IsIntimidated`. Arrested
state reads the subject actor's `arrested` field, matching CK's `IsArrested`.
`GetIntimidateSuccess` (655) reports an unavailable result when the subject
has the required actor state, because CK's `WillIntimidateSucceed` points to
the unimplemented "Mod Player Intimidation" perk entry point. Subjects without
the required confidence value return `Error::StateContract(655)`. Nonzero
unused parameter slots and subjects without actor state return explicit
errors for implemented predicates. `GetArrestingActor` (657) follows CK's
`IsArrestingTarget`: it returns false for non-guards and non-alerted guards.
The runtime does not track an alerted guard's active arrest interaction, so
that context returns `Error::Unavailable(657)`. Function IDs and parameter
counts follow local xEdit
`wbDefinitionsTES5.pas` and UESP `Raw_Function_Data.wikitext`. The actor
contracts follow the local CK `SetBribed`, `SetIntimidated`, `IsIntimidated`,
`IsArrested`, `IsArrestingTarget` and `WillIntimidateSucceed` pages.

DLG-04 implements favor spending, INFO response DATA/ENAM flag bit 14 in
`wbDefinitionsTES5.pas` ("Spends Favor Points"). `info_eligible` now treats
refusal records as fallback responses. Invisible continuation runs after an
INFO response completes.
A favor-spending response instead reads the INFO's CNAM favor level (0 None,
1 Small, 2 Medium, 3 Large) and drops the topic from `dialogue_choices`
(`Ok(false)`, no error) once the speaker's balance cannot cover that level's
cost. `choose_dialogue` debits the same amount when the response is actually
picked, after the session state is recorded, using the existing generic
`ModActorValue` path so nothing beyond the running session changes. Costs
resolve through the Default Object Manager the CK also uses: the CNAM level
picks one of the `FPCS`/`FPCM`/`FPCL` keys (`wbDefinitionsTES5.pas` DOBJ sig
list, matching CommonLibSSE-NG's `BGSDefaultObjectManager.h`
`kFavorCostSmall/Medium/Large`), `Footsteps::default_object` resolves the key
to a GLOB form through the load order, and `World::global_value` reads it. No
corpus source pins the vanilla globals' actual values, so a load order with no
override falls back to an engineering estimate (10, 25, 50), the same caveat
already carried by the `game_settings` defaults.

The spendable balance is the existing `FavorPointsBonus` actor value (CK wiki
AVIF index 89, already in `actor_values::ACTOR_VALUE_NAMES`) rather than a new
field, so a quest funds a speaker through the same generic
`SetActorValue`/`ModActorValue` surface scripts already use, and the balance
rides the existing wholesale `ActorState.values` save and hash without new
code. Vanilla instead persists the spend on the placed reference (`XFVC`
"Favor Cost", `ExtraFavorCost.h`, `REFR.wikitext`), which this engine does not
parse from saves yet. That gap means a favor balance funded on one NPC
reference does not yet distinguish between two placed references of the same
base actor the way vanilla's per-REFR `XFVC` would.

`crates/game/tests/dlg_04_favor_spending.rs` covers both done_check
clauses: `dlg_04_favor_topic_unavailable_when_balance_empty` (an unfunded
speaker never offers the topic) and
`dlg_04_favor_topic_decrements_and_exhausts` (funding the speaker, choosing
the response, checking the balance dropped by exactly the Small cost, and
confirming the topic drops out again once the remainder can no longer cover
another Small favor).

## Active magic applications

### VisualEffect target lifetimes, MAG-24

`VisualEffect.Play(target)` reads the winning `RFCT` record and stores one
transient attachment per effect and target. The attachment retains the
resolved `ARTO` effect-art form, `EFSH` shader form and record flags for a host
consumer. Repeating `Play` for the same pair updates that attachment.
`VisualEffect.Stop(target)` releases only that pair. Attachments are not saved.
The host does not yet render these attachments.

The `RFCT` `DATA` field order and its `ARTO`, `EFSH` and flags are from
`TES5Edit/Core/wbDefinitionsTES5.pas`. Neither the local CK wiki nor the
shipped vanilla scripts contain a dedicated VisualEffect declaration.
`mag_24_` tests check the Papyrus calls, authored form resolution,
target-specific release, invalid inputs and installed RFCT records.

### MGEF perk grants, MAG-21

Each active application retains the `PERK` linked by its winning MGEF record.
The actor's ordinary owned-perk set stays independent. `HasPerk`, perk
conditions and perk entry-point dispatch include grants from live effects.
Perk purchases continue to use the ordinary owned-perk set. Expiry, death
cleanup, `DispelSpell`, `DispelAllSpells` and `ActiveMagicEffect.Dispel`
remove access when the application ends, while an independently owned perk
remains. Active grants are saved in the `MAG-21` world section under SOEW
version 37. Older versions load without an effect-granted perk.

The MGEF `DATA` field order and its `Perk to Apply` reference type come from
`TES5Edit/Core/wbDefinitionsTES5.pas`. Its `KYWD` record definition confirms
the keyword records used by existing magic dispel rules. CK
`DispelSpell_-_Actor.wikitext` says the native dispels every effect from the
specified spell. Tests named `mag_21_` cover temporary access, independent
ownership, dispel, and save/load. Equip abilities remain unsupported.

### Grab Actor, MAG-66

Direct casts accept MGEF archetype 45 and retain the selected actor, caster and
source in the active effect. `World::grab_actor_effect_on` exposes the live
control relationship. Dispel, expiry and the existing death cleanup remove
that relationship with the active effect. `World::set_host_actor_pose` records
finite host poses in the actor's existing reference placement, which is already
saved with the world. The Godot host follows the player's active Grab Actor
target while its effect remains live, then releases it with a throw impulse.
This host control is transient and adds no saved fields. The xEdit MGEF
archetype table labels value 45 `Grab Actor`, and UESP's MGEF layout lists no
extra archetype data. CommonLib's `GrabActorEffect` has a grabbed actor handle
and `Start` and `Finish` callbacks. The named CK Magic Effect page is absent
from the local `ck-wiki` corpus. Tests named `mag_66_` cover the live game
relationship, interruption cleanup and host pose validation.

### Calm archetype, MAG-29

MGEF archetype 6 applies a temporary reduction to the target's `aggression`
actor value, regardless of the MGEF's serialized primary actor value or
detrimental flag. Recoverable effect state restores the prior value when the
effect expires or is dispelled. While Calm is active, the combat driver drops
an existing combat target when the actor's live aggression falls below 1,
matching its target acquisition threshold. Passive actors without Calm retain
their existing targets for authored Flee procedures.
This supports Calm delivery through the existing spell
and ingestible effect paths, subject to their existing validation limits.

The archetype and forced Aggression actor value are identified in the local
`TES5Edit/Core/wbDefinitionsTES5.pas` and
`uesp-modfileformat/MGEF.wikitext`. The CK pages for `HasMagicEffect` describe
queries for active effects but do not specify Calm's aggression update.

### EffectShader native lifetime, MAG-14

`EffectShader.Play(target, duration)` and `Stop(target)` validate that the
receiver is an `EFSH` and the target is a placed `REFR` or `ACHR`, then queue
a transient request keyed by those two resolved form IDs. The omitted Play
duration defaults to `-1`, which the host treats as indefinite. The requests
are not saved. Stop queues a release request, and repeated Play calls for the
same pair refresh one host effect instead of adding another.

The vanilla `EffectShader.psc` in the local SKSE script corpus documents the
negative infinite-duration convention. xEdit's `wbDefinitionsTES5.pas`
identifies EFSH as Effect Shader and shows MGEF's hit and enchant shader links.
The UESP EFSH page documents its texture slots and fade groups. No EffectShader
page exists in the local CK wiki corpus. Play requests carry the winning EFSH
fill texture, first membrane color key, source and destination blend values,
and fill alpha timing to the host. Missing or malformed fields use neutral
defaults. Particle data, palette and edge textures, and skin-only filtering
are not represented in the request.

### Cure Poison, MAG-50

The Cure Poison archetype (MGEF archetype 29) ends active applications whose
source is an ALCH record marked as a poison by ENIT flag bit 17. Other active
effects on the target remain applied. Ending a selected application uses the
usual dispel path, including reversal of recoverable actor-value changes and
`OnEffectFinish` delivery.

The archetype number and ALCH poison bit match
`TES5Edit/Core/wbDefinitionsTES5.pas`, MGEF and ALCH, and CommonLibSSE-NG's
`EffectArchetypes.h` and `AlchemyItem.h`. UESP's `MGEF.wikitext` lists Cure
Poison without additional archetype data. The local CK wiki corpus has no page
specifying the archetype's execution rules.

### Peak Value Modifier, MAG-55

Archetype 34 uses the winning MGEF's `PrimaryAV` and `RelatedID`. The related
form must resolve to a KYWD record. A missing related keyword returns an
unsupported-keyword error before changing state, including the installed
`FrostSlowFFContact` effect whose `RelatedID` is zero.
Active effects on the same target compete
when their keyword and primary actor value match, and the greatest signed
modifier is applied once. Distinct keywords form separate groups. A lower new
modifier does not change the actor value.
When the effect supplying the peak ends, the remaining group is recalculated
and its next greatest modifier takes effect. Secondary actor value fields do
not contribute. The group is derived from active effects and winning MGEF
records, so it adds no save fields.

`magic.rs` tests cover peak selection, keyword groups, ignored secondary
values, and promotion of the next active modifier after expiration. They use
synthetic records.

### Telekinesis prerequisite audit, MAG-41

The 2026-09-20 five-master survey found 1,573 non-deleted MGEF winners and
1,354 non-deleted SPEL winners. One MGEF uses archetype 20, `TelekinesisEffect`
(`0:01A4CB`), referenced by one SPEL, `Telekinesis` (`0:01A4CC`). Both use
cast type 2, concentration, and delivery 0, self. The spell's effect has
magnitude 1, area 0 and duration 0. Neither record has effect conditions.

The MGEF grants `AbTelekinesis` (`0:01A4CA`) as its equip ability. This is a
constant self ability whose `AbTelekinesisEffect` (`0:01A4C9`) modifies actor
value 88, Telekinesis, by magnitude 1. Concentration execution, the actor-value
mapping and owned equip-ability lifetimes therefore precede execution of the
shipped spell. Their backlog owners are MAG-03, MAG-08 and MAG-20.

### Spawn Hazard prerequisite audit, MAG-61

The 2026-09-20 five-master survey found 51 non-deleted HAZD winners, 394 PHZD,
236 PROJ, 1,573 MGEF and 1,354 SPEL. Thirteen MGEF winners use archetype 40,
Spawn Hazard, and resolve to seven distinct HAZD bases. Eleven spells contain
these effects. All eleven are fire-and-forget, ten self and one target actor.
This corpus does not establish a concentration or target-location prerequisite.

Repeat the survey with `SKYRIM_DATA` set:

```sh
rtk proxy cargo run -p soe-game --example telekinesis_survey
```

The example uses the existing SPEL/MGEF record views and resolves references
in each winning record's plugin context. It scans all five masters, prints the
equip-ability chain, and attempts the associated direct casts. On prerequisite
commit `3190621`, the shipped cast returned the existing concentration error
with byte-identical world save state afterward. This establishes the current
rejection boundary, not successful object manipulation.

Local `TES5Edit/Core/wbDefinitionsTES5.pas` identifies archetype 20 and the
casting/delivery enums. `uesp-modfileformat/MGEF.wikitext` supplies the same
archetype number without an object-motion algorithm. CommonLibSSE-NG
`include/RE/T/TelekinesisEffect.h` declares one `grabbedObject` handle and
Start, Update and Finish hooks. The CK pages
`ck-wiki/OnGrab_-_ObjectReference.wikitext` and
`ck-wiki/OnRelease_-_ObjectReference.wikitext` specify player grab and release
events, including Telekinesis. These sources establish object identity and
event lifetimes, but do not supply a Skyrim movement solver. OpenMW's
`apps/openmw/mwworld/worldimp.cpp`, `World::getFocusObject`, implements
Morrowind's extended activation distance, so that policy is not reused here.
The local skymp search found enum declarations, without a Telekinesis runtime
implementation to reuse.

The host installs movable rigid bodies and synchronizes their simulated
placement with the shared World and rendered references.
[The host implementation](godot-host.md#movable-reference-physics-hst-12)
records that path. The Telekinesis state API checks actor and reference
eligibility, while resident-body confirmation and movement use the host API.
The solver moves the kinematic body to a supplied position and does not model
vanilla range or force policy.

`World::grab_telekinesis_reference` accepts one enabled, non-deleted,
non-actor reference while that caster holds a concentration spell containing
archetype 20. PlayerNode selects the resident movable body with the smallest
eye-ray offset, breaking ties by distance, whose center is within 48 Skyrim
units of the eye ray and within 180 units. It confirms the World state and
host body together. While held, the host body is
moved to the same eye-relative distance each physics step. Releasing the
concentration cast, magicka exhaustion or a cast error clears the transient
World reference, returns the host body to rigid simulation and queues
`OnRelease`. A successful hold queues `OnGrab`. A second grab is rejected.
The World does not serialize the grab because the host body is transient.
The MAG-20 equip-ability grant is absent in this worktree, so the shipped
spell's Telekinesis actor-value bonus is not applied.

Repeat the survey with `SKYRIM_DATA` set:

```sh
rtk proxy cargo run -p soe-game --example hazard_survey
```

The example follows winning-record plugin contexts through MGEF, HAZD and
SPEL links. Its diagnostic HAZD fields and MGEF projectile links use the
existing bounds-checked ESM cursor, following local xEdit definitions. They
are not a production HAZD view. The survey initializes the player actor
before taking world snapshots, since first-use actor lookup materializes
state. At prerequisite commit `3190621`, all eleven direct casts rejected
with byte-identical world state: three for conditions and eight for the
unsupported archetype. No script or host simulation ran.

Local `TES5Edit/Core/wbDefinitionsTES5.pas`, `uesp-modfileformat/HAZD.wikitext`
and `Mutagen/Mutagen.Bethesda.Skyrim/Records/Major Records/Hazard.xml` agree on
the 40-byte HAZD DATA layout. It supplies limit, radius, lifetime, image-space
radius, target interval, flags, spell, light, impact-data and sound links.
Its flags can inherit duration or radius from the spawning effect, align to
an impact normal, drop to ground or restrict application to the player.
CommonLibSSE-NG `include/RE/B/BGSHazard.h` names the same fields.

The CK [Spawn Hazard description](https://ck.uesp.net/w/index.php?section=16&title=Magic_Effect)
assigns the hazard to the caster, restricts its hits to enemies and scales
its spell with the spawning magnitude. Constant effects spawn once per
activation. CommonLibSSE-NG `include/RE/S/SpawnHazardEffect.h` retains an
actor cause and hazard handle through its start/finish hooks. Its
`include/RE/H/Hazard.h` declares owner, age, lifetime, target timer, radius
and magnitude on the placed reference, with save/load hooks. These sources
establish fields and ownership, not an executable targeting algorithm.
The local skymp search found the archetype enum but no hazard executor.

The linked Blizzard hazards use contact spells. The largest also inherits
radius and duration, and its spell combines frost damage with a Peak Value
Modifier, owned by MAG-55. Circle of Protection uses a conditioned contact
Turn Undead spell, owned by MAG-45. Some spawning effects grant equip
abilities, owned by MAG-20. Nonzero projectile links also
occur on self effects, but their presence alone does not establish how a
hazard uses them. FMT-02 already owns exposing those MGEF fields.

MAG-61 requires the typed HAZD view in FMT-28, condition evaluation in MAG-04,
radius selection in MAG-06, contact delivery in MAG-16 and the shared hostility
policy in CMB-17. Its host lifecycle depends on HST-04. `World::place_at`
creates a generic reference but has no hazard owner, age or target timer.
The active-effect tick and finish paths only handle actor applications, with
no owned hazard cleanup. [The host audit](godot-host.md#spawn-hazard-prerequisites-mag-61)
records the corresponding resource boundary.

Archetype 40 resolves its associated item as a winning HAZD record and reads it
through the FMT-28 typed view. A valid cast places that base at the selected
target's placement, assigns the caster as owner (or the source when no actor
caster is available), and stores the reference on the active effect. The
HAZD's inherited-duration flag selects the spell effect duration. Otherwise
its positive authored lifetime controls the active effect lifetime. Expiry or
dispel disables and marks the owned hazard reference deleted through the
existing summoned-reference cleanup. Missing, malformed or invalid HAZD data
is rejected during preparation before applying effects. Tests
`mag_61_cast_spawns_authored_hazard_at_target_with_caster_owner` and
`mag_61_expiry_disables_only_its_owned_hazard` cover placement and cleanup.

At each authored HAZD target interval, the runtime selects enabled, live actors
inside the hazard radius and applies its linked spell, scaled by the spawning
effect magnitude. The caster is excluded. Radius inheritance uses the spawning
effect's area. This uses the world's placed-reference positions and does not
create a host collision shape. Timed contact applications use the hazard
reference as their source, so hazard expiry or dispel ends those applications
along with the hazard. The active effect's elapsed time and dynamic reference
use the existing save path. Tests
`mag_61_hazard_periodically_applies_its_contact_spell_to_the_target` and
`mag_61_hazard_expiry_ends_its_contact_effects` cover contact and cleanup.
No installed-save survey covers a live hazard.

### Direct active-effect foundation

The MIT CommonLib shim adapter in `shim_world` reads actor critical stages,
actor-value components, the AI driver's current package and procedure, authored
`XAPR` activation parents and live active-effect applications from `World`.
Activation-parent FormIDs resolve in the owning reference's load-order context.
Unavailable actor, reference and effect identities produce typed shim errors.
Engine-internal `AIProcess` hooks that need a character controller or scene
node produce an unsupported-hook error.

The runtime provides `World::cast_spell` for direct scripted casts
of Script, Value Modifier, Absorb, Dual Value Modifier, Value and Parts,
Dispel, Cloak and Etherealize effects, including hostile hits. Definitions
come from winning SPEL and MGEF records, with each FormID resolved in its winning
plugin. Self, aimed actor and target-actor delivery are supported for constant
and fire-and-forget casts when the caller supplies an actor target. `Spell.Cast`
credits an actor source. `RemoteCast` credits its explicit
blame actor. Player charge progress and hand input remain unfinished.

`World::spell_charge_time` returns authored SPIT charge time when Manual Cost
Calc is set, otherwise the maximum MGEF casting time in the SPEL effect list.
`World::spell_cost` returns authored SPIT base cost for manual-cost spells.
Auto-calculated cost sums each MGEF base cost scaled by magnitude and duration,
with magnitude below one raised to one, zero duration treated as ten, and
concentration duration treated as ten. It applies the SPEL half-cost perk when
the caster owns that perk. `World::cast_charged_spell` validates the cast and
available magicka before debiting the cost once and applying effects. This
one-shot API does not model charge progress or concentration ticks. These rules
follow local `uesp-modfileformat/SPEL.wikitext` and
`TES5Edit/Core/wbDefinitionsTES5.pas`. `tests/magic/mag_09_cast_cost.rs`
checks authored and calculated timing and costs, perk adjustment, and rejection
without state changes when magicka is insufficient.
The one-shot charged gameplay path sends `OnSpellCast` once after validation and payment.
The CK `ck-wiki/OnSpellCast_-_ObjectReference.wikitext` contract says the event
fires once when concentration begins and is not detected for Papyrus casts, so
the direct scripted `World::cast_spell` path does not send it.

`World::begin_concentration_cast` starts one held concentration spell after
validating its execution path and available magicka. `Game::tick` advances the
held effect and drains magicka at one tenth of the spell's calculated cost per
unpaused simulation second. This rate uses the concentration ten-second cost
duration in local `uesp-modfileformat/SPEL.wikitext`. Effect changes are applied
in 0.1-second slices, with a final partial slice when magicka runs out. Self,
aimed actor and target actor deliveries work, with the caller supplying the
actor target for aimed and target-actor spells. `release_concentration_cast`
ends the held cast, and exhaustion ends it automatically. The held gameplay
path sends `OnSpellCast` once at the start. Menu-paused ticks do not advance
held casts. Held input is transient and is not stored in SOEW, so loading a save
does not resume casting. `tests/magic/mag_03_concentration.rs` checks these
behaviors and a gated installed-data cast. Concentration fields follow xEdit's
SPEL/SPIT definition, and the one-time event follows local
`ck-wiki/OnSpellCast_-_ObjectReference.wikitext`.

Touch delivery resolves only the supplied actor target and measures its
three-dimensional distance from the source reference against the SPEL SPIT
range, including a target exactly at that boundary. Missing targets, non-actor
targets, targets beyond reach and non-finite distances reject the whole cast
before any effects apply. A touch spell with a negative or non-finite authored
range returns the typed invalid-number error. The layout and delivery enum
come from local `TES5Edit/Core/wbDefinitionsTES5.pas`. The CK
`DoCombatSpellApply_-_Actor.wikitext` page says its separate combat native
applies a Target Actor spell regardless of battlefield distance, so that
target-actor behavior is not used for touch delivery.

`World::cast_spell_at_location` accepts a selected position for SPIT delivery
4, checks the finite position and authored range from the source reference,
and rejects the cast when the host reports an obstructed line to that point.
Each effect applies to enabled, undeleted actor references inside its authored
EFIT area around the selected point. A zero area selects only actors at the
exact point. xEdit's SPIT definition and the local UESP SPEL format page name
Target Location as delivery 4 and Range as valid for this delivery. The host
collision result is supplied by `godot-host` and is not saved. Cloak effects
are the concentration exception: their associated aimed or target spell runs
once per second while the cloak is active. Their saved pulse phase and owned
effect identities are described under Cloak, MAG-56.

Etherealize is MGEF archetype 41. The Creation Kit's
[Magic Effect page](https://ck.uesp.net/wiki/Magic_Effect) says the archetype
prevents targets from dealing or taking damage. While an application is active,
the runtime blocks hostile magic and melee hits, melee attacks, hostile spell
casts and reference activation by that actor. It derives status from the
actor's live Etherealize applications. Ending one application removes only
that application's status, leaving overlapping applications and the actor's
separate ghost flag intact. The xEdit MGEF archetype table and UESP's MGEF
layout identify archetype 41 and list no additional record data. No save fields
were added because active effect identities already persist.

Each SPEL effect group's CTDAs run before that group is prepared. MGEF CTDAs
run separately, with FormIDs resolved against their owning MGEF. Both use the
delivery recipient as subject and the credited caster as target. A false group
skips only its effect. The currently implemented CTDA function set is shared
with dialogue and crafting. Variable-backed predicates return a typed error
when the referenced script variable is absent, ambiguous, nonconditional or
nonnumeric. Papyrus `Spell.Cast` and `Spell.RemoteCast` read those values from
the executing native call, so the live VM remains available without borrowing
the VM itself. Direct `World::cast_spell` calls have no VM and still reject
variable-backed predicates.
The [Creation Kit Magic Effect page](https://ck.uesp.net/wiki/Magic_Effect)
calls the flag "Swap Subject and Target" useful for running a condition on
the spellcaster. The common SPEL/MGEF context is inferred from that statement
and the shared Effect Item layout, with the recipient as default subject and
the caster as default target.

### Spell absorption, MAG-23

For each recipient with a prepared hostile MGEF, `World::cast_spell` checks the
recipient's `AbsorbChance` actor value once for that cast. The chance is clamped
to 0 through 100 percent and uses the world's saved random stream when it is
between those limits. The SPEL `Disallow Absorb/Reflect` flag bypasses the
check. On success, hostile applications for that recipient are omitted and the
SPEL base cost is restored to that recipient's magicka once, up to its current
maximum. Non-hostile applications are kept. `mag_23_` tests in
`tests/magic.rs` cover absorption, one resource return across repeated effect
entries, the SPEL flag, zero chance and non-hostile effects.

The numeric flag and actor-value fields follow local
`TES5Edit/Core/wbDefinitionsTES5.pas` and `uesp-modfileformat/SPEL.wikitext`.
The local skymp `libespm/include/libespm/SPEL.h` and `ActorValue.h` confirm the
same flag bit and `AbsorbChance` index. Those skymp files define records and
names, not cast resolution or resource-return behavior.
An active spell application checks its SPEL and MGEF conditions again at each
whole second of its elapsed lifetime. A false condition ends that application.
The elapsed lifetime is already part of SOEW magic state, so a save between
checks retains the time remaining until the next check. Concentration builds
fresh effect applications for each 0.1-second cast slice and evaluates their
conditions during that reapplication. Ingestibles do not use the spell
reevaluation cadence.

Each application gets an `EffectId` and a distinct `ActiveMagicEffect` VM object.
MGEF scripts are attached when an effect is applied, rather than attached to the
MGEF form at startup. `bind` reports missing MGEF PEX files without creating an
effect object or attaching its scripts. VMAD properties use the winning MGEF's plugin context.
The queue binds the scripts before sending `OnEffectStart`, and expiry or dispel
sends `OnEffectFinish`. The getters return the base MGEF, target actor and caster
while the application is live. Finish handlers may observe absent native state,
as allowed by the shipped ActiveMagicEffect.psc contract.

An active effect targeting an inventory holder receives `OnItemAdded` and
`OnItemRemoved` only when its own filters admit the changed base item. The
three `ActiveMagicEffect` filter natives use the same `World` filter API as
references and aliases. Filters stack, match a form directly or one of the
direct members of a form list, and remain local to each listener. Nested form
lists are not searched. Removing one filter preserves the others, while
`RemoveAllInventoryEventFilters` restores unfiltered notifications. Expiry and
dispel remove the effect's filters before later changes are queued.
`active_effect_inventory_filters_forward_matching_events_until_expiry` and
`scr_01_active_effect_inventory_filter_bindings_control_events` in
`tests/magic.rs` cover dispatch, arguments, removal, and expiry.

After each VM run, expired effect objects are reclaimed once no script variable,
array, thread frame, thread result or queued callback retains them. Pending
bindings and death recipients also keep objects alive. Live form and alias objects
remain roots. Sleeping or latent finish handlers retain their instances through
save/load, then release them when the handler and other references finish.
Reclamation removes both world handle mappings and any registrations made after
dispel. Future registrations alone do not keep an expired effect alive.
`tests/magic/collection.rs` checks this lifecycle, including 64 casts dispelled
before binding, a suspended finish callback, a live alias reference and a callback
queued during `OnEffectFinish`. VM storage checks are described in
[Papyrus](papyrus.md).

The four `mag_69_` predicate tests check true and false quest and reference
variables through both spell natives. The ambiguous-variable case confirms a
failed predicate applies no effect and queues no callbacks. The CK Papyrus
introduction describes scripts as variables read by engine functions. The
conditional flag and ambiguity rules come from the local VM contract.

Enhance Weapon (MGEF archetype 39) reads its associated ENCH record when a live
effect's target lands an ordinary melee hit with an equipped weapon. Supported
ENCH entries with direct detrimental Health modifiers add their magnitude to
that weapon's damage for the hit. Direct detrimental Stamina modifiers reduce
the target's Stamina on the same hit. Both contributions end with the active
effect and do not change unarmed damage, another actor's hit or an unequipped
item. Conditioned enchantment entries, multiple entries and other actor values
return an unsupported-combat error before the hit changes state. The contract
comes from the CK Magic Effect archetype table, xEdit's MGEF archetype and
associated item fields, and Mutagen's ENCH association type.
`mag_60_enhance_weapon_enchantment_applies_to_equipped_melee_weapon` covers
Health damage and expiry. The adversarial `mag_60_supports_a_valid_non_health_associated_enchantment_on_melee_hit`
case covers Stamina damage, and
`mag_60_effect_does_not_benefit_a_different_attackers_weapon` covers attacker
ownership.

### Frenzy, MAG-31

Direct casts execute MGEF archetype 8 by applying its signed magnitude to the
target's Aggression through the existing actor-value modifier path and
marking the actor as Frenzied while that active effect remains. Combat
acquisition may then select detected teammates, which ordinary acquisition
skips. When the last Frenzy application
ends or expires, its actor leaves the current engagement and the ordinary
target rules apply again. Target acquisition still requires Combat-level
awareness. The effect and its Aggression delta use the existing active-effect
save state, with no additional world field.

The source is local xEdit `wbDefinitionsTES5.pas`, whose MGEF archetype enum
names value 8 as Frenzy, and UESP's local `MGEF.wikitext`, which says Frenzy's
primary actor value is Aggression. The CK wiki dump has no Magic Effect
archetype page. Its `OnMagicEffectApply_-_ObjectReference.wikitext` defines
the responsible caster and effect event arguments already used by this path.
`mag_31_frenzy_allows_combat_targeting_of_a_teammate` and
`mag_31_frenzy_expiry_releases_its_combat_target_override` cover target
eligibility and expiry with synthetic records. The check names no vanilla
records, so it has no `SKYRIM_DATA`-gated case. Range selection beyond existing
Combat-level awareness and the retail target scoring rules remain outside this
behavior.

Recover modifiers change the target once and reverse their own delta at the end.
Other value modifiers integrate magnitude over unpaused simulation seconds, with
the final tick clipped to the remaining duration. Zero-duration effects apply
once. Hostile magic effects first apply the target's current `resistmagic`, then
the MGEF `Resist Value` actor value when it names poison, fire, shock, frost,
magic or disease resistance. Each term is clamped from 0 to 85 percent and
reductions multiply. `DamageResist` and other MGEF resistance indices return an
unsupported behavior error before the cast changes state. The `-1` sentinel
omits the specific resistance. Non-hostile effects bypass resistance. Current
values include the actor-value store's base, permanent, temporary and damage
components. Healing health, magicka and stamina is limited to the maximum,
including permanent and temporary modifiers. No Recast prevents a second live
application of the same MGEF on the same actor. Death removes effects unless No
Death Dispel is set.
SOEW version 9 stores identity, source, caster, target, magnitude, elapsed time,
duration and value deltas, including pending script attachment. Loading never
reapplies a saved modifier. The deterministic scenes hash includes this state.

Recover modifiers apply once and reverse their own actor-value delta when the effect ends. Other value modifiers integrate magnitude over unpaused simulation time, with the final tick clipped to the remaining duration. Zero-duration effects apply once. Healing health, magicka and stamina is limited to the maximum, including permanent and temporary modifiers. No Recast prevents a second live application of the same MGEF on an actor, and death removes effects unless No Death Dispel is set.

An active MGEF's `Equip Ability` link grants that spell for the lifetime of the
application. `Actor.HasSpell` and condition function 264 see the grant while the
effect is active. The grant is derived from the saved active effect, so expiry
or dispel removes only that application's grant and preserves a spell the actor
knows independently. The link is resolved in the MGEF's winning plugin context
and must name a live SPEL. MGEF `Perk to Apply` remains unsupported. xEdit's
MGEF schema identifies these as separate SPEL and PERK links, and CK
`DispelSpell_-_Actor.wikitext` documents spell-wide removal independently of
the active effect's equip ability.

`mag_20_effect_grants_its_authored_equip_ability`,
`mag_20_expiry_removes_its_equip_ability_grant` and
`mag_20_expiring_grant_preserves_independently_owned_spell` cover grant
visibility and ownership through expiry.

MGEF's `Resist Value` field and its enum come from local
`TES5Edit/Core/wbDefinitionsTES5.pas`. Its actor-value indices map through
`crates/game/src/actor_values.rs`. The local skymp magic action handler leaves
magic-effect application as a TODO, so it supplies no resistance formula. The
85 percent cap and order are the runtime's selected behavior. Keeping magic
and specific resistance as separate terms preserves their independent actor
values.

For timed effects, taper adds its authored duration after EFIT duration. Non-RECOVER value changes integrate `magnitude * weight * (1 - taper_time / taper_duration)^curve` over that interval. RECOVER changes keep full magnitude and reverse at the end of the taper interval. Curves at or below -1 with positive taper duration are rejected because their total is unbounded. Active spell conditions are reevaluated at whole-second boundaries, and a false condition ends the application. Concentration recasts evaluate conditions in each 0.1-second slice. The `MAG-19` world section stores active-effect taper values. Version 37 stores granted perks and command-owner data in named sections. Loading restores existing modifiers without applying them again.

Hostile effects first apply the target's current `resistmagic`, then the MGEF `Resist Value` actor value when it names poison, fire, shock, frost, magic or disease resistance. Each term is clamped from 0 to 85 percent and reductions multiply. `DamageResist` and unsupported resistance indices return an error before state changes. The `-1` sentinel omits specific resistance, and non-hostile effects bypass resistance. Current values include base, permanent, temporary and damage components.

The runtime executes MGEF archetype 33, Stagger, for spells and ingestibles.
Each prepared Stagger application passes its authored magnitude and caster
form, falling back to the cast source, to the shared stagger lifecycle once.
It uses the same `fStaggerDuration` real-time deadline as melee stagger. Actor
state retains the latest magnitude and direction source with that deadline,
serialized in the `MAG-54` actor save section under SOEW version 37. Local
xEdit and UESP MGEF layouts name archetype 33 and specify no archetype-specific
data. The local CK wiki dump has no page that defines its runtime behavior.

The source contracts are CK [Magic Effect](https://ck.uesp.net/wiki/Magic_Effect),
local `ck-wiki/AddInventoryEventFilter_-_ObjectReference.wikitext`,
`DispelSpell_-_Actor.wikitext` and
`DispelAllSpells_-_Actor.wikitext`, the shipped
`skse64/scripts/vanilla/ActiveMagicEffect.psc`, xEdit's SPEL/MGEF definitions, and
CommonLibSSE-NG `ActorValues.h` and `MagicSystem.h`. The skymp
`ActiveMagicEffectsMap` was read for comparison. Its one-effect-per-actor-value
map and wall-clock expiry do not fit independent applications or menu pause, so
those policies were not reused. `thiserror` is a direct game dependency for
typed cast errors and was already present transitively.

`World::drink_ingestible` reads a winning ALCH record, validates every EFID and
EFIT group, then consumes one item from the drinker's inventory and applies the
supported effects to that actor. The ALCH FormID is the active effect's source
and the drinker is the active-effect caster and `OnMagicEffectApply` caster.
`Actor.EquipItem` routes ALCH records through this operation. A timed effect
expires on the ordinary game clock,
while a zero-duration effect applies once. Unsupported effects and malformed
records leave the inventory unchanged. The reader follows local xEdit
`wbRecord(ALCH)` and `uesp-modfileformat/ALCH.wikitext`, which specify ENIT and
the shared EFID/EFIT groups. Food, medicine and poison flags are retained in
the record view. Food and medicine apply through this path. Poison application
to equipped weapons stays rejected without consuming the item until CMB-25
implements that separate interaction. Addiction and consume sounds have no
runtime behavior yet.

`Actor.EquipItem` also reads an armor's optional EITM link to a winning ENCH.
Only constant self-delivered enchantments apply. Their EFID/EFIT entries use
the same MGEF application rules as ingestibles, but the effects have no expiry
and stay active until the armor is unequipped or
replaced by overlapping armor. Ending an application reverses its actor-value
changes. `DispelAllSpells` keeps these effects while their source armor remains
worn. This path reads the ENCH ENIT and ordered effect layout from local xEdit
`wbRecord(ENCH)`, and the EITM link from xEdit `wbEnchantment` and
`ARMO.wikitext`. The CK `EquipItem` and `UnequipItem` pages describe the native
equipment calls. Effect conditions use the existing non-VM condition subset.
Papyrus-variable conditions remain unsupported here.

SOEW validation resolves saved active-effect entries through their owning SPEL,
ALCH or ENCH record. Restoring an active potion or worn-enchantment effect
retains its inventory state and existing modifier without applying the effect
again. Potion effects expire from their saved elapsed time. Worn effects remain
active while the armor stays equipped.

Unsupported casts return a typed error before applying effects, and Papyrus casts log that error. Direct, charged and concentration casts, projectile contact, target-location selection and area targeting use their supported execution paths. Unsupported archetypes, equip abilities and unsupported resistance values remain rejected before changing state. Area selection is described below under Area targeting, MAG-06.

### Cure Disease and Absorb archetypes, MAG-27 and MAG-28

Archetype 3 removes active applications whose winning MGEF has the `MagicDisease` keyword, using the ordinary dispel lifecycle. Archetype 4 selects its actor value from MGEF `PrimaryAV`, damages that value on the recipient and restores the same amount to the credited caster over the effect duration. `No Magnitude` suppresses both changes, while `No Duration` transfers the value immediately.

### Disarm, Invisibility, Lock, Open, Light and Bound Weapon, MAG-32 to MAG-38

Disarm drops one eligible hand weapon as a loose reference when the target's level is within the effect magnitude. Invisibility derives from the active MGEF and ends on expiry, dispel, another spell cast, an accepted melee hit or reference activation. Lock and Open update eligible door and container references through shared lock state. Light spawns a runtime LIGH reference that the active effect removes on expiry or dispel. Bound Weapon grants the authored weapon temporarily and removes only its granted copy when the effect ends.
Unsupported casts return a typed error before applying any of their effects, and Papyrus casts log that error. Unsupported cases include concentration, taper, perk adjustments, equip abilities and other unsupported archetypes. Aimed casts require projectile contact and location casts use the selected point for area targeting. Area selection is covered separately under Area targeting, MAG-06. Absorb and Cure Disease are supported below.

`Scroll.Cast(source, target)` calls `World::cast_scroll_from_record` on its owning SPEL
record. A SPEL record with cast type 3 answers as Papyrus class `Scroll`, which
lets VM method dispatch reach this native. The record must use cast type 3.
Source and optional target references flow into the shared cast executor,
which applies supported effects and keeps the source, caster and target on
each active effect. A missing required actor target or unsupported effect logs
the typed error under `Scroll.Cast`. This path does not charge the player or
consume an item. The local `skse64/scripts/vanilla/Scroll.psc` supplies the
method signature and optional target contract. The CK dump has no `Scroll.Cast`
page. `tests/magic.rs` covers cast attribution, effect application and typed
failure paths.

#### Cure Disease archetype, MAG-27

MGEF archetype 3 removes active applications on the target whose MGEF lists the
`MagicDisease` keyword. It uses the ordinary dispel path, so recovered values,
effect timers, and finish callbacks follow the same cleanup as other dispels.
`HasMagicEffectWithKeyword` reads authored MGEF keywords directly, including
effects that use the Dispel With Keywords flag. It therefore reports disease
applications before a cure and stops reporting them after removal. Learned
spells and unrelated active effects remain unchanged. The scope follows CK's
disease keyword query and xEdit's archetype table. The local skymp implementation
instead removes learned spells of Disease type, which does not match this
runtime's per-application active-effect model.

#### Absorb archetype, MAG-28

Archetype 4 selects its actor value from MGEF `PrimaryAV`. During each active
tick, it damages that value on the recipient and restores the same amount to
the credited caster, using the authored magnitude per second and clipping the
last tick to the remaining duration. A missing caster still leaves the target
drain. `No Magnitude` suppresses both sides, and `No Duration` applies the
transfer immediately. The value mapping and ordinary damage and restore rules
are shared with other active effects.

The local xEdit `wbDefinitionsTES5.pas` names archetype 4 as Absorb, and UESP's
`MGEF.wikitext` says `PrimaryAV` selects the absorbed value. The CK corpus has
no Magic Effect page. CommonLibSSE-NG's `AbsorbEffect.h` confirms start, update
and finish hooks on an absorb effect derived from `ValueModifierEffect`, but
does not provide their behavior. `mag_28_*` tests cover primary-value
selection, magnitude, duration, and the `No Magnitude` and `No Duration` flags.

#### Disarm archetype, MAG-32

Archetype 9 disarms a target when the target's actor level does not exceed the
effect magnitude and the actor has a weapon assigned to either hand. One copy
of the selected hand weapon leaves the actor inventory and becomes an enabled
loose reference at the actor's position, rotation and cell. The runtime clears
the weapon's hand assignment. Picking up that reference returns the same
single copy through the ordinary reference inventory path. A target above the
magnitude limit or without an equipped hand weapon keeps its inventory and
equipment unchanged.

Local `TES5Edit/Core/wbDefinitionsTES5.pas` and
`uesp-modfileformat/MGEF.wikitext` name archetype 9 as Disarm and specify no
archetype-specific data. The local CK corpus has no Magic Effect page. The CK
Magic Effect archetype table describes Disarm as affecting targets up to its
magnitude, which supplies the level limit. The implementation uses the shared
inventory, equipment and loose-reference state, whose metadata and pickup
behavior are documented above. `mag_32_*` tests cover the level boundary,
single-copy transfer, dropped placement and pickup.

#### Invisibility archetype, MAG-34

MGEF archetype 11 creates an ordinary active effect for its authored duration.
`World::is_invisible` derives the state from the active effect's winning MGEF,
so dispel and expiry end it without a second saved flag. Detection uses that
same query alongside direct changes to the Invisibility actor value. A
successful spell cast, accepted melee hit or reference activation ends the
actor's current Invisibility effects through the normal dispel lifecycle.

The local xEdit `wbDefinitionsTES5.pas` names archetype 11 Invisibility and
defines the duration flags. UESP's `MGEF.wikitext` describes the MGEF duration
fields. The CK corpus has no Magic Effect page, so the break actions are the
runtime's explicit interaction contract. `mag_34_*` tests cover duration,
spell and activation breaks, detection state derivation and host visibility
mapping.

#### Lock archetype, MAG-36

MGEF archetype 15 applies its rounded magnitude as the lock level on one
target reference whose base form is a `DOOR` or `CONT`. The level change goes
through `World::set_lock_level`, preserving the reference's locked state and
queuing `OnLockStateChanged` only when the level changes. Actor and other
reference targets, plus area Lock effects, return a typed error before an
effect is applied. Lock effects are immediate and do not create an
actor-targeted active-effect entry. The level setter clamps to the byte range
used by Skyrim's lock data.

Local `TES5Edit/Core/wbDefinitionsTES5.pas` names archetype 15 Lock, and the
CK `Lock` and `GetLockLevel` pages describe reference lock behavior and levels.
The CK corpus has no Magic Effect page with further Lock-specific execution
rules. The shared state path is `world/locks.rs`. `mag_36_lock_*` covers level
changes on doors and containers, preservation of locked state, lock-change
events, and rejection of actor and non-lockable reference targets.

#### Open archetype, MAG-37

MGEF archetype 16 opens a reference lock when the effect magnitude is at least
the reference's lock level. It accepts doors and containers, leaves a stronger
lock unchanged, and uses `World::set_locked` for eligible locks, so ownership
state is retained and `OnLockStateChanged` is queued. Open effects are
immediate and do not create active effects. Actor and other reference targets,
plus area Open effects, return a typed error before changing state.

The local xEdit MGEF table names archetype 16 Open. Its format description and
the local CK corpus do not specify the magnitude threshold, so the behavior
here follows the MAG-37 done check. CK `Lock` and `GetLockLevel` define the
shared reference lock state used by the executor.

#### Light archetype, MAG-35

MGEF archetype 12 resolves its Associated Item as a winning `LIGH` record and
places a runtime reference of that light at each selected target. The active
effect retains that reference in the existing spawned-reference save field.
Dispel and timed expiry disable and delete the reference through the ordinary
active-effect cleanup path. The local xEdit MGEF table names archetype 12 as
Light, and UESP's MGEF page identifies `RelatedID` as its light FormID. The
local CK corpus has no Magic Effect page. `mag_35_authored_light_spawns_at_target_and_is_removed_on_expiry`
checks the resolved base, target placement, save/load and expiry cleanup. The
Godot host does not yet turn runtime `LIGH` references into rendered light
instances.

#### Bound Weapon archetype, MAG-38

Archetype 17 resolves the MGEF Associated Item through the effect's owning
plugin and requires a WEAP record marked with the WEAP Flags2 Bound Weapon bit.
It grants one inventory copy to the target and selects that item in the right
hand. Expiry and dispel clear its hand and equipped entries and remove one copy
through the normal inventory event path. When another copy remains, its hand
selection remains valid. Active applications and inventories already persist,
so the associated weapon is resolved again from the winning MGEF after load.
The acceptance tests cover both final-copy cleanup and preserving an existing
copy when the temporary copy expires, plus a five-master link survey. xEdit's
`wbDefinitionsTES5.pas` defines the MGEF associated-item types, archetype 17
and the WEAP flag. CommonLib's `EffectArchetypes.h` confirms the numeric
archetype name. The local CK corpus does not contain its Magic Effect page.

Unsupported casts return a typed error before applying any of their effects,
and Papyrus casts log that error. This includes projectiles, contact and
location delivery, concentration, taper, perk adjustments, equip abilities and
other archetypes. Area selection is covered separately below, under Area
targeting, MAG-06.

### Guide effect, MAG-46

MGEF archetype 25 is accepted as an active effect. `World::guide_route` returns
the navmesh route from the player to the nearest live reference target of the
first eligible displayed objective. It considers running, active quests that
are not completed or failed, sorts quests by descending priority and then by
FormID, and skips objectives that are hidden, completed or failed. Objectives
with no resolvable enabled reference target produce no route. The returned
route is recomputed from current quest and reference state, and becomes absent
when the Guide application expires.

The definitions come from local `TES5Edit/Core/wbDefinitionsTES5.pas`, which
names archetype 25 as Guide and selects a HAZD associated item for it, and
`uesp-modfileformat/MGEF.wikitext`, which says that associated hazard is used
only by Clairvoyance. The CK dump has no Guide archetype page. The local
CommonLib `GuideEffect.h` declares quest, quest target and hazard members plus
Start and Finish hooks, but no behavior. The hazard link is not treated as the
objective target. Guide concentration casting, Clairvoyance hazard visuals and
host rendering of the route are not implemented by this runtime API.

`mag_46_guide_routes_to_the_current_eligible_objective`,
`mag_46_guide_clears_the_route_when_its_effect_expires` and
`mag_46_guide_skips_completed_and_failed_objectives` exercise route selection,
objective eligibility and expiry with synthetic records. The check names no
vanilla record, so it has no `SKYRIM_DATA`-gated case.

`ObjectReference.InterruptCast` clears a pending fire-and-forget charge or
concentration session for that reference. Beginning either session applies no
effects and spends no magicka. Releasing a fire-and-forget session calls the
existing direct effect path, after which `InterruptCast` leaves its effects
alone. Concentration sessions can be interrupted, but sustained effect ticks
and casting costs are not implemented. Pending cast sessions are transient and
are not part of SOEW save state. The contract comes from the local CK
`InterruptCast_-_ObjectReference.wikitext`. `tests/magic/mag_73.rs` covers
charging interruption, concentration interruption and interruption after
fire-and-forget release.
An MGEF with the Dispel with Keywords flag reads its KWDA entries as the
cancellation list. Casting that effect, or applying it from an ingestible,
ends only live effects on the recipient whose winning MGEF carries a matching
keyword. Other effects from the same spell and effects on other actors stay
active. Each ended application queues one `OnEffectFinish`, even when several
rules select it. The list comes from xEdit's MGEF flag and KWDA definitions,
with CK `DispelSpell_-_Actor.wikitext` documenting the separate explicit
spell-removal native. PERK definitions were checked but perk adjustments remain
unsupported. Synthetic `mag_07_*` tests cover selection and delivery. The
installed five-master survey for this rule has not been run.
Actor-value indices from 0 through 163 resolve through the shared mapping in
[actor values](actor-values.md). Recover effects use the temporary modifier
domain. Other effects damage or restore the damage component without changing
permanent modifiers.
Automatic ability application from AddSpell or NPC spell lists, enchantments,
potions, resistance and absorption, effect artwork and sounds, actor-event
forwarding and VM object reclamation remain unfinished.

### Vampire Lord transformation, MAG-67

MGEF archetype 46 resolves its associated item as a RACE form. Applying it
sets the target actor's runtime race to that winning form and adds the race's
direct SPLO entries whose winning SPEL records have the Ability type. Only
spells newly inserted by that transformation are owned by it. Their constant
effects are applied through the actor ability path. The `MAG-67` world save
section retains the previous race, assigned race, added spells and resulting
effect identities. Dispel or expiry restores the previous race only if the
actor still has the race this effect assigned, then removes its added spells
and their active effects. A spell that was already known remains known.

The local `TES5Edit/Core/wbDefinitionsTES5.pas` MGEF archetype enum and
`uesp-modfileformat/MGEF.wikitext` identify archetype 46 and its associated
RACE link. xEdit's `wbSPLOs` definition identifies race SPLO entries as actor
effects. The requested CK Magic Effect page is absent from the local CK wiki
dump. The available CK `GetRace`, `AddSpell` and `RemoveSpell` pages establish
the actor race and spell operations used here. Race SPLO entries that are
leveled lists or shouts are not applied by this path.

### Reanimate, MAG-43

Direct target-actor casts accept MGEF archetype 22 for an enabled, dead,
non-ghost, non-essential actor. A positive magnitude caps the target's level.
The cast requires a credited actor caster. It restores the corpse's health,
credits that caster as the reference owner while the effect is active, and
retains the previous owner. The authored EFIT duration advances on the active
magic clock. Expiry or dispel kills the reanimated actor and restores its
previous owner. The MAG-43 world section saves that owner so expiry after load
has the same result. Reanimate reads the
authored EFIT magnitude, duration and area even when the corresponding MGEF
flags hide those fields in the editor.

The local xEdit MGEF archetype table identifies Reanimate as 22, and the CK
Magic Effect page specifies a dead target under caster control. Local
`uesp-modfileformat/MGEF.wikitext` lists no associated item for this
archetype. `CommonLibSSE-NG/include/RE/R/ReanimateEffect.h` derives it from
`CommandEffect` and names Start, Update, Finish and save hooks. The runtime
uses its existing active-effect lifecycle and world reference state rather
than those C++ hooks.

### Soul Trap, MAG-44

Direct target-actor casts accept MGEF archetype 23 as an active application.
Its authored EFIT duration advances on the active magic clock, including for
fire-and-forget casts.
When its target dies, the credited caster's inventory is searched for empty
SLGM records whose capacity meets the target's soul size. Soul size follows
`iLesserSoulActorLevel`, `iCommonSoulActorLevel`, `iGreaterSoulActorLevel` and
`iGrandSoulActorLevel`, with engine defaults 4, 16, 28 and 38. The smallest
eligible capacity wins, with FormID as the tie-breaker. An eligible gem needs a
valid `NAM0` link to a filled SLGM record. For targets marked by the
`ActorTypeNPC` keyword on their NPC base or race, the empty gem must also have
record flag bit 17, `Can Hold NPC Soul`, from xEdit's SLGM definition. Capture
replaces one empty gem with one filled gem and queues both inventory count
events. All matching Soul Trap applications end on the first death resolution, even when the
selected caster has no eligible gem, so repeated applications cannot fill
multiple gems from the same death.

The local xEdit MGEF table identifies archetype 23 as Soul Trap. Its SLGM
definition and UESP `SLGM.wikitext` define current soul, capacity, filled gem
link and the record flag for NPC souls. xEdit assigns that flag to bit 17.
UESP `MGEF.wikitext` says Soul Trap has
no archetype-specific data. CK `TrapSoul_-_Actor.wikitext` describes capture
as a success or failure, while the local CK Magic Effect archetype page is not
present in the corpus. The runtime reuses the active-effect target and caster,
the existing death queue and the shared inventory event path. The local CK
archetype page is absent, so `ActorTypeNPC` is used as the available target
classification marker. `mag_44_npc_soul_requires_a_soul_gem_marked_for_npc_souls`
checks that NPC marked targets require the authored gem flag.

### Paralysis, MAG-42

Direct casts and ingestibles accept MGEF archetype 21 as a timed active effect.
Its signed magnitude temporarily modifies the forced `paralysis` actor value.
While that effect is live, `World::actor_is_paralyzed` reports true for its
recipient. The NPC package driver pauses that actor before package evaluation
or movement, and the host player controller sends zero horizontal movement and
does not jump. Expiry reverses only this effect's actor-value delta and clears
the condition, so NPC and player movement resume. This uses the winning MGEF
record and existing saved active-effect state, with no additional save fields.

The sources are local `TES5Edit/Core/wbDefinitionsTES5.pas` and
`uesp-modfileformat/MGEF.wikitext`. Their archetype tables identify 21 as
Paralysis and require the `paralysis` actor value. The CK `HasMagicEffect` and
`OnMagicEffectApply` pages describe active application checks and callbacks,
not movement handling. `mag_42_paralysis_stops_npc_travel_until_effect_expires`
exercises NPC movement, the forced actor value and expiry with synthetic
records. Player locomotion is covered by the host's `mag_42_*` tests. The check
names no vanilla records, so it has no `SKYRIM_DATA`-gated case.

### Cure Paralysis, MAG-48

Direct casts and ingestibles execute MGEF archetype 27 by ending each active
archetype 21 application on the recipient. Other effects on that actor and
paralysis effects on other actors remain live. The existing effect-ending path
reverses the ended applications' temporary `paralysis` actor-value changes and
queues their `OnEffectFinish` events, so `World::actor_is_paralyzed` becomes
false when no paralysis application remains. The implementation uses the
existing active-effect save state and adds no save fields.

The local xEdit `MGEF` archetype table identifies 27 as Cure Paralysis. UESP's
`MGEF.wikitext` says this archetype has no extra data and identifies 21 as
Paralysis, which forces the `paralysis` actor value. The CK dump has no
archetype-specific execution page. Its active-effect and dispel pages describe
the live application and finish path reused here. Synthetic
`mag_48_*` tests cover target selection, restoration of control, and the
ingestible path. The check names no vanilla records, so it has no
`SKYRIM_DATA`-gated case.

### Accumulate Magnitude, MAG-53

MGEF archetype 32 accumulates the effect's authored magnitude into its primary
actor value, at `fMagicWardPowerMaxBase` points per unpaused simulation second.
The current value is capped at the effect magnitude. An effect with No Duration
stays active until it is dispelled. Menu time does not accumulate the value.
For the shipped ward records, xEdit names archetype 32 and UESP identifies its
primary actor value as WardPower. The accumulation rate comes from the
`fMagicWardPowerMaxBase` GMST. A missing or invalid setting returns a typed
magic error and leaves the effect's elapsed time unchanged.

The accumulated value uses the actor's existing persistent value components,
so it and the active effect continue from their saved state after reload. No
separate magic save field is added. `mag_53_accumulates_primary_actor_value_on_unpaused_clock_and_caps_at_magnitude`
checks the primary value, paused clock, rate and cap.
`mag_53_accumulated_amount_and_effect_resume_after_reload` checks the saved
partial amount and continued accumulation. The local CK wiki corpus has no
Accumulate Magnitude page describing its rate formula. This implementation
uses the named GMST directly. The source inventory is local
`TES5Edit/Core/wbDefinitionsTES5.pas` and
`uesp-modfileformat/MGEF.wikitext`.

### Demoralize, MAG-30

Direct Demoralize effects use the active-effect lifecycle to lower the target's
Confidence by the effect magnitude for the effect's duration. A live effect
also overrides package and combat movement: the actor drops its combat target
and follows a navmesh route away from the credited caster when one is
available. The actor's package state remains stored underneath the override,
so expiry or dispel lets package evaluation resume. Dead or ghosted hostile
targets follow the existing hostile-hit rejection path.

The local `TES5Edit/Core/wbDefinitionsTES5.pas` MGEF table names archetype 7
Demoralize. The local `uesp-modfileformat/MGEF.wikitext` says its primary actor
value is forced to Confidence. The CK wiki dump has no MGEF archetype page.
Its local `OnMagicEffectApply_-_ObjectReference.wikitext` and
`HasMagicEffect_-_Actor.wikitext` establish the application event and active
effect query contracts. The implementation uses the existing active-effect
save data and reverses the Confidence modifier through the shared actor-value
store. It adds no save field or version.

`mag_30_demoralize_makes_eligible_actor_flee_and_lowers_confidence`,
`mag_30_demoralize_rejects_dead_actor_without_applying_changes` and
`mag_30_demoralize_expiry_releases_flee_override_and_restores_confidence` in
`tests/magic.rs` cover application, target rejection and expiry with synthetic
records. The check names no vanilla records, so no `SKYRIM_DATA` case applies.
Flee movement requires a route on the loaded navmesh. Resistance, visuals and
sound remain outside this row.

### Rally, MAG-59

MGEF archetype 38 uses Confidence regardless of the authored Primary AV. On
application, it raises each selected actor's current Confidence to the effect
magnitude when that value is lower. It does not lower Confidence that already
meets or exceeds the magnitude. The added temporary modifier remains for the
effect duration and the existing expiry or dispel path removes only that
application's contribution. Ordered effect preparation accounts for earlier
Rally entries in the same cast. The implementation follows xEdit's archetype
table and UESP's MGEF notes that PrimaryAV is forced to Confidence, plus the
Creation Kit [Magic Effect](https://ck.uesp.net/wiki/Magic_Effect) description
that Rally maxes target Confidence up to MAG.

`mag_59_rally_modifies_confidence_for_its_duration_and_restores_prior_value`
checks the forced actor value, the magnitude cap, unrelated Health preservation
and expiry restoration. The companion test checks that a target already above
the authored magnitude keeps its prior Confidence through expiry.

### Summon Creature, MAG-39

Direct casts execute MGEF archetype 18 by resolving the effect's associated
item as an `NPC_` base whose effective ACBS flags include Summonable, then
placing one actor at the cast source. The spawned reference records the
credited caster as its owner. Its active effect retains the dynamic reference
identity, and ending or expiring that effect deletes and disables that actor.
A summon reference is saved with the effect in SOEW version 33, so expiry
still dismisses the same actor after loading.

The source inventory is local `TES5Edit/Core/wbDefinitionsTES5.pas`: its
archetype table names 18 as Summon Creature and its associated-item selector
maps this archetype to the actor association field. UESP's local `MGEF.wikitext`
requires that NPC to have the Summonable flag, and `NPC.wikitext` locates this
flag at ACBS bit `0x4000`. The local CK wiki dump has no Magic Effect archetype
page. Its `OnMagicEffectApply_-_ObjectReference.wikitext` defines the
responsible caster argument used by the active effect. Placement, ownership
and dismissal reuse `World::place_at` and the existing effect expiry path.

### Detect Life, MAG-40

Archetype 19 selects loaded, enabled, living actor references within the active
effect magnitude of its target, excluding the target itself. Results are
ordered by active effect and actor identity. Each result carries the winning
MGEF's `Hit Shader` FormID from DATA offset 32, resolved in the plugin that
owns that MGEF. The selection does not load distant actors. Direct
concentration casts are accepted only when every spell effect is archetype 19.
They stay active until dispelled. This does not add charge or magicka-cost
handling. The spell-entry and MGEF conditions are evaluated for each candidate
actor, with that actor as subject and the caster as target. xEdit's local MGEF
definitions supply the archetype and field layout.
The local CK dump has no Magic Effect archetype page with further Detect Life
execution semantics.
`mag_40_active_detect_life_selects_only_live_actors_in_range_with_authored_shader`
checks selection and shader identity. The host adapter converts those results
to Godot coordinates and retains the authored shader FormID for presentation.
The host reconciles active targets into EFSH start and stop requests. The
RenderingServer path displays the authored membrane settings while each target
remains selected. See [godot-host](godot-host.md#detect-life-presentation-mag-40)
for the rendered fields and limits.

`mag_39_summon_check_spawns_authored_actor_with_caster_ownership_and_dismisses_on_expiry`
exercises the row's spawn, ownership and expiry check with synthetic records.
`mag_39_rejects_an_actor_base_without_the_summonable_flag` checks the sourced
ACBS requirement.
`mag_39_saved_summon_keeps_its_actor_link_until_expiry` checks the extended
save state. The check names no vanilla records, so it has no
`SKYRIM_DATA`-gated case. Summon combat behavior, command and banish effects,
visuals, sounds, and host presentation remain outside this check.

### Slow Time, MAG-58

Direct casts execute MGEF archetype 37. The active effect magnitude is treated
as a percentage of time removed, clamped to 100 percent. When multiple Slow
Time effects are active, the strongest one sets the rate. The resulting rate
scales game-calendar advancement and NPC package movement. The real-seconds
clock, its update timers, active-effect durations and Papyrus real-time clock
continue at the supplied tick duration. Menu ticks pause world clocks and
movement while the Papyrus real-time clock continues.

Local xEdit `wbDefinitionsTES5.pas` names archetype 37 Slow Time. Local UESP
`MGEF.wikitext` says it has no archetype-specific extra data. The CK wiki dump
has no page describing this archetype's execution formula, so the percentage
interpretation uses the effect entry magnitude. `mag_58_slow_time_scales_game_time_without_slowing_real_time_waits`
checks calendar progression and completion of an ordinary Papyrus wait. The
menu test checks that menu pause preserves world clocks, completes a Papyrus
menu wait and leaves an ordinary wait asleep. Both use synthetic records, and
the check names no vanilla records.

### Spawn Scripted Ref, MAG-64

A target-location cast whose effects all use archetype 43 resolves the MGEF's
EXPL link and that explosion's placed-object link in their owning plugin
contexts. It places one dynamic reference at the target reference's transform.
The reference uses the placed object's Papyrus class and receives scripts and
properties from that base object's winning VMAD. Its dynamic FormID and VM
object state survive SOEW save/load. An invalid or missing explosion or placed
object returns a typed error before applying the cast. Other target-location
archetypes and mixed-archetype target-location spells remain unsupported.
EXPL DATA accepts the documented 40, 44, 48 and 52 byte layouts. Other lengths
are rejected before placement.

The layout comes from local
`TES5Edit/Core/wbDefinitionsTES5.pas` (`MGEF.DATA` and `EXPL.DATA`) and
`uesp-modfileformat/MGEF.wikitext` and `EXPL.wikitext`. The Creation Kit
[Magic Effect page](https://ck.uesp.net/wiki/Magic_Effect) says Spawn Scripted
Ref uses the explosion's placed object and identifies Throw Voice as an
example. Script attachment follows this crate's VMAD and `Game::bind` path.
`mag_64_spawn_scripted_ref_places_authored_base_with_scripts_and_preserves_identity_on_load`
checks placement, base identity, VMAD attachment and save/load identity with
synthetic records. `mag_64_rejects_truncated_expl_data_before_spawning` checks
that a short DATA payload is rejected before application. The check names no
vanilla records, so no data-gated case applies.

### Command Summoned, MAG-33

Direct casts execute MGEF archetype 10 only for a reference spawned by a live
Summon Creature effect. The command effect temporarily assigns the spawned
reference to its caster and restores its previous runtime owner when the effect
ends. An actor with no active summon application is ignored. The authored NPC_
record is read as usual and is not changed.

TES5Edit's local `wbDefinitionsTES5.pas` names archetype 10 Command Summoned,
and UESP's local `MGEF.wikitext` says it has no additional archetype data. The
local CK wiki dump contains no Magic Effect archetype page. Eligibility uses
the runtime summon identity already retained by MAG-39. SOEW version 37 stores
the prior reference owner with the active effect so expiry after loading still
restores the same runtime state.

`mag_33_transfers_only_a_live_summons_runtime_owner` checks eligible transfer
and that the NPC record remains unchanged. The ineligible actor, effect end
and saved expiry checks exercise ignored targets and owner restoration.

Validation on the installed five-master load order found 1,354 non-deleted
SPEL winners. The direct path accepted 178 spells and created 205 applications.
The survey checks definitions, identity and removal without running their scripts.
The rest returned explicit unsupported reasons. The installed startup still ran
423 quests through 1,388 native calls with zero unresolved calls. Binding now
attached 27,563 scripts, with seven missing scripts and 88 missing properties,
because MGEF script attachments wait until an application exists.

### Werewolf Feed, MAG-47

Direct casts execute archetype 26 against one eligible corpse. The feeder must
resolve to an actor. The target must be an enabled, undeleted actor reference
whose actor state is dead. Feeding deletes and disables that reference, using
the existing saved reference state, and a later cast on the same corpse is
rejected. The feed action creates no active-effect entry.

Local `TES5Edit/Core/wbDefinitionsTES5.pas` and
`uesp-modfileformat/MGEF.wikitext` name archetype 26 Werewolf Feed and specify
no extra MGEF data. CK's local
`ck-wiki/StartCannibal_-_Actor.wikitext` describes feeding on a dead actor.
`mag_47_feed_consumes_corpse_once_and_rejects_a_repeated_cast` and
`mag_47_feed_does_not_consume_a_living_target_or_other_corpse` cover the
eligible corpse transition, repeated use and ineligible targets.

### Werewolf, MAG-57

Direct casts execute MGEF archetype 36 by resolving its associated item to a
winning `RACE` form. The race's `SPLO` `SPEL` abilities and `SHOU` entries are
added to the target's state, constant-effect abilities are applied through
the existing ability path, and the target's current race changes to the
authored race.
Removing or expiring that effect restores the prior race only if the actor
still has the transformed race. It removes only spells and active ability
applications this transform added, retaining pre-existing race and spell
state. Actor-owned transform metadata is saved in the `MAG-57` actor section.

`TES5Edit/Core/wbDefinitionsTES5.pas` names archetype 36 Werewolf and
identifies its related form as a race. The local
`uesp-modfileformat/MGEF.wikitext` independently says the related ID is a
race, and `RACE.wikitext` defines `SPLO` entries as `SPEL` or `SHOU` links.
The cloned CK wiki has no Magic Effect archetype page, so the record meaning
comes from xEdit and UESP. `ck-wiki/Actor_Script.wikitext` describes
`SendLycanthropyStateChanged` as notifying listeners. This executor does not
raise that event. Host appearance refresh is not handled here.

`mag_57_transformation_uses_authored_race_and_grants_its_ability`,
`mag_57_reversion_restores_only_race_and_abilities_owned_by_transform` and
`mag_57_saved_transform_restores_owned_state_after_load` cover the synthetic
record path. The check names no installed records, so it has no
`SKYRIM_DATA`-gated case.

### Cloak, MAG-56

Archetype 35 resolves its associated item as a winning SPEL record. The
associated spell must use concentration casting with aimed or target-actor
delivery. A live Cloak effect uses its authored magnitude as the radius around
the effect target and applies the associated spell to enabled actor references
in that range once per second. Area target selection ignores cell seams.
Each cloak tracks the effect
applications from its latest pulse. A later pulse removes those applications
before creating the next set, and dispelling or expiring the cloak removes its
owned applications.

The xEdit source `TES5Edit/Core/wbDefinitionsTES5.pas` maps archetype 35 to a
SPEL associated item. Local `uesp-modfileformat/MGEF.wikitext` agrees on the
archetype and area fields. The local CK dump has no Magic Effect page. The
published [Creation Kit Magic Effect page](https://ck.uesp.net/w/index.php?section=16&title=Magic_Effect)
specifies the associated concentration spell and says Cloak casts it on area
targets. Its concentration section establishes one-second reapplication.
`area_targets` reuses MAG-06's loaded actor selection. The cloak pulse phase
and owned active-effect identities are saved in the world section `MAG-56`,
without changing SOEW version 37.

`mag_56_cloak_pulses_on_its_one_second_cadence_and_selects_nearby_actors`
checks pulse timing, radius selection and exclusion outside the authored
area. `mag_56_cloak_expiry_cleans_up_only_its_owned_effects_and_survives_save_load`
checks owned cleanup at expiry after save/load. Neither check names a vanilla
record, so no `SKYRIM_DATA` case applies. Host visuals, sound and concentration
input are outside this row.

### Banish, MAG-63

Direct casts execute MGEF archetype 42 against the actor reference selected by
the spell. The target is eligible when it is the live actor reference retained
by an active Summon Creature effect and its actor level does not exceed the
effect magnitude. Banish ends that summon effect through the normal
effect-finish path, which deletes and disables its spawned actor. An ordinary
actor reference, an over-level summon or a different summon is left unchanged.
This does not apply a Confidence value modifier.

The local xEdit MGEF table names archetype 42 as Banish and assigns Confidence
as its primary actor value. The local UESP MGEF table also records that actor
value. The Creation Kit [Magic Effect page](https://ck.uesp.net/wiki/Magic_Effect)
states that Banish affects summoned actors up to the effect magnitude. The local
CK dump has no Magic Effect archetype page. The implementation uses the summon
reference retained by MAG-39 and the existing effect-ending path. It does not
use the actor value as a modifier. Synthetic checks
`mag_63_banish_dismisses_the_targeted_summon` and
`mag_63_banish_leaves_persistent_actors_and_other_summons_unchanged` cover
target eligibility and dismissal. `mag_63_banish_does_not_dismiss_a_summon_above_its_magnitude_level`
covers the level cap. No vanilla records are named by this check, so it has no
`SKYRIM_DATA`-gated case.

### Direct hostile hits, MAG-01

The `magic-hostile` branch accepts hostile effects on live, non-ghost actor
references through the direct cast API. Each applied hostile effect queues
`OnHit` for the target, running reference aliases and its existing active
effects. The arguments identify the credited caster, spell and an absent
projectile, with all melee attack flags false. A non-actor source is retained
as the aggressor when no actor is credited. `RemoteCast` uses its blame actor.
The MGEF No Hit Event flag suppresses the callback without suppressing damage.
No Recast skips both repeated application and its hit callback.

All effects are prepared before hit events or actor-value changes. Unsupported
later effects leave no partial hit. Instant damage uses the shared death path
and queues OnHit before death notification. Timed damage retains caster
identity and the existing unpaused clock, expiry and save behavior.

Sources are local `ck-wiki/OnHit_-_ObjectReference.wikitext`,
`OnMagicEffectApply_-_ObjectReference.wikitext` and the MGEF flags in
`TES5Edit/Core/wbDefinitionsTES5.pas`. CK describes one magic-effect hit per
application and a separate projectile contact. Projectile delivery is MAG-02.
The skymp `ActiveMagicEffectsMap.cpp` was read, but its per-value map and
wall-clock expiry do not provide the per-application hit or shared death
lifecycle needed here. Existing World event recipients and actor-value
mutation are reused. No dependency, host node or save format was added.

The `mag_01_hostile_hit_damages_only_the_target_with_spell_and_aggressor_arguments`
regression checks the cast result and exact event arguments. Companion tests
cover hit suppression, No Recast, rejected targets, atomic failure, alias and
active-effect listeners, RemoteCast attribution and instant/timed death.
Resistance, charging, combat targeting and other archetypes keep
their separate BACKLOG.csv scopes. This step does not establish complete
hostile spell gameplay or host contact detection.

The dependency audit marks MAG rows blocked when an explicit prerequisite is
unfinished. Casting cost now names PRG-08 for perk entry points. NPC abilities
name PRG-11 for leveled spell lists. Command Summoned and Banish name MAG-39
for summon ownership. Cure Paralysis names MAG-42 for the paralysis lifecycle.
Werewolf and Vampire Lord transformations name MAG-13 for automatic abilities.
The CSV remains the task list and stores every item's immediate dependencies.

### Aimed projectile contacts, MAG-02

`World::cast_spell_on_projectile_contact` accepts only an aimed SPEL and an
actor resolved from one projectile contact. A direct scripted cast of an aimed
spell rejects before applying effects. The contact target passes through the
same preparation and hostile-hit path as other spells, preserving caster and
spell attribution and applying the authored effects only to that target. The
host supplies the contact actor identity. This API does not create or simulate
projectile motion.

Local `TES5Edit/Core/wbDefinitionsTES5.pas` and
`uesp-modfileformat/SPEL.wikitext` define delivery value 2 as Aimed.
`ck-wiki/OnMagicEffectApply_-_ObjectReference.wikitext` defines the caster and
effect observed by scripts when the effect is applied. The CK corpus has no
separate magic-delivery page, and `OnSpellCast` describes cast notification,
not projectile movement. No projectile state or save data is added.

`mag_02_aimed_projectile_contact_hits_only_its_hostile_actor_once` checks one
resolved target, one application and one hostile hit with the expected caster
and spell arguments. `mag_02_aimed_spell_requires_a_projectile_contact` checks
that direct casts reject without changing target health or active effects.
Neither check names vanilla records, so no `SKYRIM_DATA` case applies.

The installed five-master survey reads 1,354 winning spells. It accepts 179
and creates 206 applications, including one authored hostile effect. All 24
magic tests pass with installed data. The other spells remain explicit
unsupported results, so this measurement covers the direct cast boundary.

The Godot `PlayerNode::aimed_spell_projectile_contact` resolves the contacted
actor body and calls this API. A live host contact was not exercised in this
worktree.

### Automatic constant abilities, MAG-13

When an actor is first built, constant-effect abilities from its resolved spell
set apply to that actor. `Actor.AddSpell` does the same when it adds an ability
later. Ordinary spells remain known without being cast. Repeated actor access
does not apply an ability a second time. Spawned abilities have no VM object
until a VM path creates one. World save and load retain these headless effects.

Local xEdit `wbDefinitionsTES5.pas` defines NPC `SPLO` as actor effects and
labels spell type 4 as Ability. The CK `AddSpell_-_Actor.wikitext` page says
the native adds the spell to the actor. `mag_13_actor_abilities.rs` covers
NPC initialization, repeated access and world save and load. `natives_dlc`'s MAG-13 adversarial
test drives `AddSpell` through Papyrus and checks the active effect. These
tests use synthetic records and do not include a vanilla-data survey.

### Worn enchantments, MAG-10

Equipping armor applies its ENCH effects to the wearer. Unequipping the item,
or replacing it through an overlapping armor slot, removes only active effects
from that item. The ENCH record supplies effect entries and ARMO's EITM link
names the enchantment. These fields are defined in local xEdit
`wbDefinitionsTES5.pas`. `magic/worn_enchantment.rs` covers apply and unequip.
`mag_10_adversarial.rs` covers removing the replaced source without disturbing
other effects. Weapon enchantments and charge use remain outside this path.

### Magic resistance, MAG-12

Hostile effect magnitude applies Resist Magic first, then the specific
resistance named by MGEF, with each actor value independently capped at 85
percent. MGEF resistance values 40 through 45 use their matching actor value.
44 is Resist Magic and is not applied twice. Each area recipient uses its own
current actor values. A spell with SPIT Ignore Resistance skips both terms.
The same SPIT bit on a scroll means Script Effect Always Applies and leaves
resistance in place, as `uesp-modfileformat/SCRL.wikitext` records.
Non-hostile effects keep their authored magnitude. The
mapping and resistance field are defined in local xEdit
`wbDefinitionsTES5.pas`. CommonLibSSE `include/RE/S/SpellItem.h` confirms the
Ignore Resistance flag. The order and cap are
the documented magic resistance behavior reviewed for this row. The
`mag_12_adversarial.rs` and `magic.rs` tests cover the independent caps and
actor-value modifier behavior with synthetic records.

### Area targeting, MAG-06

`World::cast_spell` no longer rejects a spell for a nonzero effect area. Each
effect entry's own EFIT area (the per-spell radius, not the MGEF's Spellmaking
area default) selects every actor reference within that radius of the cast's
resolved target, always including the target itself. The MGEF `kNoArea` flag
(bit 11 of DATA flags) zeroes the radius for that effect regardless of the
EFIT value, matching the CK Magic Effect flag list. Selection is cell
agnostic, raw Euclidean distance against each actor's position, the same
precedent `World::distance` already used for `GetDistance`, since existing
magic fixtures place actors without a parent CELL group.

An area-selected actor that is not the explicit cast target is dropped from a
hostile effect when it is dead or ghosted, so one ineligible bystander does
not cancel the cast. The explicit target keeps its existing hard rejection on
a dead or ghosted hostile hit. Each selected actor gets its own prepared
effect, active-effect entry and actor-value baseline, so an area hit applies
independently per actor rather than sharing one target's state. `EffectId`
assignment stays a single sequential counter across every recipient of one
cast, preserving existing single-target ID and count behavior.

Sources are local `TES5Edit/Core/wbDefinitionsTES5.pas` for the EFIT
Magnitude, Area and Duration layout, `ck-wiki` Magic Effect pages for the area
and Spellmaking flag semantics, and CommonLibSSE-NG `EffectSetting.h` and
`Effect.h` for the `kNoArea` flag bit and the confirmation that runtime area
reads the spell's own EFIT field, not the MGEF default.

`mag_06_area_hit_selects_each_eligible_actor_once_in_radius` and
`mag_06_area_hit_skips_ineligible_actors_in_radius` in `tests/magic.rs` cover
both done-check clauses with synthetic records: an in-radius actor and the
explicit target both take the effect once, an out-of-radius actor takes no
effect, and a dead hostile-ineligible actor inside the radius is skipped
without affecting the other recipients. The done check names no specific
vanilla record, so no SKYRIM_DATA-gated case was added. Ingestible and poison
area remains its own unsupported entry, out of this row's scope. Concentration,
projectiles and contact and location delivery stay separately unsupported.

### Concussion archetype, MAG-51

xEdit numbers Concussion as MGEF archetype 30. The local UESP MGEF page says
this archetype has no extra data. Its associated-item selector does not assign
archetype 30 a specialized association field. The existing `MagicEffect` view
still decodes the shared DATA values, including archetype, actor values,
casting and delivery values, and effect metadata. Those decoded fields do not
establish which values Concussion execution reads.

The local CK wiki corpus has no Magic Effect archetype page and defines no
Concussion execution behavior. The local source evidence therefore establishes
no live runtime contract, consumed archetype-specific fields or implementation
to port. No execution row is warranted by this characterization.

`mag_51_concussion_named_fixture_decodes_generic_mgef_data` exercises the
shared DATA view on a named synthetic MGEF fixture with archetype 30.
`mag_51_concussion_five_master_corpus_result` surveys the winning records in
Skyrim, Update, Dawnguard, HearthFires and Dragonborn. On the installed data,
the survey decoded 1,573 non-deleted winning MGEFs and found zero archetype-30
records. This establishes absence from that five-master corpus, not absence
from mods or behavior in the original runtime.

### Unnamed archetypes, MAG-68

The five-master inventory on 2026-09-20 reads all 1,573 non-deleted winning
MGEF records from Skyrim, Update, Dawnguard, HearthFires and Dragonborn. None
uses archetype 13 or 14. This is a winning-record result for the installed
masters, not a claim about all modded records or original runtime behavior.

| Value | xEdit label | CommonLib label | Installed winning uses |
|---|---|---|---|
| 13 | Unknown 13 | Darkness | 0 |
| 14 | Unknown 14 | NightEye | 0 |

The source declarations are `TES5Edit/Core/wbDefinitionsTES5.pas`,
`CommonLibSSE-NG/include/RE/E/EffectArchetypes.h` and
`Mutagen/Mutagen.Bethesda.Skyrim/Records/Major Records/MagicEffectArchetype.cs`
in the local reference corpus. CommonLib's corresponding `.cpp` converts
these values to strings but supplies no execution contract. Mutagen omits
both enum entries. The local UESP MGEF page was also inspected and does not
establish their execution semantics. No public implementation was ported.

`mag_68_five_master_winning_effect_inventory_has_no_unnamed_archetypes` prints
the complete winning archetype histogram. Its pre-run acceptance criteria are
more than 1,000 decoded effects and zero uses of 13 and 14. It fails if those
values appear, requiring a fresh characterization rather than assuming they
are inert. `mag_68_unnamed_archetypes_reject_without_changes` independently
asserts an Unsupported error and byte-identical game state for each value.
Both tests passed with installed data using this command:

```text
cargo test -p soe-game --test magic --test magic_archetypes mag_68 -- --nocapture
```

Retain the existing explicit unsupported boundary for both values. The source
names and absent corpus uses do not justify implementing darkness or night
vision. MAG-68 is characterization only and adds no runtime behavior, new
record parser, dependency or save format.

## Host gameplay integration (2026-09-19)

`Game::from_shared_order` and `World::from_shared_order` accept the host's
immutable `Arc<LoadOrder>`. `load_scripts_from_vfs` reads its existing asset
overlay. The owning constructors and data-directory loader remain available.
There is one winning-record table and one authoritative game state.

`Game::activate_deferred` uses ordinary default activation and `OnActivate`
dispatch. For a player load door it arms and commits the doorway, retaining
the source cell until the host installs destination rendering and collision
and calls `settle_unload` on the following step. Existing `activate` callers
keep immediate unloading. Both paths have the same queued cell and activation
event sequence in the synthetic fixture.

Local CK `Activate_-_ObjectReference.wikitext` and
`OnActivate_-_ObjectReference.wikitext` require blocked activation to return
false while still sending the event. A default-only call must ignore the
block and suppress `OnActivate`. These contracts are now applied and tested.
This corrects the old unconditional-success result and blocked default-only
behavior. It does not implement every type's default action.

Host resource preparation, player synchronization, lock restrictions and
remaining rendering limits are recorded in [godot-host.md](godot-host.md).

## Footstep record selection (2026-09-19)

`footsteps::Footsteps` borrows the load order and resolves a selected FSTS,
movement type, animation event tag and MATT surface to IPCT and SNDR identities.
It follows FSTS to FSTP to IPDS to IPCT, retaining both impact sound links.
Every link resolves in the file supplying its winning record. Deleted records,
malformed counts and material-parent cycles return errors. Unmatched event tags
and empty movement groups produce no impact.

Sources are the local xEdit `wbDefinitionsTES5.pas` FSTS, FSTP, IPDS, IPCT,
MATT and DOBJ definitions, UESP's corresponding record pages, and CommonLib's
`include/RE/C/CRC.h` and `include/RE/M/MaterialIDs.h`. FSTS stores its DATA
groups in reverse XCNT order. DOBJ merges keys across sparse override records.
The DLC default-object records do not repeat Skyrim's DFTS entry, so reading
only the winning DOBJ would lose the default footstep set.

Material IDs are CRC32 over lowercase MNAM bytes with initial zero and no
final inversion. This input convention was checked against all 79 installed
material names and CommonLib's declared IDs. The runtime uses `crc32fast`,
already present transitively, whose only dependency is `cfg-if`. No new
packages were added to the lockfile. Hashing retains raw plugin bytes and
stops at NUL. Non-ASCII case folding has not been established by the corpus.
Missing IPDS entries fall back along MATT parent links. That fallback follows
the record hierarchy and has synthetic coverage, but has not been compared
against the original executable's behavior.

The installed five-master test checks wood, stone and dirt, both feet, and
walk, run, sprint and sneak. All 24 cases resolved, supplying 48 sound links.
Sprint uses `FootSprintLeft` and `FootSprintRight`, unlike the ordinary
`FootLeft` and `FootRight` tags. Synthetic tests cover distinct movement
groups, sparse defaults, owner-relative links, parent cycles, malformed counts,
deleted records and three published material hash constants.

The two impact sound slots retain their positions, including null links. The
installed impacts pair NPC sound 1 and player sound 2. Player playback currently
selects sound 2, falling back to sound 1 when absent, based on that content
pattern. Selection across other impact types is not established by this survey.

`World::footstep_sets` reads current race and equipment for each event. The
resolver follows NPC trait templates, NPC skin with race-skin fallback, and
ARMO MODL links to ARMA. Addons match the current race or its armor-parent
chain, including additional races. Links retain the defining file's namespace.
Each body slot selects the addon with the higher sex-specific priority. Skin
is considered before equipment. Silent addons can replace skin parts, and
each surviving FSTS is returned once. If no set survives, the resolver uses DFTS.
The implicit player now retains reference 0:000014 and its Player NPC base,
so skin and race selection work before an explicit placed player is loaded.

Sources are local xEdit NPC_/RACE/ARMO/ARMA definitions, UESP record pages,
and CommonLibSSE-NG `src/RE/T/TESObjectARMA.cpp` for armor-parent matching.
Equal-priority conflicts use later record/equipment order. Deduplicated
layering and tie handling are engine policy pending original-runtime checks.
Equipment is the current actor state's equipped set. `TNAM` armor templates
keep the child `FULL` name, `DATA` value and `EITM` enchantment. They inherit
every other field, including `DATA` weight, `DNAM`, body slots, keywords and
`MODL` armor addons, from the template chain. Each `TNAM` is resolved in the
winning record's defining-file namespace. Cycles, malformed links and missing
or non-ARMO templates reject the consuming operation before it changes state.
The source is the [Creation Kit Armor page](https://ck.uesp.net/w/api.php?action=parse&page=Armor&prop=wikitext&format=json),
retrieved on 2026-09-20. Local xEdit `Core/wbDefinitionsTES5.pas` supplies the
ARMO layout, and CommonLibSSE-NG `include/RE/T/TESObjectARMO.h` confirms the
`templateArmor` relationship and the armor-rating/addon consumers. CMB-11 is
not a dependency because its terminal-record combat lookup does not supply
field-specific inheritance.
Default outfits initialize through the reference path described below. Runtime
skin overrides are not supplied by this path.

Outfit auto-equipping is not supplied by this path. Runtime skin overrides use
the shared appearance view described below. SPID `Skin` distributions are
selected before the NPC's direct skin and the race fallback when assembling an
actor's appearance. The selected `ARMO` keeps its resolved `GlobalFormId`, and
its armor addons enter the same body-part selection as an authored skin. The
load pass resolves distributions by NPC base. Fractional deterministic SPID
rules are evaluated as each actor is built, with the seed combining the parsed
entry value, save character ID and actor runtime FormID. The seeded
Xoshiro256** draw follows CLibUtil `rng.hpp`, which SPID uses through
`LookupFilters.cpp`.
A host can set this input directly with `World::set_player_character_id`.
It is runtime metadata and is not serialized by the engine.

Outfit auto-equipping and runtime skin overrides are not supplied by this path.
The World footstep path supplies the selected trait source for leveled actors.
Direct record-only actor_sets calls still require a non-leveled trait chain.

Ten actor-selection tests pass, including installed-data checks for Nord
bare feet, light boots, heavy boots and wolf skin. The same run resolves all
161 non-deleted race skins into 41 distinct sets across the five masters.
Synthetic cases cover live equipment/race changes, implicit player identity,
sex-specific slot priorities, skin replacement, additional and inherited races,
owner-relative overrides, malformed links and cyclic armor-parent chains.

## Sound preparation (2026-09-19)

`sound::prepare` resolves SOUN markers through their SDSC SNDR links, then
resolves SNDR files, signed frequency shift, frequency and attenuation
variation, SNCT parent gain and menu defaults, underwater muting,
and SOPM output parameters. Slider overrides are explicit inputs. Descriptor,
category and output links use their winning owners. Category cycles, malformed
fields, invalid random draws and unsupported descriptor types return errors.
`sound::prepare_for` evaluates a descriptor's ordered CTDA list through the
shared evaluator. The initiating reference is the subject, and the descriptor
keeps its original global ID so `World::resolve_from` follows the winning SNDR
record to resolve local FormIDs in its file. A false list has no playback
branch. SNAM remains the authored Alternate Sound For link, retained on
`SoundPlayback` without an invented fallback order. Loop modes are preserved.

Sources are local xEdit SOUN/SNDR/SNCT/SOPM definitions, UESP's corresponding pages,
and CommonLib `BGSStandardSoundDef.h`, `BGSSoundCategory.h` and
`BGSSoundOutput.h`. The three caller-supplied random draws select a file,
uniform signed pitch variation and uniform additional attenuation. This
distribution is engine policy, not a measured vanilla random sequence.
Player footsteps draw from the world's saved RNG.

Eight synthetic tests cover SOUN marker descriptor resolution, file/path choice,
pitch and decibels, category
sliders, inherited underwater muting, conditional refusal, invalid inputs,
cycles and output links supplied by overrides. All 48 installed footstep sound
links prepared successfully in the existing 24-case survey. Host execution and
its output-model limits are documented in [godot-host.md](godot-host.md).

## Record-backed music selection (AUD-05)

`music::select` reads a winning `MUSC`, evaluates each candidate `MUST`'s CTDA
list through the shared condition evaluator, and descends eligible MUST
palettes to one leaf. A leaf returns its normalized filename or silent-track
marker, duration, fade-out, loop range and count, cue points, and the parent
music type's fade, priority, ducking amount, and flags. Unsupported condition
functions and malformed links or fields return errors. An abrupt-transition
flag makes the selected fade-in zero. A missing candidate or a palette with no
eligible child returns an error instead of selecting an ineligible track.

The selection draw is supplied by the caller, must be finite in 0 to 1, and
selects among eligible entries in their record order. The chooser does not own
cycle history or save music playback state.

The installed `MUSCombat` winner contains `GetCombatTargetHasKeyword`
(condition function 707), which the shared evaluator rejects under AI-41's
combat-state contract. The installed-data music test surveys for an eligible
track with supported conditions and skips that known unsupported predicate.

## Shader particle and Sound native audit (2026-10-02)

The registry-derived test filters `NativeRegistry::method_bindings` and
`global_bindings` for `shaderparticlegeometry` and `sound`. Its exact expected
sets are `Apply` and `Remove` methods on `ShaderParticleGeometry`, `Play` on
`Sound`, and `StopInstance` and `SetInstanceVolume` globals on `Sound`. It
also checks the absent `Sound.PlayAndWait` and `Sound.GetDescriptor` methods.

The local CK crawl contains neither a `ShaderParticleGeometry` class page nor
named pages for `Apply` or `Remove`. `ATTEMPTS.md` records the attempted CK
routes and archived-page crawl. The local xEdit function data lists the
related condition functions as `Form, Float`. CommonLib's native-function RTTI
records a `float` argument and `void` result. Those sources support the
registered method shape but do not supply a CK description of its argument.
`Apply` and `Remove` take a fade duration in seconds in this runtime. Zero is
the immediate transition. `SCR-46` owns SPGD validation and transient layer
behavior. The host does not yet render those layers, which remains within the
scope of `SCR-46`.

The local vanilla `skse64/scripts/vanilla/Sound.psc` declares four functions:

| Registry binding | Source comparison | Current implementation |
|---|---|---|
| `ShaderParticleGeometry.Apply` method | The CK function page and class declaration were not present in the crawl. UESP lists Play as a form plus float function. | Validates the winning SPGD record and nonnegative finite fade, then applies the transient layer. |
| `ShaderParticleGeometry.Remove` method | The CK function page and class declaration were not present in the crawl. UESP lists Stop as a form plus float function. | Validates the fade and removes the layer immediately or fades it out. |
| `Sound.Play` method | `Sound.psc` declares one `ObjectReference` source and an integer playback instance result. | Returns a fresh instance ID and records the source and sound. It does not route this native call to host audio playback. Physical output verification is tracked by `HST-05`. |
| `Sound.StopInstance` global | `Sound.psc` declares one integer playback instance argument and no return value. | Removes that instance if present. |
| `Sound.SetInstanceVolume` global | `Sound.psc` declares an instance ID and float volume, clamped between 0 and 1, with no return value. | Updates the selected instance and clamps the requested volume. |
| `Sound.PlayAndWait` method | `ObjectReference` source argument, boolean result, latent playback. | Registered as a false-returning stub. Waiting for physical playback completion belongs with `HST-05`. |

## Authored locks and keys (2026-09-19)

Placed references initialize their lock state and level from XLOC, which was
previously ignored. `World::key` resolves the winning reference's KEYM link.
Default door and container activation uses the activator's inventory key,
unlocks without consuming it and continues ordinary activation. Missing or
wrong keys leave the lock and inventory unchanged. BlockActivation prevents
the key action as well as opening. Default-only activation bypasses that block
but still needs the key. The host now delegates lock handling to this path.

`ObjectReference.GetKey` is registered. `Lock` updates shared state, closes an
open object's logical state when locking, and queues `OnLockStateChanged` on
every call, including repeats. `SetLockLevel` clamps to 0 through 255 and queues
the event when the level changes, retaining the current locked state. Level
zero is a novice lock, not an unlocked marker. Existing engine state saves and
hashes already retain the mutable lock fields, so no save layout changed.

`ObjectReference.MoveTo` applies its optional `abMatchRotation` argument. The
Papyrus default is true, so a call without the fifth argument copies the
target's rotation. Passing false preserves the moved reference's current
rotation. Position offsets and destination cell are applied in both cases.

Sources are local CK Lock, GetLockLevel, OnLockStateChanged and Activate pages,
the shipped ObjectReference.psc declarations, and xEdit's XLOC definition.
OpenMW's esm4 loader confirms that XLOC presence initializes a locked object.
Key unlock queues its lock event before OnActivate. That ordering and
out-of-range level clamping have tests but no original-runtime comparison.

Six synthetic scenarios cover key checks on doors and containers, blocked
and default-only activation, repeated lock events, logical closing, native
queries, override namespaces, engine save restoration and load-door unload
retention. A separate installed-data run read all 1,497 non-deleted locked
references across five masters and resolved all 509 non-null key links.
Chest 0:016DB7 refused activation without its key, then unlocked with key
0:03C590 while retaining the key in inventory. There are 146 leveled-lock
flags in that survey. Their level scaling is not executed yet.

UI-08 adds `Game::attempt_lockpick`. It checks for a locked non-key-only
reference, the loaded Lockpick form and a carried pick. Invalid angles leave
the lock and inventory unchanged. A successful turn unlocks through `set_locked`.
A miss removes one pick through the notified inventory transaction. The host
then runs ordinary activation after success. The sweet spot is derived from
the reference FormID. XLOC level narrows the angle window and Lockpicking skill
increases it, with a two-degree minimum. CK `GetLockLevel` defines 0 to 1 as
Novice, 2 to 25 as Apprentice, 26 to 50 as Adept, 51 to 75 as Expert, 76 to
254 as Master, and 255 as Requires Key. xEdit's `wbDefinitionsTES5.pas`
defines XLOC as a level byte, three unused bytes, a KEYM link, a flags byte,
three unused bytes and eight trailing unused bytes. The parser accepts the
documented truncated forms of that structure. These sources define lock-level
ranges and key-only locks, not the menu's angular rule, so the angle behavior
is runtime policy. The `ui_08` tests cover successful unlock and subsequent
activation, failed-turn pick consumption, key-only refusal, malformed XLOC,
and invalid-angle preservation. No saved state was added.

Vanilla minigame feel and timing, key-use messages, crime handling, reciprocal
door lock synchronization and `Lock`'s abAsOwner public/private-cell effect
remain pending. Lock events currently target reference scripts, without alias
forwarding. Logical closing does not animate or update the host collision of
a swinging door. The ordinary unloaded/load-door resource path is unchanged.

`Game::resolve_lockpick` applies a result from a host interaction. It requires
the activator to carry the `Lockpick` item, refuses unlocked and key-only
references, preserves the pick on success and consumes one after a failed
attempt. Success clears the same reference lock state used by Papyrus and
queues `OnLockStateChanged`. The host is responsible for deciding whether the
pick succeeds. The resolver does not calculate difficulty, skill, leveled
lock scaling or lockpick durability. HST-03 tests cover successful and failed
results and key-only refusal.

## Vanilla reference placement restoration

`World::restore_vanilla_reference_placements` applies saved identities, bases
and transforms to a fresh reference table, reserves dynamic IDs and restores
the player location without queuing events. It validates the entire pass
before changing the world. Its report identifies missing dependencies and
unapplied state. Inventory, actor values, extra data and saved execution remain
separate requirements. PCON saved placements use the reference table and
record-backed base queries. xEdit does not bind PARW type 47 to an initial
placement prefix, so PARW placement is not applied. Runtime reference state
uses the existing SOEW reference encoding. See [save.md](save.md#reference-placements) for
sources, validation and the limits of this intermediate reconstruction pass.

`World::restore_vanilla_spatial_records` applies decoded CELL common flags,
cell flags, full name and mapped owner links to shared record overrides. LCTN
and ECZN common flags update their record headers. LCTN and ECZN placeholder
payload tails stay opaque and persist in the named `SAV-28` world section.
The pass is atomic, refreshes warm CELL state and persists records through the
existing record-override table in SOEW. CELL ownership queries use the mapped
override link after reload.

For reference headers with the generic form-flags bit, the pass retains both
mask words in its report and XORs the first word with winning-record flags
before deriving enable and persistence. xEdit names the first word `Mask
Invert Flags`; the second word is `Mask Set unk010` and remains opaque. The
pass accepts Special Edition ESS header versions from 12 and matching file and
change-form versions from 77. Tests compare authored reference state before
and after restore for header versions 12 and 13, form versions 77 and 78, all
four authored enable/persistence combinations, and varied values in both saved
words.
Deleted winning records remain disabled regardless of this mask.
The effective persistence value feeds cell-unload retention and
encounter-zone reset decisions. SOEW version 37 stores it with each reference.
Older SOEW states derive persistence from the current winning record.
The test `sav_54_reference_mask_words_invert_enable_and_persistence_across_supported_versions`
checks those before-and-after cases, the persistence query and engine save/load.

## Inventory instances and ESS application, work in progress

`RefState::inventory` now holds one canonical per-item total and ordered
extra-data lists. The previous count map could not retain a tempered item,
charge, owner, equipped marker or custom name after restoration or transfer.
`Inventory` checks quantity arithmetic and keeps mapped form identities,
string bytes and floating-point bits in its stack fields. Stack fields retain
information without claiming that every extra-data type already drives gameplay.

Saved extra-list counts are not always a partition of the item total. The
corpus contains overlapping positive lists and negative ExtraLeveledItem
bookkeeping under zero-total entries. Those descriptions are preserved, while
the item total bounds every removal. Ordinary removal uses undescribed items
first, then positive stacks in saved order. Negative bookkeeping stays with
the source. Transfers clear Worn/WornLeft on received stacks. This selection
policy has not been compared with vanilla's selection among otherwise similar
instances. Ownership changes, theft rules, equipment events and restocking
remain unfinished.

Crafting prepares its changes on a cloned inventory and commits the complete
result only after eligibility and quantity checks pass. Consumed stacks lose
their extra data with the items. Unspent stacks keep it. Missing recipients,
self-transfers and recipient overflow do not remove anything from the source.

`World::restore_vanilla_inventories` reconstructs authored quantities and
applies saved deltas, with the ExtraLeveledItem authored-count exclusion from
CommonLib. Saved RefIDs in extra fields map through the saved plugin list.
Count defaults to one when ExtraCount is absent. Every reference is prepared
before any inventory is replaced, with no inventory events or random template
selection. Saved actor template choices and created base records must exist
before their inventories can be reconstructed. Item definitions absent from
the current table are reported. General reference extras remain unapplied.

SOEW version 12 appends stack descriptions to the existing quantity prefix.
Versions 4 through 11 remain readable. The extension rejects duplicate item
or reference keys, invalid field tags, excessive nesting and truncation before
replacing world state. Inventory hashes include the complete stack extension.

Sources read were local CommonLibSSE-NG `TESObjectREFR.cpp`,
`InventoryEntryData.cpp`, `ExtraDataList.cpp`, `ExtraCount.h` and
`ExtraLeveledItem.h`, plus the save layout sources in save.md. Existing world,
form mapping, engine-save IO and inventory event paths are reused. No new
package or host node was added. Full ESS loading, actor inventory restoration,
item-use effects, equipment combat, tempering and menus remain open.

The 60-save non-actor inventory survey restores 59,854 inventories, 402,177
item entries and 165,799 extra-data lists with no missing item definitions.
Engine save/load preserves every surveyed inventory and the world hash. The
combined game-side vanilla-save suite passes 28 installed-data tests. These
results do not cover actor inventories or prove vanilla stack-selection rules.

## Restoring NPC records

`World::restore_vanilla_npcs` applies standalone NPC_ changes before reference,
actor-base or actor construction. It prepares all records and mappings before
installing any, and queues no events. It rejects duplicate, missing, deleted
or non-NPC targets and malformed payloads. Missing mapped link definitions are
reported without replacing their identities with other forms.

Runtime overrides are records keyed by the same resolved FormID as their
plugin origin. `World::record`, NPC views, names, actor initialization and
inventory-source lookup see the override. The immutable `LoadOrder` remains
the authored input, shared with the host. Unchanged links retain their winning
plugin's namespace. Changed links use private relocation tokens, allocated
outside every four-byte window in the original subrecords and resolved by
`World::resolve_from`. This retains opaque original data without inventing a
plugin or rewriting its master list. A null raw link resolves to None.

ACBS, AIDT, DNAM, factions, spells, class, race, face, gender and outfit changes
become their corresponding NPC subrecords. Saved names retain Windows-1252
bytes and are treated as plain text even if the winning plugin is localized.
Form and NPC text queries reuse the ESM Windows-1252 decoder. Shouts initialize
the actor's learned shout collection separately from spells. SPID shout
distributions add to that same collection during actor initialization, with
repeated forms stored once. The collection is written in SOEW version 3 and
later. PRG-11 expands leveled spell lists during actor initialization, as
described below.

CommonLib names originalRace and bodyTintColor as runtime NPC fields. The
original race and exact four tint bytes are retained alongside the record.
QNAM receives normalized RGB, following CommonLib NiColor's byte conversion.
Morph floats retain their exact bits. Generic form-flag masks are preserved,
reported and serialized, but remain unapplied. No such masks occurred in the
60-save NPC survey. The absent-head face case follows xEdit and has only
synthetic layout coverage. Rendered NPC appearance has not been verified.

SOEW version 13 introduced overrides after the version-12 inventory extension.
Engine load reconstructs them against the same plugin order and validates
origins, identity ranges, link-token collisions, duplicate records, editor
identities, bounds and NPC structure before changing the world. Old saves
clear the override table. World hashes include its complete serialized state.

Sources read were xEdit's NPC_ and changed-NPC definitions, FallrimTools
ChangeFormNPC, CommonLibSSE-NG TESNPC and NiColor, and OpenMW's TypedDynamicStore.
OpenMW informed the separation of authored and runtime records, not Skyrim's
field layouts. Existing record, reference mapping, save IO, text decoding and
actor initialization code is reused. No dependency or host node was added.

All 13,362 standalone NPC bases across 60 saves restore into shared records and
survive engine save/load with no missing mapped links. The combined installed
vanilla-save suite passes 38 tests with NPC restoration before the existing
template and inventory surveys. Synthetic cases cover reordered regular and
light plugins, a winning override with a different master namespace, untouched
opaque bytes, non-ASCII names, live actor queries, Papyrus health, restoration
before actor initialization, failed late imports and old saves clearing state.
The temporary-base follow-up below adds inline NPC application. Complete
actor state, save resumption and saved package execution remain open.

## Restoring light and static base objects

`World::restore_vanilla_base_objects` maps LIGH, STAT and MSTT changes through
the saved plugin list, validates every target and payload, then installs all
records together. The shared record lookup and name query continue to read the
winning record's fields. `World::restored_base_object_extra` exposes the saved
form-flag words and the value and full-name changed markers. xEdit defines the
last two markers as zero-byte fields, so they carry no replacement value or
name. These fields persist in SOEW version 37 and participate in the world
hash.

## Temporary actor bases

The template-restoration pass now materializes a private NPC record for every
saved leveled actor before actor initialization. Each template group comes
from the validated saved chain. ACBS flags and fields and the DNAM fields
follow their group ownership. The record keeps the original editor identity,
removes TPLT, marks the copied-template bit and receives a distinct dynamic
FormID. Inline NPC changes then apply to that private record. Original and
selected plugin bases remain unchanged.

Known FormID fields are relocated using each component's source namespace.
This includes script object properties, inventory ownership, keywords, attack
data and alternate textures. Opaque fields stay with their original source.
The base-data source supplies localized text provenance. Actor initialization,
inventory, keyword queries and footstep selection read the resulting record.
Papyrus GetActorBase retains the authored base, while GetLeveledActorBase
returns the private base. New engine spawns materialize it on the first such
query. Imported saved choices all receive their bases during restoration.

The entire import prepares records, choices and dynamic IDs before installing
any. Bad later entries and exhausted IDs leave the world unchanged. Sorting
actors before allocation makes saved change-form order irrelevant. The pass
does not consume RNG or queue events.

SOEW version 14 adds record origin, text origin and owning actor identity.
Load checks the owner against its saved template chain, rejects collisions
and IDs beyond the allocator, and reconnects each choice to its private base.
Versions 4 through 13 remain readable. World maps now serialize in sorted
order, fixing byte differences after a save/load cycle caused by hash-map
iteration. Saved record metadata and associations contribute to the world hash.

Sources were local UESP NPC template descriptions, skymp NPC_.h, xEdit NPC_
and alternate-texture layouts, CommonLib's ACTOR_BASE_DATA and
ExtraLeveledCreature declarations, and CK GetActorBase/GetLeveledActorBase.
Existing VMAD parsing/writing, record overrides and actor initialization are
reused. No dependency or host node was added.

The 60-save corpus creates and persists 156,169 temporary actor bases and
applies all 4,930 nonempty inline NPC changes. Applicable saved DNAM health
values reach live actor initialization. Synthetic checks cover separate
stats for actors sharing an authored base, Papyrus base queries, component
links across different plugin namespaces, script properties, inventory
ownership, footsteps, stable save bytes, version-13 migration and atomic
rejection of corrupt identities and exhausted allocation.

Group composition follows the public field descriptions. It has not been
compared with a running vanilla executable for every flag or modded layout.
Temporary-base cleanup on reset, deletion and respawn, saved Papyrus rebinding
to vanilla temporary base IDs, generic form-flag application, rendered
appearance and complete actor/AI execution remain unfinished. Passing these
tests does not establish a complete ESS load or a resumed game.

## Death lifecycle worktree

The unmerged combat-lifecycle increment routes native kills, lethal value
changes and supported magic through one transition. SOEW version 15 preserves
held requests and undelivered notifications. Actor, running alias and active
effect scripts receive death events. See [actor values](actor-values.md#death-lifecycle-increment-unmerged)
for sources, event timing, validation and the remaining combat requirements.

## Ordinary melee worktree, unmerged

The combat-hits increment evaluates ordinary physical contacts from winning
weapon/armor records, current skills, equipment and game settings, then queues
OnHit before any death notifications. It builds on the frozen lifecycle review.
Host hit detection, special attacks, perks and full combat remain unfinished.
See [physical melee execution](combat.md) for supported rules and sources.
Ranged release reads the selected bow or crossbow and equipped compatible AMMO
form from actor equipment. A valid release consumes one round, queues the
ordinary item removal notification, and returns a transient PROJ flight
request to the host. `FlightMotion` advances that request in fixed time steps
using its record gravity, speed and range. Flight state is transient and does
not enter the save. See [ranged release](combat.md#ranged-release-cmb-08) and
[projectile flight](combat.md#projectile-flight-cmb-22) and
[recovered arrows](combat.md#recoverable-missed-arrows-cmb-23).
The combined full gate passes 1,163 Rust tests and 11 Python tests, with no
warnings or skipped optional tools. Installed-data validation passes 12 melee
and 11 death-lifecycle tests. It decodes 3,262 winning weapons and 3,675 armors,
then executes 184 supported weapons against synthetic actors. The remaining
weapons reject unsupported types, templates or enchantments. Real armor records
were decoded, while armor damage calculations use synthetic fixtures.
These changes await commit and merge review on `combat-hits`.

## Vanilla calendar restoration

The global import initializes `GameDaysPassed` and `TimeScale` in the live
clock while retaining `GameYear`, `GameMonth`, `GameDay` and `GameHour` as
separate form-backed values. `World::calendar` is a read-only view of those
records, not another authoritative state table. Gameplay time conditions and
package schedules use its hour and date. Game ticks advance these globals
before firing timers or evaluating packages. Month rollover uses the fixed
month lengths in CommonLib Calendar.h, including 28 days in Sun's Dawn.
Menu pause and a zero timescale leave the calendar unchanged.

Import validates all six values whenever the saved table contains a calendar
field. Missing fields, non-finite values, invalid dates or hours, negative
elapsed days and negative timescales fail before any globals change. Ordinary
non-calendar globals retain their existing bit-preserving import contract.
Saved hour and elapsed days are not forced into agreement. The global native
setter also keeps elapsed days and timescale synchronized with the clock.
The existing global and clock engine-save fields preserve this state, so no
SOEW version change is needed.

Sources read were UESP `Global_Variables.wikitext`, the local CK
`Utility_Script.wikitext`, shipped `Utility.psc`, and CommonLibSSE-NG
`Calendar.h` and `Calendar.cpp`. The backlog's named CK GameHour page is not
in the local corpus. CommonLib provides the separate global identities and
month lengths, and CK equates GetCurrentGameTime with GameDaysPassed. Existing
save decoding, identity mapping, global import, ticking and serialization are
reused. No parser, dependency or host node was added.

SAV-08 has no backlog prerequisite. It establishes calendar values before
execution. SAV-18 separately restores scheduler epochs and deadlines. Calendar
import does not execute queued messages, fire timers or resume quests.
`restore_vanilla_papyrus_execution` separately restores instances and schedules
validated active Papyrus frames. Full ESS loading and host resumption remain
separate backlog items.

The native game save stores the world clock and the VM clock state together.
Papyrus waits retain their real-time, menu-time or accumulated game-hour
deadline, so a reload does not reinterpret one clock domain as another.
`tests/sav_18_wait_deadlines.rs` exercises each wait through a game save and
load, including a timescale change and one-shot wake behavior. The CK
`Utility_Script.wikitext` documents `WaitGameTime` in game hours, while UESP
`Global_Variables.wikitext` defines saved globals as form IDs and floats. The
runtime scheduler uses elapsed game hours from the saved game clock, not the
calendar's `GameHour` field.

The installed survey passes all 60 saves and all 360 calendar values, with
exact engine-save bytes and world hashes after reloading. Synthetic checks
cover plugin remapping, independent saved hour and elapsed days, a queued VM
call and a due game-time timer left untouched by import, native time queries,
time conditions, package schedule dates, atomic malformed-value rejection,
month/year rollover, menu pause and zero timescale. The survey reads the
external saves in place. No comparison with a running vanilla executable was
performed for tick-level floating-point rounding or calendar advancement.

## Saved ownership and locks, SAV-16 and SAV-69

The reference-extra pass writes the shared owner, locked bit and
lock level. GetActorOwner, GetFactionOwner, IsLocked and GetLockLevel observe
those values immediately. GetKey and default door/container activation prefer
the saved key, including explicit null, over the authored XLOC link. The
activator's key is retained after unlocking. Restoration queues no events. A
synthetic restoration check applies placements first, then extras, before
checking native queries and keyed activation.

Rank and global ownership metadata and unassigned lock fields persist in the
SOEW version 20 extension and contribute to the world hash. Older saves
retain authored-key behavior. The decoder accepts the observed Sequence and
Stacked PackageData frames, including extras after those frames. Unknown
layouts still fail before the importer changes the world.
See [save.md](save.md#reference-ownership-and-locks-sav-16-and-sav-69) for sources
and the extra-list boundary. SAV-69 checks every top-level list in all 60
external saves, saved lock bytes against authored XLOC levels, and restoration
of 12,591 references with 2,228 ownership changes and 10,404 locks. The
restoration survey compares decoded owner, rank, global, key, level, flags
and retained words with live state, then verifies exact reference state,
world hashes and bytes through engine save/load without queued events.
No new saved fields, host nodes or dependencies were added. Leveled-lock
scaling, ownership crime rules and lockpicking remain separate mechanics.

## Vanilla actor-value restoration boundary

SOEW version 19 preserves base, permanent, temporary and damage components
separately. Importing the same components from vanilla ESS remains blocked in
SAV-06 because their serialized ACHR boundaries are not established. SAV-64
tracks that source prerequisite. See [saved actor-value modifiers](save-actor-values.md)
for the source audit, all-save coverage and identity requirements.

## Actor condition batch AI-01

Both the gameplay CTDA evaluator and the package evaluator now execute
GetActorValue (14), GetStaminaPercentage (128), GetBaseActorValue (277),
IsKiller (408), GetHealthPercentage (430) and GetPermanentActorValue (494)
through one helper over the existing actor store. Current includes every
modifier, base excludes them, and permanent includes only base and the
permanent modifier. Health and stamina fractions reuse the Papyrus percentage
contract, including zero maximum and exhausted values. IsKiller reads the
existing death attribution and requires a dead actor. Missing, deleted and
non-actor subjects return explicit errors, and package predicates fail closed.

Sources are local xEdit `wbConditionFunctions`, CommonLibSSE-NG
`include/RE/A/ActorValues.h`, and CK `GetActorValue`, `GetBaseActorValue` and
`GetActorValuePercentage` pages in `engine-refs/ck-wiki`. The
[CK IsKiller contract](https://ck.uesp.net/wiki/IsKiller) names the dead actor
as subject and the possible killer as parameter. The
[SKSE command reference](https://skse.silverlock.org/vanilla_commands.html)
describes the permanent-value query as excluding temporary modifiers.
The condition index table covers all 164 CommonLib actor values, with the
existing store spellings for marksman, speechcraft and critchance. This does
not widen the separate magic effect actor-value allowlist.

AI-86 retains IsKillerObject (409) and GetXPForNextLevel (530). xEdit and
SKSE name a FormList argument for 409 without defining what it matches. The
[Experience author](https://www.nexusmods.com/skyrimspecialedition/articles/7028)
describes 530 as a placebo vanilla command fixed by that mod. These queries
remain unsupported pending a Skyrim contract. They are not counted as
implemented predicates.

## Magic condition batch AI-61

The gameplay condition evaluator checks spell keywords (693) against the
current spell's KWDA, spell skills (696) against each linked MGEF's magic-skill
actor value, and SpellHasCastingPerk (713) against the SPEL half-cost perk.
These three functions require the active spell context supplied while SPEL
effect and MGEF conditions run. HasMagicEffectKeyword (699) checks live active
effects on the subject. HasBoundWeaponEquipped (706) uses Casting Source 0 for
the left hand and 1 for the right, then checks the equipped WEAP bound flag.
Voice source 2 is valid and returns false because it has no bound hand weapon.
other casting-source values return a parameter error.
IsPoison (721) reads the subject reference's ALCH base and its poison flag.
EffectWasDualCast (724) reads optional caller-supplied cast context. Without
that value it returns a typed magic-context error. The direct scripted cast
path supplies false because it has no hand-cast dual-cast input. Function 681
remains unsupported because the local xEdit and UESP tables identify its name
and Actor Value parameter but the CK dump has no condition-function semantics
for its skill-advancement entry point.

Local xEdit `Core/wbDefinitionsTES5.pas` establishes the function IDs and
parameter types. UESP `Raw_Function_Data.wikitext` identifies IsPoison as the
poison-potion query. The CK `Magic_Effect` page supplies the shared spell/MGEF
condition context and swapped subject/target behavior. The CK conditions page
is absent from the local dump, and the available sources do not define an
original-runtime dual-cast call path. No state is saved or duplicated.

`ai_61_magic_conditions.rs` covers positive, negative and invalid-context
cases, including unsupported diagnostics for 681 and missing dual-cast state
for 724. The fixtures use synthetic records. No installed-record survey is
claimed.

## Dialogue condition batch AI-48

GetDialogueEmotion (434) and GetDialogueEmotionValue (435) require a reference
subject, then read the emotion type and 0 to 100 value from the current response
of the active dialogue INFO for its speaker. They return zero when that
reference has no current response.
IsSceneActionComplete (550) resolves its SCEN and action slot and reads the
scene's completed-action set for the current phase. IsInFavorState (635) reads
the actor's existing `doing_favor` flag. Invalid scene slots and non-actor
favor subjects return typed errors.

GetNoRumors (320), IsTalkingActivatorActor (370), IsScenePackageRunning (429)
and IsInScene (590) return an unsupported-function diagnostic. xEdit and
UESP name those conditions but do not establish a usable runtime contract for
the state this engine currently tracks. The CK page for IsInDialogueWithPlayer
confirms actor and talking-activator dialogue participation, but does not
define condition 370's actor parameter. The implementation adds no parallel
scene, rumor, activator or dialogue-emotion state.

Sources read: local xEdit `wbDefinitionsTES5.pas`, UESP `Raw_Function_Data.wikitext`
and `INFO.wikitext`, and CK `IsInDialogueWithPlayer_-_ObjectReference.wikitext`,
`Actor_Script.wikitext` and `SetDoingFavor_-_Actor.wikitext`. `IsDoingFavor`
defines the shared actor favor flag, and the INFO response layout defines the
emotion fields. The condition-name sources give no further semantics for the
four diagnosed functions.

`ai_48_dialogue_conditions` checks positive and negative response emotion,
scene action and favor values, invalid actors and slots, and explicit errors
for the four conditions without a usable local contract. No saved state or
save version changed.

## Compatibility condition batch AI-42

GetSKSEVersion (1024), GetSKSEVersionMinor (1025), GetSKSEVersionBeta (1026)
and GetSKSERelease (1027) are parameterless condition functions. The engine
returns zero for each component because it does not load an SKSE runtime, so
conditions can test for SKSE absence. Nonzero integer parameters are errors.
The CK corpus has no pages defining these SKSE-added condition functions.
ClearInvalidRegistrations (1028) is also parameterless in xEdit and Mutagen,
but SKSE documents it as cleanup of invalid update registrations rather than
as a query. Evaluating it returns `NoConditionContract` instead of mutating VM
registrations from a condition. All five still require the evaluator's normal
run-on subject context.

The function IDs come from xEdit `wbDefinitionsTES5.pas` and the parameter
types from Mutagen Skyrim condition data. SKSE's save-load hook and change log
document registration cleanup. The source corpus defines no CK return contract
for the added checks, so the zero-version result expresses that SKSE is absent
from this runtime rather than claiming an SKSE version.

## Location condition batch AI-54

The condition evaluator supports GetWantBlocking (0), GetLocked (5),
GetInSameCell (32), GetLockLevel (65), IsPlayerInRegion (117),
IsInMyOwnedCell (146), HasSameEditorLocAsRef (180), and
HasSameEditorLocAsRefAlias (181). They read shared actor and reference state,
the player's current cell region list, CELL ownership, authored editor
locations, and the named quest alias. GetWantBlocking reads the actor's live
block stance also used by combat. Region membership reads the CELL `XCLR`
list. Editor-location matching applies the supplied keyword to the common
location. No condition adds state or changes save data.

xEdit `wbConditionFunctions` supplies the function identities and parameter
types. CK `GetLockLevel` and `IsLocked` define the corresponding reference
values. The remaining CK function pages are absent from the local corpus, so
their behavior follows the named condition and its typed record fields.
Missing actor, reference, player-cell, quest-alias, or form context returns a
typed condition error. A malformed unused parameter is rejected. Synthetic
coverage is in `tests/ai_54_location_conditions.rs`.

`condition_actor_values` checks both evaluators against independent expected
values, high indices, invalid indices and subjects, resource fractions and the
existing kill/resurrection lifecycle. No new authoritative actor state,
dependency or save format was added.

## Player identity and faction conditions (AI-80)

The shared CTDA evaluator implements GetPCIsSex (131), GetPCInFaction (132),
SameFactionAsPC (133), SameRaceAsPC (134), SameSexAsPC (135),
GetPCEnemyofFaction (197) and GetPCFactionAttack (199). Player sex uses the
resolved player NPC traits, faction membership uses the player's faction
entries, and the Same* conditions compare the subject actor with the player.
Enemy state reads the faction's `SetPlayerEnemy` value. A successful player
melee hit records each faction membership of its target, and the attack flag
is retained in SOEW version 36. Older saves read that flag as false.

The local xEdit `wbConditionFunctions` table supplies the indices and
parameter types. CK `IsInFaction`, `GetFactionRank`, `GetRace` and
`ActorBase.GetSex` pages establish the underlying actor queries. The local CK
corpus does not define a usable contract for GetIsClassDefault (228), so the
evaluator returns `Error::Function(228)` and does not list it as supported.
`ai_80_player_conditions.rs` checks positive, negative and invalid-context
cases, including a successful attack, a no-attack faction and save/load.

## Reference state conditions AI-76

The gameplay CTDA evaluator implements IsProtected (476) from the subject's
NPC base state and PlayerKnows (493) for spells held in the player's spell
set and the player's known-form set for Magic Effects, Words of Power and
Enchantments. IsProtected rejects
parameters and non-actor subjects. PlayerKnows rejects malformed forms and
extra parameters, compares known forms with the matching player state, and
returns a state contract diagnostic for other form types. The `AI-76` actor
save section retains known forms across save and load. For an Enchantment, the
caller must query the base Enchantment that the player knows.

The local xEdit `wbConditionFunctions` table declares these function indices
and parameter types. CK's `ActorBase_Script.wikitext` documents the protected
flag, and `PlayerKnows_-_Form.wikitext` describes the form Known flag for
magic effects, words of power and enchantments. The CK page defines a broader
set of knowable forms than the current actor spell state can answer.
GetAllowWorldInteractions (503), IsWin32 (521), IsGoreDisabled (547),
GetActorsInHigh (554), HasLoaded3D (555) and GetCurrentDeliveryType (572)
remain explicit unsupported condition diagnostics. xEdit declares their
names and parameters, while the local CK corpus does not define usable
condition contracts for their settings, process tier, host 3D state or active
cast state. No parallel state or save data is added.

`ai_76_reference_conditions` checks positive and negative results for the
available contracts, invalid form and parameter cases, missing run-on context,
and explicit diagnostics for the uncontracted functions. The adversarial MGEF
case checks a known effect and its saved state.

## Motion condition batch AI-62

`GetPos` and `GetAngle` read the subject reference's current placement by
axis. `GetStartingPos` and `GetStartingAngle` read its authored `REFR` or
`ACHR` transform, so runtime movement does not change those results. Position
values use game units. Angle values convert the record's radians to degrees.
All four functions accept axes 0 through 2 and reject other axes or a nonzero
second parameter. Missing references and authored transforms return form
diagnostics.

`GetSleeping` returns the actor's modeled sleep state: 0 when idle, 2 when its
current package is Sleep and it has not reached the bed, and 3 while the AI
driver marks it sleeping. The runtime does not distinguish the CK's transient
loading-sleep-idle state 1 or wants-to-wake state 4. Non-actor subjects return
a form diagnostic. `IsMoving`, `IsTurning` and `IsRotating` validate their
reference context and then return an unavailable-state diagnostic because
the world does not track those motion flags.

The contracts follow local xEdit `wbDefinitionsTES5.pas` and UESP
`Raw_Function_Data.wikitext` parameter declarations, CK's ObjectReference
position and angle pages for values and units, and CK's `GetSleepState` page
for the documented sleep states and `GetSleeping` loading-idle note. The
`ai_62_motion_conditions` tests cover runtime versus authored placement,
degree conversion, supported sleep states, invalid inputs and unavailable
motion flags.

## Motion condition batch AI-64

`IsSwimming`, `IsSneaking`, `IsRunning` and `IsRidingMount` return the
subject actor's current modeled movement flags. The running flag means running
as opposed to walking or sprinting, matching CK's `IsRunning` page. These
flags default to false and persist with actor state. `IsPlayersLastRiddenMount`
compares the subject actor with the world's recorded last ridden horse.
Non-actor subjects and nonzero parameters return form and parameter errors.

`GetAnimAction`, `GetPairedAnimation` and `GetIdleDoneOnce` accept no
parameters, validate a live reference, then return an unavailable-state
diagnostic because no current animation action or completion state is tracked.
Their CK condition contracts are absent from the local CK corpus. The local
xEdit table and UESP raw function table confirm their IDs and no-argument
signatures. The UESP note for paired animation names a query but does not
define its condition result.

The contracts follow local xEdit `wbDefinitionsTES5.pas`, UESP
`Raw_Function_Data.wikitext`, and CK's actor pages for `IsRunning`,
`IsSwimming`, `IsSneaking`, `IsOnMount` and
`IsPlayersLastRiddenHorse`. `ai_64_motion_conditions` covers true and false
results, actor and non-actor contexts, both invalid parameter slots and the
three unavailable animation queries. SOEW version 36 stores the swimming,
running and mount flags, with older saves loading them as false.

## Legacy condition batch AI-53

IsPS3 (520) has no parameters and returns false in this Skyrim Open Engine
runtime, which does not run on PlayStation 3. The evaluator rejects nonzero
parameter slots and invalid run-on context. Both the gameplay condition and
package evaluators use this result.

The VATS area-free functions 516 to 518 and target-visible functions 522 to
525 remain unsupported. xEdit identifies their first parameter as a placed
reference. The local CK function reference does not specify the result
semantics, and this runtime has no VATS selection or targeting state to query.
Gameplay condition evaluation returns `Error::Function` for these functions.
Package conditions fail closed and increment the existing unsupported-function
diagnostic.

Sources are xEdit `wbDefinitionsTES5.pas` and the CK wiki's
`Function_Reference.wikitext`. Mutagen's local `ConditionDatas.xml` confirms
which VATS condition fields store a reference and marks the other fields as
unused. The `ai_53_legacy_conditions` tests cover platform comparisons,
parameter and run-on validation, both evaluator paths and VATS diagnostics.

### Sequential package validation

AI-02 has synthetic tests for child order, nested and root repeats, early
package completion, skipped conditional subtrees, current-package conditions,
Once Per Day, Travel before Sleep, save/load on the second child, malformed
child counts, empty repeats and unsupported groups. The existing package
regressions remain applicable. The installed Skyrim.esm survey decodes 5,961
packages with no failures under the new child-count validation. Its 3,966
supported first procedures are an entry-point count, not complete tree coverage.
The earlier strongest-procedure survey above describes the old heuristic.

### Random package branches

AI-13 runs one eligible immediate child of a Random branch. Conditions are
checked once per candidate, and the selected child does not recheck a random
condition while entering. After that child finishes, the Random branch ends
or chooses again if Repeat when Complete is set. A selected Sequence runs all
its children before returning to Random. Success Completes Package still ends
the whole package. Empty repeating groups yield until the next driver tick.

Selection draws from the existing world RNG, whose state is saved with the
world. The AI-02 cursor already identifies the chosen subtree and active leaf,
so no new save version or parallel authoritative state is needed. Restoring
an active child resumes its path without consuming another selection draw.
The static `procedure_of` helper returns None when its first leaf depends on
a random choice. It does not consume the runtime RNG for a preview.

Local xEdit PACK PRCB and the UESP PACK layout supply the group representation.
The [AI Overhaul author's two-location example](https://www.nexusmods.com/skyrimspecialedition/articles/2467)
and the [random travel authoring example](https://forums.nexusmods.com/topic/804089-npc-random-exploring/)
supply the single-choice and repeat behavior. These sources provide no reusable
interpreter. Choice uses the engine's existing deterministic RNG and equal
candidate weights. Exact Bethesda RNG output remains unverified.

Tests extend `package_sequences.rs` with single-child selection, candidate
conditions, nested sequences, repeat, empty groups, one condition evaluation,
and save/load equality of both movement and the subsequent RNG draw.

### Wait package procedure

AI-34 resolves ordered Wait parameters through the package template chain.
The first PKC2 input is ActualSeconds, a float, followed by StopMovement,
a Boolean. Public input names do not control execution. Package overrides
take precedence over template defaults. The installed Skyrim.esm survey found
87 Wait branches, all with this float-then-Boolean order, including renamed
inputs. Local xEdit PACK ANAM/CNAM and PKC2 definitions supply the value and
binding layouts. The existing ESM and PACK readers perform the survey.

Positive durations finish on the first driver tick at or after their deadline
in the saved world real-seconds clock. Menus pause that clock, and game
timescale does not change it. Entry starts the timer at the current clock,
so the earlier part of that tick is not counted as waiting. Repeated driver
calls at the same clock cannot spend the duration again. Zero waits forever.
Missing, mistyped, negative or nonfinite inputs produce an explicit
Unsupported procedure rather than silently becoming an endless wait.

The source for positive and zero durations is the primary author's
[Wait experiment](https://skyrimeasy.blog-rpg.com/ckworks/procedure07).
The [CK Wait notes reproduced in the Papyrus index](https://papyrus.bellcube.dev/skyrimse/script/utility/function/wait/)
identify AI Wait's real-world, non-menu clock. These sources provide no
reusable timer implementation. The world clock and actor state already provide
the needed timing and ownership. Negative-duration semantics are unverified.

StopMovement is retained as authored. A standalone Wait issues no walking
commands with either setting. Concurrent movement arbitration belongs to AI-14.
The flag does not immobilize an actor against external displacement. Branch
conditions are checked on entry, not polled continuously while waiting.
Repeating a containing sequence begins a fresh timer. Package cancellation
clears its timer without reporting success or recording Once Per Day completion.

SOEW version 17 adds the Wait procedure tag, movement flag and optional absolute
deadline to the actor's procedure state. Existing cursor and world-clock state
are reused. Versions 4 through 16 remain readable. Saves with an invalid
deadline or a Wait tag under an older version are rejected before replacing
the world. This is SOEW persistence, not vanilla ESS Wait-state restoration.

`tests/package_wait.rs` checks timing, menu pause, timescale, repeated driver
calls, package overrides, finite and endless cancellation, repetition,
save/load equality, state hashes and invalid state. Its installed-data test
checks every vanilla Wait branch's parameter types when SKYRIM_DATA is set.

### Walking Patrol routes

AI-03 follows ordinary reference markers through keyword-free XLKR links.
Its six ordered parameters are PathStart, PointRadius, Repeatable,
StartAtNearestPoint, RideHorseIfPossible and StaticPathing. The read-only
Skyrim.esm survey found 45 Patrol branches and confirmed the order on the
vanilla Patrol template. Some branches omit optional inputs using index 255.
Omitted Repeatable, StartAtNearestPoint, RideHorseIfPossible and StaticPathing
inputs default to false. Missing PathStart or PointRadius remains unsupported.
The CK procedure-tree tutorial authors Static Pathing as a boolean input but
does not describe a separate movement result. This runtime accepts either
value and follows the same cached navmesh segment route for both. It does not
model a separate static-path request or dynamic-obstacle replanning.

PathStart currently supports a specific reference or the actor's keyword-free
linked reference. Template overrides retain their defining plugin namespace.
Each marker must carry a finite nonnegative XPRD idle time. Zero idle time
yields until the next driver call before selecting another marker. Positive
idle time uses the non-menu real-seconds clock. A finite route completes after
the terminal dwell. Repeatable reverses along an open route. A route linked
back to its start loops even when Repeatable is false. StartAtNearestPoint
treats the nearest marker as already used and begins at its successor.
Distance ties use authored route order.

The driver stops within PointRadius and respects the movement budget even
for small ticks. It reuses the existing navmesh path and off-graph straight-line
fallback, including the fallback's destination-cell assignment on arrival.
This does not add load-door traversal or obstacle avoidance. Marker position,
cell, availability and current route membership are checked before movement.
Changed positions or cells invalidate the old path. Malformed links, non-root
cycles and a single-marker open repeat report Unsupported before walking.

The saved Patrol state contains its chosen start, current marker, direction
and optional dwell deadline. The route, marker properties and procedure inputs
remain on the winning records and are reread. SOEW version 18 adds the Patrol
procedure tag and progress. Earlier versions 4 through 17 remain readable.
Invalid deadlines and Patrol tags under older headers are rejected before
world replacement. Vanilla ESS Patrol state is not restored by this change.

Local xEdit PACK PKC2/PTDA and REFR XLKR/XPRD/XPPA/INAM/PDTO supply the layouts.
PTDA type 4 carries an alias ID. The UESP PACK description confirms the target
and count fields, while the CK package target pages were not present in the
local corpus.
Mutagen's `Records/Common Subrecords/Patrol.xml` confirms the patrol group.
The parser distinguishes patrol INAM from the earlier image-space INAM and
accepts null topic placeholders. The primary author's
[Patrol tests](https://skyrimeasy.blog-rpg.com/ck/packagetemplate07) establish
linked order, nearest entry, reversal and circular traversal. The
[procedure authoring tutorial](https://skyrimeasy.blog-rpg.com/ck/proceduretree01)
describes the parameter types and authors Static Pathing as an assigned input,
without documenting a distinct path result. CommonLib names a
`MovementMessageSetStaticPath` type, but its available declarations do not
expose a Patrol-specific behavior contract. The runtime comparison fixture
therefore establishes only that both values use the current cached navmesh
segment walker. These sources provide no reusable runtime walker, so this
implementation extends the existing actor driver.

The row was split before implementation. AI-88 covers marker idle, furniture
and topic actions and depends on AI-23 and HST-09. AI-89 covers mounted Patrol.
AI-90 establishes StaticPathing and omitted-input defaults. AI-91 resolves a
PTDA alias through the package's owning quest and selects the alias's reference
as the route start. That resolved global form identity is saved with Patrol
progress, so an alias into an overriding plugin retains its namespace after
reload. Object ID targets resolve in the package's defining plugin. A target
that names a FormList searches its entries in authored order. Object Type
targets search matching winning base records in load-order order, then use the
first placed reference of that base. The xEdit Object Type enum values 1
through 28 map to their record categories. Food versus alchemy, combat armor,
weapon classes, spell delivery, and bed versus chair furniture use their
authored subtype fields. Object Type 8, Miscellaneous, searches winning MISC
bases in load-order order. Actor targets require a placed actor reference, and
an actor used as a route marker defaults to zero dwell if it has no XPRD.
Both FormList and Object Type paths save the selected global reference
identity with Patrol progress. AI-33's Find procedure owns a separate
selected-reference result and is not present in this worktree.

xEdit `wbDefinitionsTES5.pas` defines the Object Type categories, their
subtypes, and PTDA Object ID as a form link. The UESP PACK description
confirms the target and count fields. The CK package target pages were not
present in the local corpus.

`tests/package_patrol.rs` exercises linked order, final dwell, open and circular
repeat, nearest entry, radius budgets, menu timing, save/resume during travel,
dwell and reversal, winning-template namespaces, live marker changes, action
field contexts, invalid state, and
`ai_91_alias_route_start_keeps_winning_namespace_and_saved_target`,
`ai_91_form_list_route_start_saves_the_authored_first_reference` and
`ai_91_object_type_route_starts_save_the_selected_reference`, and
`ai_91_search_route_start_keeps_the_winning_namespace_and_saved_target`. The
Object Type test covers each selectable enum category and saves and reloads its
selected start.
These are headless synthetic behavior checks.
The installed-data survey establishes parameter representation, not a measured
Patrol run in Skyrim or Godot.

## Localized dialogue and messages, 2026-09-20

`World::load_localized_strings` reads selected-language tables through the
existing VFS. A resolved field uses the winning record's plugin namespace and
xEdit's table rule: `DESC` uses DLSTRINGS, INFO response text uses ILSTRINGS,
and INFO prompts and ordinary names use STRINGS. `Game::load_scripts_from_data`
loads English by default, while `load_scripts_from_data_language` selects another
installed language. Installed-data dialogue validation reaches Hulda's Rumors
topic and checks its resolved choice and response text.

The data loader discovers each plugin's combined, Misc and Interface archives
case-insensitively and reports archive errors through the load report. Loose
files retain VFS precedence. Missing language tables produce the existing
`<string N>` fallback. A malformed table leaves the previously loaded language
intact. A successful language switch clears tables absent from the new language.
Synthetic tests cover separate winning owners for a DIAL question and shared
INFO response, fragment ownership, language switching and archive errors.
Host dialogue widgets and translated voice selection are outside DLG-12.

### Ingestible survey, MAG-11

The installed five-master survey parsed all 407 ALCH winners. It accepted 114
items and created 143 applications. The 293 rejections covered 187 unsupported
archetypes, 80 weapon poisons, 23 actor-value indices, one area or taper entry,
one conditioned effect and one perk, equip-ability or keyword-dispelling
entry. Every rejected item preserved the initialized World bytes. Every
accepted item consumed exactly one item, including pre-existing inventory.
Run `installed_ingestible_survey_reports_supported_applications` with
`SKYRIM_DATA` to repeat the survey. These counts describe this branch before
integration with the other magic milestones.

## Effect shader playback, SCR-24

`EffectShader.Play` and `EffectShader.Stop` update a world-owned set keyed by
the EFSH form and target reference. Play requires an EFSH form, a REFR or ACHR
target and a finite duration. The shipped
`skse64/scripts/vanilla/EffectShader.psc` sets the default duration to -1
seconds and defines negative values as infinite. A zero duration leaves no
active playback. Repeating Play replaces that shader and target pair without
changing other pairs. Stop removes only its requested pair. Timed playbacks
expire against unpaused world time and survive SOEW save/load in the `SCR-24`
world section. Invalid arguments log a native error and leave active playback
unchanged.

Successful Play and Stop calls also queue MAG-14's transient renderer requests.
Loading shader state queues stops for the old playbacks and starts for the
restored pairs using their saved remaining durations. A failed section load
preserves the live playback set and its pending renderer requests.

The supplied CK wiki corpus has no named EffectShader function page. The
shipped PSC supplies the signatures and duration rule. xEdit
`Core/wbDefinitionsTES5.pas` identifies EFSH as Effect Shader. The host
synchronizes playback identities and remaining durations for presentation
and retains MAG-14's temporary emissive spheres. It does not yet apply the
authored EFSH textures, colors or particle settings to rendered references.

## Image-space modifier lifetimes, MAG-25

The five `ImageSpaceModifier` natives now update shared World state. Ordinary
applications retain their unclamped finite strength. `PopTo` removes its source
and applies its destination without interrupting an unrelated crossfade.
`ApplyCrossFade` and `RemoveCrossFade` operate on a separate ordered chain.
The host receives ordinary applications in application order, followed by the
crossfade chain. Reapplying the same modifier replaces its application and
restarts its age. A modifier has at most one application in this implementation.

The local shipped `skse64/scripts/vanilla/ImageSpaceModifier.psc` supplies the
five signatures, default strengths and durations, and the distinction between
ordinary applications and crossfades. The CK
[PopTo contract](https://ck.uesp.net/wiki/PopTo_-_ImageSpaceModifier) confirms
unclamped strength and preservation of unrelated crossfades. CommonLibSSE-NG
`include/RE/I/ImageSpaceModifierInstance.h` stores strength and age, while
`ImageSpaceModifierInstanceForm.h` exposes trigger and stop entry points.
Their implementation is a binary relocation, so it provides no reusable
crossfade algorithm. Local skymp and Community Shaders supplied no lifetime
executor to reuse.

`TES5Edit/Core/wbDefinitionsTES5.pas` and CommonLibSSE-NG
`include/RE/T/TESImageSpaceModifier.h` define the 244-byte DNAM lifetime prefix.
Each application reads its winning IMAD record. Animated applications expire
at their authored duration. Invalid records and non-finite arguments reject
before changing the stack. All 225 non-deleted IMAD winners in the installed
five-master survey passed this lifetime reader and a Game save/load round trip.

Crossfades interpolate linearly from their current weights when interrupted.
Nonpositive fade durations settle immediately. Ages and fades use unpaused
simulation seconds. These timing and replacement policies are engine choices
covered by tests, not measured vanilla parity claims. SOEW version 16 stores
the applications, ages and in-progress fade. Older saves start with an empty
stack. Loading validates identities, finite values and the correspondence
between fade time and weights before replacing live state.

`crates/game/tests/image_space.rs` calls all five natives through Papyrus
dispatch. Its SCR-33 cases cover default arguments, unclamped strengths,
crossfade interruption and removal, PopTo preservation, invalid-form errors,
and the installed IMAD winner survey.

`tests/image_space.rs` exercises all five natives, interruption, menu pause,
animated expiry, invalid-call atomicity, save continuation and corrupt-save
rejection. Its installed-record test is gated on `SKYRIM_DATA`. The host
snapshot test checks order, fade weights and clearing on world replacement.
MAG-71 separately owns authored color, blur, depth and HDR curve evaluation
and rendering. MAG-25 adds no visible post-processing and claims no rendered
parity with Skyrim.

## ImageSpaceModifier and Keyword native audit, SCR-63

The registry binds seven methods and one global across these classes. The test
`scr_63_binding_inventory_matches_the_ck_contracts` derives that inventory
from the live native registry and checks each method/global classification.
`scr_63_image_space_and_keyword_calls_follow_documented_contracts` invokes
each binding and checks its return value or World effect.

| Binding | Local contract and implementation |
|---|---|
| `ImageSpaceModifier.Apply(float afStrength = 1.0)` method | Shipped `skse64/scripts/vanilla/ImageSpaceModifier.psc` says the strength is not clamped. The native applies the modifier at the requested strength, defaulting to 1.0. |
| `ImageSpaceModifier.ApplyCrossFade(float afFadeDuration = 1.0)` method | The shipped PSC adds the modifier to the crossfade chain and removes the prior crossfade modifier. It remains separate from ordinary applications. |
| `ImageSpaceModifier.PopTo(ImageSpaceModifier akNewModifier, float afStrength = 1.0)` method | The shipped PSC says this replaces the source with the destination without interrupting an unrelated crossfade. The CK [PopTo contract](https://ck.uesp.net/wiki/PopTo_-_ImageSpaceModifier) confirms that behavior and unclamped strength. |
| `ImageSpaceModifier.Remove()` method | The shipped PSC says to remove this modifier. The native removes its ordinary application. |
| `ImageSpaceModifier.RemoveCrossFade(float afFadeDuration = 1.0)` global | The shipped PSC declares this global and says to fade out the current crossfade modifier. |
| `Keyword.GetString()` method | CK `Keyword_Script.wikitext` records the SKSE member contract as returning the string associated with the keyword. The binding returns the keyword record's editor ID. |
| `Keyword.SendStoryEvent(Location akLoc = None, ObjectReference akRef1 = None, ObjectReference akRef2 = None, int aiValue1 = 0, int aiValue2 = 0)` method | CK `Keyword_Script.wikitext` and the shipped `Keyword.psc` say this sends the keyword and optional context to the Story Manager. It is a quiet no-op until authored Story Manager execution lands in `PRG-14`. |
| `Keyword.SendStoryEventAndWait(Location akLoc = None, ObjectReference akRef1 = None, ObjectReference akRef2 = None, int aiValue1 = 0, int aiValue2 = 0)` method | CK and the shipped PSC say it waits for Story Manager processing and returns whether a quest started. Its quiet false result is the no-quest-start case. The missing dispatch and wait path is owned by `PRG-14`. |

The class page provides the Keyword contract, while individual ImageSpaceModifier
function pages are absent from the local CK dump except for `PopTo`. For the
other ImageSpaceModifier signatures and descriptions, the shipped vanilla PSC
is the local contract source. `docs/reference-audit.md` identifies PRG-14 as
the Story Manager execution work. `SAV-20` covers persistence of a pending
event context after such dispatch exists.

## Image-space rendering prerequisite audit, MAG-71

MAG-71 depends on MAG-25's active modifier lifetimes and MAG-72's shared base
image-space inputs. `World::image_space_curve_samples` resolves each active
modifier through its winning IMAD record, validates the authored key arrays,
and samples float and RGBA keys at normalized age. Float keys use linear
interpolation, with values held at the first or last key outside the key range.
This interpolation is the current engine rule, not a recovered vanilla
interpolator contract. The host receives each sample with its independent
active stack strength, composes supported color, blur, DOF and HDR channels,
and renders them through Godot. `docs/godot-host.md` records that mapping and
the pixel fixture. Exact Skyrim shader parity remains unverified.

The local xEdit `Core/wbDefinitionsTES5.pas` IMGS and IMAD definitions,
`Core/wbDefinitionsCommon.pas` interpolation and weather helpers, and Mutagen
`Records/Major Records/ImageSpaceAdapter.xml` and `.cs` establish the layouts.
IMAD DNAM contains key counts, target flags and fixed radial-blur centers.
Scalar curves store time/value pairs. Tint and fade store time plus RGBA.
The first byte of each IAD subrecord signature is a numeric channel code,
including non-printable values. HDR and cinematic channels have separate
multiply and add curves. These are modifiers of base image-space parameters.
CommonLibSSE-NG `include/RE/I/ImageSpaceData.h` separates base IMGS data from
modifier-only fade and blur data. `ImageSpaceManager.h` retains current,
override and underwater base data alongside the combined result.

`examples/imad_curve_survey.rs` uses the existing load-order reader and xEdit
field layouts. Run it with `SKYRIM_DATA` set through
`cargo run -p soe-game --example imad_curve_survey`. It surveys the five
installed masters in their normal order and ignores deleted winning records.
The 2026-09-20 run found 225 IMAD records, including 153 animated records,
and 293 IMGS records. All 55 curve arrays in every IMAD had at least two keys,
with finite values, matching DNAM counts and no descending key times.
Non-neutral values occur in 143 blur-radius curves, 160 radial-strength
curves, 16 double-vision curves, 8 motion-blur curves and 29 DOF-strength
curves. Tint alpha is nonzero in 160 records and fade alpha in 130.
These counts describe authored data, not successful rendering.

The base dependency is used by the installed corpus. Non-neutral multiply
curves occur in 74 bloom-scale records, 29 sky-scale records, 15 sunlight-scale
records and 70 brightness records. The survey resolves 875 CELL/XCIM links
to 49 distinct IMGS records and 432 WTHR/IMSP links from 108 weather records
to 227 distinct IMGS records. xEdit defines the four weather image spaces as
sunrise, day, sunset and night. `World::base_image_space` reads the winning
IMGS for the loaded CELL/XCIM or an explicitly selected WTHR/IMSP pair. It
resolves both links through the record's owning load-order namespace.
HNAM, CNAM, TNAM and DNAM become typed HDR, cinematic, tint and depth
values. An ENAM record supplies the older compact layout when those fields
are absent.
Missing weather slots fall back to the loaded cell, and a missing cell source
returns no base image space. Malformed authored fields return a state error.
The selected weather, slots and blend survive save/load in the `MAG-72`
world section. An absent section restores an empty selection.
Invalid selection leaves the previous one intact.
SCR-73's audit records the registered `Weather.SetActive` method and eight
additional vanilla Weather natives missing from the registry. SCR-54 binds
`Weather.SetActive` to the shared request API and rejects deleted or
non-weather receivers and non-Boolean flags without replacing the current
request. The host applies the selected weather's image-space values. The
RND-18 sky also consumes the override and acceleration flags.

Several rendering contracts need further evidence within MAG-71. The local
UESP `IMAD.wikitext` describes normalized times but leaves interpolation
unspecified. The corpus contains 39 key timestamps outside zero to one.
Five pairs labelled unused by xEdit contain non-neutral values in
`ISMExterior`. CommonLib's `TESImageSpaceModifier.h` supplies interpolator
storage, while `ImageSpaceModifierInstanceForm.h` calls binary entry points
instead of defining interpolation or weighted composition. Target flags are
set in 84 radial-blur records and 4 DOF records. The instance type stores a
scene target, but the five Papyrus natives expose no target argument.
Fallback target behavior, curve interpolation parity, unused channels and
exact vanilla strength composition are not recovered contracts. The host uses
its documented weighted composition policy for supported channels. The
synthetic pixel fixture demonstrates visible host behavior, not Skyrim shader
parity.

## Value and Parts archetype, MAG-52

MGEF archetype 31 prepares the authored primary actor value as a reversible
modifier on each selected actor, for spells and ingestibles. It does not change
other actor values or actors outside the delivery target. The selected MGEF
association field does not define a body-part target for this archetype.

The local xEdit MGEF definition names archetype 31 and selects the `Unused`
associated-item field for it. The local UESP `MGEF.wikitext` says the archetype
uses PrimaryAV and is not used in game. The runtime follows that field contract
for explicit records while keeping effect state in the shared actor-value
system. The `mag_52_value_and_parts_changes_only_the_authored_value_on_its_target`
and `mag_52_value_and_parts_ingestible_changes_only_its_authored_value` tests
check the independent values and target actors for spell and ingestible paths.

## Dispel archetype, MAG-05

MGEF archetype 2 removes the target's active applications with a positive,
finite duration. Constant applications and zero-duration applications are
not selected. The cast does not delete learned or equipped spells, and it
does not affect other actors. Magnitude does not limit this selection.

The CK [Magic Effect archetype table](https://ck.uesp.net/wiki/Magic_Effect)
describes removal of effects with duration and reports magnitude as apparently
unused. The implementation follows that documented rule. Local
`TES5Edit/Core/wbDefinitionsTES5.pas` identifies Dispel as archetype 2.
`CommonLibSSE-NG/include/RE/D/DispelEffect.h` declares a Start override and no
additional saved members. It supplies no executable implementation, and local
skymp has the enum but no archetype executor. The engine therefore implements
the documented selection and reuses its existing per-application finish path.
This is distinct from the exclusions in local
`ck-wiki/DispelAllSpells_-_Actor.wikitext`, which describe that Actor native.

The cast prepares actions in authored effect order before allocating effect
objects or queuing events. It validates modifier reversals as well as new
modifiers. A Dispel entry can remove an earlier application, allowing a later
No Recast entry to apply again. A rejected later entry or overflowing reversal
leaves the existing applications, values and queued callbacks unchanged.
Victims are finished in application order, reversing only their own recover
modifiers and delivering OnEffectFinish once. The Dispel application itself
uses the normal script and lifetime path. Its action runs only at application,
so loading it or advancing time does not dispel effects added afterward.
No dependency or save-format change was added.

`tests/magic/dispel.rs` verifies native casting, finite-duration selection,
learned spells, other actors, callback counts, authored order, atomic rejection
and saved continuation. The installed five-master survey found 1,573 non-deleted
MGEF and 1,354 SPEL records, with zero Dispel effects or linked spells. All
30 magic tests pass with installed data, and the existing spell survey still
accepts 179 spells with 206 applications. Dispel execution is validated with
synthetic records, not an installed vanilla use or an original-runtime probe.
Concentration, contact delivery and condition support retain their
separate backlog scopes. Area selection is now covered by MAG-06.

## Turn Undead archetype, MAG-45

Direct casts accept MGEF archetype 24. It applies only when the target has the
`ActorTypeUndead` keyword. An active application makes an AI-driven target flee
from its caster at run speed along the available navmesh path, with a straight
path fallback when no route is available. A teammate waits until its combat
state begins. The active MGEF application ends at its authored duration, and
the AI driver stops fleeing on the next tick. The fleeing route is derived from
the active application and does not add saved fields.

Local `TES5Edit/Core/wbDefinitionsTES5.pas` names archetype 24 Turn Undead and
`uesp-modfileformat/MGEF.wikitext` assigns Confidence as its primary actor
value. The CK [Magic Effect archetype table](https://ck.uesp.net/w/index.php?section=16&title=Magic_Effect)
specifies the `ActorTypeUndead` keyword requirement and the teammate combat
exception. It does not specify movement distance or path selection. The engine
uses its existing AI movement rate and navmesh driver for those details.
`tests/mag_45_turn_undead.rs` covers eligible and ineligible targets, duration,
the teammate exception, and installed record lookup when `SKYRIM_DATA` is set.

## Runtime NAVM door links, AI-09

The AI graph keeps NAVM door links as door-reference-to-triangle associations.
It binds a link when the placed reference position lies inside the associated
triangle in XY and within 64 units of its surface height. A closed bound door
blocks that triangle in path searches. An open door restores it. A change to a
bound door's open state clears cached actor waypoints so movement plans again
against the current graph. Door triangles that do not contain the placed
reference position remain unbound.

## Scroll casting, MAG-18

`World::cast_scroll` accepts a transient one-item `Selection` from the holder's
inventory. It validates a winning `SCRL` record and its cast type, then uses the
same ordered effect conditions, target checks and action preparation as a
direct spell. It removes that exact selected item only after all prepared
effects apply successfully. A stale selection, unsupported effect or invalid
target returns before inventory removal. xEdit names SCRL cast type 3 as Scroll
and its cast enum names MGEF type 1 Fire and Forget, type 2 Concentration. The
executor accepts only fire-and-forget MGEFs on a scroll and requires matching
delivery. Scroll cast data is read through the shared SPIT and effect layout
used by `SPEL`.

Local xEdit `Core/wbDefinitionsTES5.pas` defines SCRL SPIT with cast type 3,
delivery, duration, range and ordered effects. UESP's `SCRL.wikitext` describes
the same structure. The CK `OnSpellCast_-_ObjectReference.wikitext` says a
concentration spell raises its cast event once when casting begins. A successful
scroll queues one `OnSpellCast` event on its source reference after effects apply
and its item is consumed. Failed casts queue no event and run no concentration
lifecycle.
`tests/magic/scroll.rs` checks one selected item is consumed after a successful
cast, its event is queued, and rejected casts preserve inventory and actor
state. No saved state was added.

## Exact crafted item selection (CRF-04)

`Inventory::select` chooses an explicit extra-list index or the plain remainder
and a positive quantity. `selected_stack` exposes that selection's metadata.
A selection is transient. Every inventory mutation invalidates earlier
selections, including removal that shifts indices and restoration of identical
contents. Save/load requires a fresh selection. Cloned inventories share the
initial selection revision so crafting can prepare an atomic transaction.
Revisions are process-local validity checks, excluded from saves and hashes.
They do not duplicate item state or assign new persistent item identities.

`update_selected` patches quality, charge, poison and enchantment extras while
retaining every unmentioned field, including opaque bytes and float bits.
Partial updates split only the selected quantity into a new stack. Plain
selection materializes a stack for just that quantity. The returned selection
names the changed items. The API rejects duplicate patches, unsupported kinds,
wrong field shapes, nonfinite or negative new quality/charge, null form links
and zero poison doses before changing inventory. The lower inventory API
accepts resolved identities. `World::update_item` also checks enchantment and
poison links against live ENCH and ALCH records.

`take_selected` consumes the exact selection. `World::remove_selected_notified`
prepares source and destination before committing either. Missing recipients,
self-transfer, stale selections and overflow leave quantities and events
unchanged. Successful transfers clear received worn markers and queue source
then destination count events through the existing reference/alias filters.
Updating metadata emits no count event. SOEW already serializes these extras,
so this change needs no new save version.

Exact selection rejects overlapping positive extra-list counts because they
do not identify a unique physical quantity. Saved bookkeeping and overlapping
lists remain preserved by import and ordinary count-based operations. Negative
leveled entries cannot be selected. This is an explicit engine API for future
crafting operations, not a change to Papyrus RemoveItem selection policy.

Sources read: local CommonLibSSE-NG `InventoryEntryData.h`, `ExtraHealth.h`,
`ExtraEnchantment.h` and `ExtraPoison.h`, CK `SetItemHealthPercent` and
`RemoveItem` pages, and the existing save extra-data field decoder. The headers
establish distinct per-instance fields. CK documents health as tempering state
and the limits of reference-based inventory calls. Existing inventory, save
and event paths supply the implementation. No new dependency was added.

Synthetic inventory tests cover selected quality updates retaining enchantment,
poison, charge and opaque fields, exact transfer, adjacent stacks, save/load,
plain quantity splits, stale indices, identical restoration, malformed patches,
ambiguous counts, bookkeeping, recipient overflow and failure event atomicity.
Poison use and host menus belong to their separate backlog items. Tempering
uses this exact-selection API and does not change
Papyrus RemoveItem selection policy. These tests do not compare selection
policy against the original runtime or load installed save files.

## Disenchanting (CRF-03)

`Game::disenchant` consumes one selected weapon or armor instance and adds each
unknown MGEF linked by its ENCH record to the world's learned effect set.
Instance ExtraEnchantment data takes precedence over the base item's EITM link.
Staff enchantments, malformed or effectless ENCH records, already-known
effects, and items with the `MagicDisallowEnchanting` keyword are rejected.
Eligibility is checked before the selected inventory clone and learned-effect
set are committed. A successful operation emits the normal OnItemRemoved event.

SOEW version 34 saves the learned MGEF identities, and the determinism hash
includes them. Earlier world saves load with an empty learned-effect set.
The xEdit `wbDefinitionsTES5.pas` ENCH definition supplies the ENIT field
layout, enchantment kind and EFID/EFIT pairs. The local UESP ENCH page documents
both 36-byte ENIT data and 32-byte form-version-37 data without the trailing
worn-restriction form ID. The parser accepts those two lengths. The CK wiki corpus has no
disenchanting page. Original-runtime edge cases such as partial learning from
multi-effect enchantments remain unverified. The local UESP KYWD reference
identifies `MagicDisallowEnchanting` as an engine-recognized keyword.

`crf_03_disenchant` covers learned effects, selected-stack consumption,
rejection atomicity, excluded enchantments and save/load continuation. Tests
use synthetic records and do not claim an installed-data survey.

## Quest and reference alias native contracts (SCR-68)

The Papyrus registry exposes sorted method and global binding names. The
SCR-68 inventory derives the Quest, Alias and ReferenceAlias bindings from
that registry and compares them with the method lists and descriptions in
`Quest_Script.wikitext`, `Alias_Script.wikitext` and
`ReferenceAlias_Script.wikitext`. The local corpus also has individual pages
for `CompleteAllObjectives`, `GetOwningQuest` and `Clear`. Most linked native
pages for these classes are not present in the corpus, so their class-page
contract is the comparison source.

Quest stage getters both return the highest completed stage, matching the CK
page for `GetStage`. `SetStage` and `SetCurrentStageID` share stage execution.
Alias getters resolve the owning quest, alias ID, name or assigned form from
the quest record and runtime alias state. ReferenceAlias inventory filters
use the same filter state as ObjectReference filters. `GetActorReference` and
`GetActorRef` return the held target only when it resolves to an Actor.
`Clear` checks the QUST alias `FNAM` Optional bit, which is bit 1 in xEdit and
the UESP QUST layout. It logs an error and leaves a required alias unchanged.
For an optional alias it removes the target and reevaluates the held actor's
package through the world package driver, matching
`Clear_-_ReferenceAlias.wikitext`.

Quest `IsStarting` and `IsStopping` appear in the CK class page and are
registered as quiet false stubs. They do not report the startup and shutdown
transitions described by CK. SCR-43 owns their behavior and regression tests.

## Alias-backed dialogue eligibility (DLG-13)

Dialogue already evaluates `GetIsAliasRef` (CTDA 566) against the current
speaker reference and the quest that owns the winning INFO's topic. The alias
number is a quest-local identifier. It is not a FormID or an NPC base. Two
references using one NPC base can therefore receive different topic choices.
`World::alias_ref` reads the canonical saved quest state on each eligibility
check. Clearing or reassigning an alias changes the next choice listing.
Selection checks again before consuming Say Once or dispatching fragments.

The DLG-13 conformance tests establish this existing behavior through the
conversation API, including a winning plugin override, alias zero in two
quests, references sharing one base, clearing a displayed choice, reassignment,
and save/load of both empty and filled aliases. No production code or new
speaker-state cache was needed. AI-12's search-based alias filling is not a
prerequisite for consuming an already filled alias.

Sources read: local xEdit `wbDefinitionsTES5.pas` declares condition 566's
alias parameter, QUST reference-alias fields and INFO ANAM as an NPC-base link.
UESP `Raw_Function_Data.wikitext` names `Ref.GetIsAliasRef(QuestAlias)`.
CK `Clear_-_ReferenceAlias.wikitext` specifies emptying an optional alias.
The tests use optional aliases and the existing world mutation API. They do
not establish automatic alias filling, original-runtime condition parity,
Papyrus Clear error behavior, or scene action scheduling. Those capabilities
retain their own backlog scopes.

## Leveled spell initialization (PRG-11)

Actor initialization expands each LVSP in its inherited and distributed spell
set at the resolved actor level. Nested lists use that same level. LVLF selects
all eligible levels or only the highest eligible level. Duplicate entries keep
their random selection weight. Use All Spells selects every eligible entry.
Nested entry quantities cause separate rolls when the nested list requests
Calculate For Each Item In Count. Repeated concrete spells collapse in the
actor's existing spell set. Direct spells and authored shouts retain their
existing paths. Root lists are visited in resolved FormID order.

Selection reads the winning record's namespace and the canonical mutable
leveled-list state, including script additions and chance-none edits. An
initialized actor retains its resulting spells, including an empty result,
through the existing save format and hash. Later list edits affect newly
initialized actors. No second spell-selection table or save version was added.
Previously saved concrete spells remain unchanged. Older engine saves can
contain unresolved LVSP placeholders. The next actor access expands those
placeholders once while retaining already granted concrete spells. The result
then uses the same saved spell set, without repeating selection on later reads
or another save/load cycle.

`World::select_leveled_spells` exposes typed selection errors. Invalid sizes,
entry counts, spell links, flag bits, zero quantities, out-of-range chance-none,
and cycles reject selection. LVSP has no authored LVLG chance-global field.
The traversal permits 128 nested lists and 65,536 traversal/roll operations,
then returns an error. A failed selection restores the RNG and installs no
partial actor. Lazy list reads can still materialize shared list state, as with
actor-template selection. The existing optional actor getter returns no actor
when initialization fails.

Sources read: local xEdit LVSP and `wbLeveledListEntry`, Mutagen Skyrim
`LeveledSpell.cs` and `LeveledSpell.xml`, UESP `LVSP.wikitext`, skymp
`LeveledListUtils.cpp`, and OpenMW `mwmechanics/levelledlist.cpp`. xEdit supplies
the twelve-byte entry layout and the three flag bits. OpenMW and the existing
actor-template consumer supply the highest-eligible/all-level comparison.
skymp supplies the recursive count-selection pattern, but its missing
highest-level restriction and chance-global shortcut are not reused. These
references do not provide a reusable Rust consumer. No dependency was added.

Synthetic tests cover actor-level filtering, nested counts, duplicate
weighting, Use All Spells, saved selections and empty outcomes, winning links,
template inheritance, traversal bounds, malformed inputs and RNG rollback
after earlier successful draws, plus older saved placeholders. PRG-21 adds
synthetic cases for flags 0, 1, 4, 5, 6 and 7 at levels 1, 10 and 20, nested
Use All selection, and chance-none values 0 and 100. These cases exercise the
engine selector and do not establish original-runtime behavior. xEdit supplies
the LVSP flag names and values. CommonLib's `TESLeveledList::Flag` supplies the
same bit names, but its interface does not specify selection precedence. The
SKYRIM_DATA-gated five-master survey executes winning LVSP lists at levels 1,
10 and 20 and checks that every result resolves to a SPEL record. The survey
checks record execution, not comparison against a running original game. Live
actor leveling and later spell reevaluation retain their progression scope.

## Concrete inventory reference moves

INV-01 extends ObjectReference.AddItem to accept a concrete reference with
count one. A loose reference owns one item stack until pickup. The complete
quantity and extra-data list move into the destination inventory, while the
loose reference becomes disabled and leaves its cell. A later AddItem moves
that same stack between holders. It does not create another copy. Invalid
counts, container objects, missing destinations, self-transfers and quantity
overflow leave item quantities and event queues unchanged.

The original reference identity uses mapped ExtraOriginalReference data,
kind 32. ExtraReferenceHandle, kind 28, is container metadata and cannot claim
that identity. Ordinary quantity removal consumes plain items first. A partial
split retains the original identity only on the remainder. Full transfer keeps
it with the moved stack. Consumption leaves no loose stack to recreate, even
after save/load. Transfers clear worn flags and preserve the other extras.

OnItemAdded and OnItemRemoved carry the concrete reference and source or target
container. OnContainerChanged follows a pickup or transfer. Destruction by
RemoveItem does not queue OnContainerChanged. Inventory filters continue to
apply to holder and running-alias notifications.

SOEW version 16 adds loose item metadata. It reads older versions by building
loose metadata from their reference state and authored record. New saves check
for duplicate contained identities and loose/contained collisions before
replacing world state. Older saves and imported stacks can name a contained reference that has no
loaded placement. Load reconstructs that identity from its record or from the
saved stack base, clears loose placement and reserves dynamic IDs. Item
metadata participates in the world hash.

Sources read locally were CK AddItem, OnItemAdded and OnContainerChanged,
CommonLibSSE-NG ExtraOriginalReference, ExtraReferenceHandle and ExtraDataTypes,
and xEdit wbDefinitionsTES5.pas REFR. The authored loose fields initialized here
are XCNT, XCHG, XOWN and XRNK. SetActorOwner, SetFactionOwner and SetDisplayName
update loose metadata before pickup. Runtime stacks retain arbitrary mapped extras,
including quality, enchantment, poison and text metadata. This does not add
restoration of general loose-reference ESS extras or dropped-item physics.
No dependency or per-reference host node was added.

The inventory_references fixtures check loose pickup, quantity-three pickup,
exact-stack transfer, adjacent plain items, notification identity, consumed
references, partial splits, overflow, Papyrus calls, legacy-save migration,
metadata hashing, stable save bytes and atomic rejection of corrupt saves.

## Encounter-zone selection levels

`World::encounter_level` reads a receiver's explicit XEZN, current parent cell
and containing worldspace, in that order. Missing zones and the hardcoded
NoZoneZone identity, Skyrim.esm form 00001E, use current player level. The query
does not initialize references, consume RNG, lock zones or add level variance.
The result is at least one and does not apply an additional player-level clamp.

Zone calculation reads winning ECZN DATA and live `fLevelScalingMult`. Below
the minimum, Match PC Below Minimum preserves the player level, otherwise the
minimum is used. At or above the minimum, the runtime calculates minimum plus
player-minus-minimum times the scaling multiplier, then truncates. A nonzero
maximum caps that result. The executable default multiplier is 0.6. The runtime
reads min/max bytes as unsigned despite the signed declarations in xEdit and
CommonLib. Zero is the unlocked sentinel, so a calculation of zero remains
unlocked even though the public level query returns one.

`load_cell` and `attach_cell` lock their zone before loading references or
queuing transition events. Invalid zone data prevents that entry. Prefetch and
ordinary queries do not lock a zone. Placed actors and dynamically placed actors
feed the zone level into existing leveled-template selection. Template choices
retain their existing once-per-reference lifetime. If the player level changes
between prefetch and entry, prefetched actors retain the earlier selection even
when entry locks a different zone level. Refreshing unobserved prefetched actors
is not implemented. Zone locks survive unloading and later entry. Explicit
`enter_encounter_zone` provides a fallible entry API. After ten game days since
the prior entry, a zone without the ECZN Never Resets flag clears its previous
level lock and rebuilds its cached nonpersistent references from their winning
records. This restores authored placement and contents. Persistent and dynamic
references keep their state. Uncached references already read from their records
need no work. Each entry, including an exterior streaming focus change,
restarts the interval. The new lock uses the current player level. SOEW version 34 adds
sorted last-visit game days, rejects duplicate or invalid entries before
changing state, and reads older saves with empty zone history. Both zone tables
contribute to the form-state hash.

Sources are local xEdit ECZN/XEZN declarations, UESP ECZN and CommonLib
`BGSEncounterZone.h`, plus CK Reset documentation. xEdit and CommonLib define
Never Resets as flag bit zero. The CK Reset reference describes restoring the
authored reference state, which this reset applies to cached references.
The installed SkyrimSE executable, SHA256
`846efccf0c1374d71f892907f46549560f2fcb0a75cb87a3eed438baa0f1402f`, supplied
read-only checks of zone lookup at `0x140300ab0`, receiver level at
`0x140300b90`, scaling at `0x14028f9e0`, locking at `0x14028fa70`, and the
NoZoneZone identity constructor at `0x14028fd93`. No executable bytes or
proprietary code were copied. PRG-03 is not a dependency: the existing template
selection already accepts an explicit level without computing NPC attributes.

This work supplies zone levels to the existing LVLF selection rules. The CK
LeveledCharacter page also describes XLCM difficulty-specific selection,
including Easy weighting and Very Hard tier advancement. Those selector rules
are a separate gap. PRG-10 adds zone reset and lock clearing for cached
nonpersistent references.
Importing vanilla ESS encounter-zone changes is outside this selection contract.

## Ordinary perk purchases (PRG-02)

`Game::check_perk_purchase` and `Game::purchase_perk` accept the first PERK
record of a rank chain. They follow winning NNAM links, select the first
unowned record and evaluate its top-level CTDA prerequisites on the player.
Passing a later rank directly returns `NotRoot`. Missing records, deleted
records, malformed purchase fields and cycles reject the complete chain.
A preview spends nothing. Purchase checks again, inserts that rank's resolved
FormID into the canonical actor perk set and consumes exactly one point.
Failures preserve perk ownership and points. Lower ranks stay owned, and
externally granted ranks are not charged again. Existing saves and hashes
already contain both fields, so the save format is unchanged.

Purchase conditions end at the first PRKE effect entry. Effect conditions
never gate purchase. HasPerk and condition FormIDs use their winning record's
master namespace. The root-only API is engine policy. CTDA 277, GetBaseActorValue, now reads the existing base
component, excluding temporary and permanent buffs. Actor-value indices use
the existing mapping for indices 0 through 45. Unknown indices, unavailable
actors and unavailable run-on targets return typed errors. Other unsupported
condition functions keep the shared evaluator's explicit error behavior.

DATA Playable and the record Non-Playable flag gate purchases. Trait, Level,
Num Ranks and Hidden are accepted at any value (PRG-16). Higher purchased
ranks are separate NNAM-linked forms, not increments of PRKE's effect rank
byte, so Num Ranks (a Creation Kit display field) is read from nowhere in the
purchase path, the chain walked by `rank_chain` is the count that matters.
Script AddPerk remains a separate grant path. This API does not execute
entry-point effects, abilities or perk fragments, provide a skill menu, or
implement beast-form point currencies.

DATA also accepts the two truncated lengths wrye-bash's
`MelPerkData(MelTruncatedStruct)` declares as `old_versions={'4B', '3B'}`
(`Mopy/bash/brec/common_subrecords.py`, reused unmodified for Skyrim's
`MrePerk` in `Mopy/bash/game/skyrim/records.py`): a 4-byte DATA carries
Trait, Level, Num Ranks and Playable with Hidden zero-padded, and a 3-byte
DATA carries only Trait, Level and Num Ranks with Playable and Hidden both
zero-padded, matching `MelTruncatedStruct`'s own default fill. Since Hidden
carries no purchase meaning here and a zero-padded Playable reads as false,
a 3-byte record purchases only if some other well-formed record grants
Playable, otherwise it fails with `NotPlayable`, never with the malformed-
record error. Only lengths outside 3 to 5 bytes, or a second DATA subrecord
on the same record, are reported as malformed.

PRG-16 verified Trait and Level have no purchase meaning here. Both bytes are
zero on every one of the 475 winning PERK records across the installed base
game and its three DLC, so nothing in the current load order exercises a
non-zero value. No source in the corpus describes a runtime consumer for
either byte: not `wbDefinitionsTES5.pas`, not UESP `PERK.wikitext`, not
CommonLibSSE-NG `BGSPerk.h`, not wrye-bash's `MelPerkData`, not Mutagen's
`Perk.xml`. The perceived level requirement on perks such as Armsman is a
per-skill prerequisite expressed as a top-level CTDA against the relevant
actor value and, where a literal character level gates a perk, against
GetLevel (function 80, already implemented), confirmed against UESP's
`Skyrim:Perk` page, never the DATA Level byte. Hidden only ever hid a perk
from the CK or an SKSE UI list, this API has no UI layer for that flag to
affect, so a hidden perk purchases exactly as a shown one does. 62 installed
winning records set Hidden and remain purchasable under this rule.

Sources read were local xEdit `wbDefinitionsTES5.pas` PERK, CTDA 277 and
actor-value definitions, `wbDefinitionsCommon.pas` Num Ranks normalization,
UESP `PERK.wikitext` and (PRG-16) `Skyrim:Perk`, CommonLibSSE-NG `BGSPerk.h`,
wrye-bash `common_subrecords.py` `MelPerkData`, Mutagen's `Perk.xml`, and CK
AddPerk, HasPerk and GetBaseActorValue pages. SKSE `PapyrusActorValueInfo.cpp`
independently tests HasPerk for every NNAM-linked rank. Together with
authored HasPerk conditions, this supports retaining lower identities. That
retention is an inference, without a direct original-runtime purchase
comparison. NPC PRKR is a separate FormID plus rank byte. UESP labels that
byte unused, and the existing NPC reader retains only its FormID. This
purchase path does not reinterpret it. Entry-point dispatch also uses PRKR
for identities and derives ranks from NNAM chains. The full CK Perk page was
absent from
the local dump and its online fetch was unavailable for PRG-02, PRG-16 did
not need it since the installed corpus and the sources above settle all four
bytes. The existing CTDA parser, condition evaluator, actor values and
ownership/save paths were reused. No package or host node was added.

Synthetic tests cover single-point consumption, false prerequisites, stale
previews, base skill versus buffs, OR groups, unsupported conditions and
actor values, effect-condition separation, winning override namespaces,
missing/cyclic links, root-only requests, externally granted ranks, zero
Num Ranks, non-playable records, non-default Trait, Level, Num Ranks and
Hidden, and save/load.

**Live run (2026-09-24, PRG-16).** The installed-data survey reads 475
winning perks across five masters. Trait and Level are zero on every one. Of
the Num Ranks 1 records, 13 are not playable and shown, 21 are not playable
and Hidden, 374 are playable and shown, and 40 are playable and Hidden. Of
the Num Ranks 5 records, 26 are playable and shown and 1 is playable and
Hidden. It validates all 397 root chains, none of which raise an unmapped
condition function, and exercises all five Armsman ranks, including a failed
low-skill purchase. Root eligibility across the installed corpus is 11
already owned, 154 eligible, 32 not playable and 200 with a false
prerequisite. These tests do not establish original-runtime parity for
malformed graph policy, effect execution or menu behavior.

## Perk entry-point dispatch (PRG-08)

`EntryPointDispatcher` registers handlers by entry-point ID and visits an
actor's owned winning PERK records in load-order order. NPC base PRKR entries
also supply owned identities. All actors use the highest owned rank in that
PERK's NNAM chain. Within each PERK it filters out complex
entries above that rank, then orders eligible entries by ascending PRKE
priority. Equal priorities retain their source order. The invocation carries
the entry's one-based PRKE rank, priority, and EPFT/EPFD data. Priority direction, tie
behavior and cross-perk interleaving still need a vanilla capture. The local
sources do not establish exact versus cumulative effect-rank selection.

Each PRKC tab evaluates its own CTDA conditions against the matching supplied
tab subject. Conditions within a tab use the shared evaluator's file-order
AND/OR behavior. A matching entry without a registered handler returns a
typed error. Malformed data, missing tab subjects and unsupported conditions
also return errors before that entry's handler runs. Non-complex PRKE sections
are not dispatched. Handler changes are applied as the dispatcher reaches
each eligible section.

The combat entry points `ModAttackDamage` (35) and `ModIncomingDamage` (36)
are consumed by `World::melee_hit`. CMB-09 applies supported numeric functions
to the staged weapon damage, with outgoing effects before incoming effects.
Their condition tabs are owner/weapon/target and owner/attacker/weapon,
respectively. Malformed function payloads, unknown actor values, unsupported
functions and condition errors reject the hit before health or events change.
This path retains winning-record order, followed by ascending priority and
source order for ties within each PERK. These ordering choices have not been
compared with the reference runtime. CMB-09 hit tests exercise noncommuting
outgoing and incoming modifiers at different priorities and across two
winning PERK records. They verify this implementation's order and do not
establish Skyrim's runtime order.
It does not claim the unresolved priority-tie, acquisition-order or exact
versus cumulative rank behavior documented above. Source-order ties and
ascending priority are deterministic engine choices, not captured vanilla
behavior. Cross-perk ordering also needs a capture.

### NPC perk loading and sparse condition tabs, CMB-39

NPC base PRKR entries retain the first occurrence of each resolved perk
identity. A duplicate does not run a perk's effects again or reject dispatch.
The rank byte does not override NNAM-derived eligibility. A structured
`game::perks` warning names the NPC base once per world, including when
multiple references share that base. Its warning set is session diagnostics
and is excluded from saves. PRKR payloads must still be exactly eight bytes.

Complex entry DATA's third byte describes available condition tab types,
rather than the number of serialized PRKC groups. An absent group imposes
no conditions. Present groups still evaluate against their supplied tab
subjects and report missing subjects or condition errors before handlers run.
The installed player's `AlchemySkillBoosts` PERK uses sparse groups and
participates in attack-damage dispatch, including the protected-NPC kill path.

Sources are local UESP `NPC.wikitext` and `PERK.wikitext`, xEdit
`Core/wbDefinitionsTES5.pas` NPC PRKR and PERK condition definitions, and
Mutagen `Npc.xml` and `Perk.xml`. UESP calls PRKR's rank byte unused, and
xEdit marks the tab-count byte `cpIgnore`. The local CK `AddPerk` page
does not specify duplicate PRKR rank selection. The existing reader and
dispatcher are reused with identity deduplication and optional tab groups.

## Leveled item entry extras

`World::roll_leveled_inventory` returns the existing `Inventory` model with
selected COED ownership and condition extras. `roll_leveled_items` remains the
quantity-only projection. Neither call mutates a reference or queues events.
The inventory result can be applied through the existing bundle transfer API.
Owner and NPC ownership-global links resolve in the winning list's namespace.
Faction ranks retain signed bits, with rank -1 omitted as in the runtime.
Weapons and armor retain condition multiplier bits outside the runtime's
open default interval of 0.9999 to 1.0001. The -1.0 deletion sentinel
produces no health extra. Other item types ignore condition.
Selected stacks persist through the existing SOEW inventory extension.

Sources are local xEdit `wbDefinitionsTES5.pas` COED/LVLI declarations,
Mutagen `ExtraData.cs`, CommonLib `ContainerItemExtra.h` and its ownership,
global, rank and health extra types, and the public Creation Kit
[LeveledItem](https://ck.uesp.net/wiki/LeveledItem) documentation. These sources
declare the layout but do not establish nested metadata precedence.
A read-only check of the installed SkyrimSE executable supplied that rule.
The executable SHA256 is
`846efccf0c1374d71f892907f46549560f2fcb0a75cb87a3eed438baa0f1402f`.
The multi-item resolver at `0x1401e90f0` recurses without passing intermediate
COED into the child result. Only selected leaf entries supply inventory extras.
The separate single-form recursive selector has a different fallback rule,
so it does not govern this inventory API.

The COED constructor at `0x140155d10` uses multiplier 1.0, its loader at
`0x140155d30` copies the stored float unchanged, and application at
`0x140156060` converts owner, global, rank and equipment health to extras.
This corrects a misleading CommonLib constructor default of 100.0. The CK
health display uses percentages, while the record and ExtraHealth use the
same multiplier. No executable bytes or proprietary code were copied.

Synthetic tests cover nested precedence, distinct and identical stacks,
script-added entries, namespace remapping, signed ranks, health boundaries,
invalid metadata, RNG preservation and selected-inventory save/load.
Special-loot selection and Papyrus AddItem integration are separate rows.

## Leveled AddItem transactions

The INV-06 transaction stages the selected inventory before applying any
counts or COED extras. A capacity failure discards all staged additions and
restores RNG state. Successful application queues one OnItemAdded event per
selected base form with no original-reference argument.

The AddItem path carries a player-level cap separately from the caller's
selection level. Ordinary selection uses the lower level, while recursion
retains the original input. The general roll API keeps its caller-supplied
level. Read-only inspection of the installed executable identified the
AddItem helper at `0x140a56220`, its usePlayerLevel argument at
`0x140a563fb`, and the ordinary cap at `0x1401e8ea6`. The executable hash
is recorded in the entry-extras section above. Special Loot bypasses that
ordinary cap, including when selected beneath an ordinary parent list.

The transaction fixtures cover events, metadata, save/load, empty selection,
player-level capping and capacity failure both before and after staged work.
The ObjectReference.AddItem native dispatches LVLI through this transaction.
It reads the receiver's encounter level without locking a new zone or applying
variance. Ordinary selection applies the separate player cap. Papyrus fixtures
check zone versus player levels, Special Loot, positive counts and explicit
zero or negative counts. Concrete-reference dispatch retains INV-01 behavior.

The combined AddItem resolver includes PRG-16 precedence. Calculate-each wins
before Use All, matching runtime branches `0x1401e91f1` and `0x1401e92d4`.
Flags 5 and 13 use every entry. Flags 6, 7, 14 and 15 roll per requested count,
with Special Loot still active for flags 14 and 15. Integrated tests check
these rules together with selected COED metadata and the player-level cap.

INV-06 integrates the reviewed INV-01 reference-transfer implementation
`52b1605`, PRG-04 encounter zones `8710fad`, PRG-18 Special Loot `ce0551e`
and PRG-20 flag precedence `112a85e` over PRG-19 entry metadata `fd96e42`.
The combined native path keeps ordinary item counts, concrete reference moves
and leveled additions distinct. FormList expansion remains the INV-05 row.

## Default outfit instances

Fresh placed and dynamic NPC references resolve DOFT from their selected
INVENTORY template source. OTFT INAM links use the winning outfit record's
namespace and retain authored entry order. Each LVLI entry rolls separately at
the NPC's calculated level with count one, including repeated list entries.
This caller does not use AddItem's encounter-level input or player-level cap.
The ordinary, Special Loot and mixed-flag rules use the shared LVLI resolver.

Outfit application creates one instance per selected base, regardless of the
rolled quantity. Duplicate results carrying the same outfit tag do not add a
second copy. ExtraOutfitItem 142 records the resolved outfit FormID. The outfit
resolver mode skips COED processing, matching the original caller's discarded
metadata. PRG-15 is therefore not a semantic dependency. INV-06 supplies the
integrated shared resolver used by this caller.

The selected outfit stack receives ExtraWorn 22. Existing plain copies of the
same base remain plain. Armor equipment uses CMB-16's supported slot-conflict
rules and INV-07's resolved template fields, retaining displaced armor as an
unworn inventory item. Weapon and ammunition forms, plus carryable lights,
receive worn markers through their declared equipment categories. Other
selected inventory forms remain carried without invented worn markers.
The existing equipment model does not resolve hand conflicts or execute
scroll/consumable equip actions. Those categories retain the selected item.

OTFT entries are processed in authored order. Each LVLI result uses the shared
resolver's sorted FormID order. Later armor replaces earlier conflicting
supported slots. This is deterministic engine policy, not a verified claim
about the original runtime's conflict winner.

Outfit preparation does not recursively initialize actor state. The later actor
view derives its equipped set from worn stacks. Existing references are never
initialized again, even if their outfit items have been removed. Actor
references survive cell unloading, and existing SOEW inventory extras and actor
equipment preserve the result through save/load. Restored inventories are not
repopulated. No inventory events are queued during initial construction. A
malformed outfit rejects fresh authored-reference construction, or disables a
dynamic placement, without installing a partial outfit or consuming outfit RNG.
SOFT sleep-outfit materialization and runtime SetOutfit replacement are separate
from this default spawn contract.

Sources are local xEdit OTFT/DOFT definitions, CommonLibSSE and CommonLibSSE-NG
`BGSOutfit.h`, `InventoryChanges.cpp`, `ExtraOutfitItem.h` and equipment class
headers, plus CK `EquipItem` and `SetOutfit`. skymp's `GetOutfitObjects` confirms
the INVENTORY template source, but its separate rolls for inventory and equipment
were not copied. Read-only inspection of installed SkyrimSE 1.7.104.0, SHA256
`846efccf0c1374d71f892907f46549560f2fcb0a75cb87a3eed438baa0f1402f`, verifies
`InitOutfitItems` at `0x140233f90`, its visitor at `0x14023fa80`, NPC level input
at `0x14023c3e6`, and tagged result application at `0x14023c0b0`. The COED call
at `0x14023c1a5` passes a null extra list and returns without applying fields.
The wear helper at `0x1402e8070` finds the outfit-tagged extra list and passes
that exact instance, count one, to ActorEquipManager at `0x1406dc310`.
No executable bytes or proprietary code were copied.

`tests/outfits.rs` checks exact outfit stacks, plain-copy isolation, selected
templates, winning namespaces, per-entry draws, nested quantity collapse, COED
discard, slot conflicts, nonwearable results, dynamic placement, once-only
initialization, restored inventories, save/load and invalid-input rollback.

## Live skin overrides and body parts (INV-08)

`World::set_actor_base_skin` and `World::set_race_skin` write WNAM into the
canonical record overlay. `ActorBase.GetSkin/SetSkin` and `Race.GetSkin/SetSkin`
use these operations. Null clears the direct skin, so actor assembly falls back
from NPC WNAM to race WNAM. A private leveled NPC base affects its owning actor.
No setter inserts an inventory stack, equips an item or sends an inventory event.
The existing SOEW record-override encoding also persists race WNAM. Its reader
rejects race changes outside that field, invalid skin targets and NPC-only metadata.

`appearance::ActorAppearance` derives surviving ARMA body slots and male/female
third-person and first-person model paths. It reads current record overrides,
actor race and equipped forms. Footsteps and the host use this same selector.
Armor templates retain INV-07's inherited armature source and plugin namespace.
Priority ties retain the documented later-addon and sorted-equipment policy.
This change does not establish an original-runtime comparison for those ties.

Sources read locally:

- `skse64/skse64/PapyrusActorBase.cpp`, GetSkin and SetSkin, and
  `PapyrusRace.cpp`, GetSkin and SetSkin, directly read and assign the skin form.
- `TES5Edit/Core/wbDefinitionsTES5.pas`, NPC and RACE WNAM, ARMO armatures,
  ARMA sex priorities, model paths and additional races.
- `CommonLibSSE-NG/src/RE/T/TESObjectARMA.cpp`, IsValidRace follows armor parents.
- `ck-wiki/ActorBase_Script.wikitext`, the SKSE skin method declarations.

`footstep_actors` checks live changes, null fallback, mapped links, save/hash
round trips, invalid-save rollback, private-base isolation and unchanged carried
inventory. `natives_2` calls all four methods through Papyrus bytecode. The host's
`real_skinning` tests exercise the same refresh preparation used by its node.

## Nearby authored interior destinations, HST-07

`World::nearby_interior_destination` reads resident source doors in the
caller's logical cell, filters disabled and deleted references, and follows
their winning-record XTEL links to interior cells. It compares source positions
only after cell identity matches. The nearest source wins, with FormID as the
deterministic tie breaker. Invalid radius and non-finite coordinates select
nothing. This read does not construct references, choose actor templates,
change residency, move the player or queue events.

The policy follows local OpenMW `scene.cpp` teleport-door preload scheduling.
The existing reference loader supplies owner-relative link resolution without
a second record parser. The host prepares immutable CPU resources using the
selected cell, leaving `arm_doorway` and the cold-cell event decision until
activation. Synthetic checks cover overlapping coordinates in unrelated
interiors, disabled/deleted doors, invalid input, unchanged reference counts
and preserved cold residency. Host resource reuse and budget scope are
recorded in [godot-host.md](godot-host.md#nearby-interior-prefetch-hst-07).

UI-02 routes native menu equipment through World::equip_item and
World::unequip_item. Actor.UnequipItem shares the latter operation, which removes
both the actor's equipped entry and the inventory's worn markers. The menu
requires a carried weapon or armor before calling these operations, so a stale
selection cannot create an item. The host's inventory_menu tests cover shared
armor slots, worn-marker cleanup and rejected missing or unowned selections.

UI-03 adds `World::equip_hand_spell` and `World::hand_spell` in
`world/spell_hand.rs`, following the existing `weapon_hands.rs` split of
explicit-slot state from the known-item set. `ActorState` gains `left_spell`
and `right_spell`, independent of `left_weapon`/`right_weapon` so a weapon
and a spell can occupy the same named hand at once, matching vanilla dual
wield of a one-handed weapon against a spell. `equip_hand_spell` rejects a
spell the actor does not know and any SPIT spell type other than 0 ("Spell"
in xEdit's SPEL enum), the CK wiki's EquipSpell/UnequipSpell hand argument
convention (0 left, 1 right). `hand_spell` reads back the assigned spell only
while the actor still knows it, mirroring `weapon_hands::hand_weapon`'s
stale-entry handling. Both new fields are covered by `state_hash` alongside
the existing weapon-hand fields, for World/replay determinism.

`save.rs` carries `left_spell`/`right_spell` from `SAVE_VERSION` 32 onward,
written unconditionally (the writer always emits the current format) and read
behind `if version >= 32`, the same pattern `witnessed_crimes` used for
version 31. An older save simply loads with both hands empty, matching how a
pre-31 save loads with no witnessed crimes. The `AI-16` world section retains
script-qualified pending calls for package fragments, including their actor
argument and queue position. The unknown-version probe in `save_tests.rs` uses
the version after `SAVE_VERSION`.
pre-31 save loads with no witnessed crimes. The `AI-16` world section retains
script-qualified pending calls for package fragments, including their actor
argument and queue position. The unknown-version probe in `save_tests.rs` tracks
the current version and remains one past `SAVE_VERSION`.

`natives.rs`'s `EquipSpell`/`UnequipSpell`/`GetEquippedSpell` now share
`equip_hand_spell`/`unequip_hand_spell`/`hand_spell` with the menu for source
0 and 1 (left and right hand), so a script equip and a menu equip are the
same state and either can see the other's result. `equip_hand_spell` also
keeps the actor's generic `equipped` set in sync (inserting on equip, removing
once neither hand holds a form) so `IsEquipped` and other `equipped`-set
readers agree with the hand slots. Sources 2 and 3 (voice and instant, used
for Powers and Lesser Powers) fall back to the old direct `equipped` set
write, since those spell types are rejected by `equip_hand_spell` and have no
hand to occupy.

`GetEquippedSpell` only reads sources 0 and 1. The generic equipped set written
for sources 2 and 3 does not retain their source identity, so those getter
sources return no form. `natives_dlc.rs`'s
`scr_15_get_equipped_spell_reads_the_requested_hand` checks both hand
arguments, the unassigned opposite hand, and an unsupported source.

`godot-host/src/magic_menu.rs` builds the known-spell list through the same
force-build-then-read path `inventory_menu.rs` uses (`World::actor` once,
then `World::actor_ref`), and `godot_player/magic.rs` wires it to the host's
open/select/equip calls, so the Godot-side magic menu equips through
`equip_hand_spell` too. `game/tests/ui_03_magic_menu.rs` covers
`equip_hand_spell`, `unequip_hand_spell`, `hand_spell` and the save round
trip: equipping to a hand, the opposite hand staying untouched, an unknown
spell rejected, a Power rejected as not hand-castable, a deleted SPEL record
rejected, and a hand spell surviving a `save_state`/`load_state` cycle.
`godot-host/tests/ui_03_magic_menu_adversarial.rs` covers the menu path: a
cold actor still listing its spells, a deleted record excluded from the list
and rejected on equip, and `EquipSpell`/menu equip agreeing on
`GetEquippedSpell`. The existing `world.rs` and `hash.rs` suites were left
unchanged since neither exercised the two new fields before this change.

## Race animation models, ANM-55

`race::AnimationModels` reads male and female skeletal `ANAM` strings and
behavior project `MODL` paths in the `RACE` subrecord order defined by
`TES5Edit/Core/wbDefinitionsTES5.pas`. The `MNAM` and `FNAM` markers select the
sex slot. `World::actor_animation_rig` selects the paths for the actor's
current race and sex after the same trait and live-race resolution used by
appearance selection. Missing sex-specific paths remain absent for the host
to report.

CommonLibSSE-NG's `TESRace.h` names the matching runtime members
`skeletonModels[sex]` and `behaviorGraphs[sex]`. The focused synthetic test
checks the marker order and confirms one sex does not borrow the other sex's
paths. The host follows the selected project into its `hkbProjectData` and
`hkbCharacterData` records to find the HKX rig and behavior graph named there.

MAG-22 adds `Game::begin_hand_cast` and `Game::end_hand_cast` for input routed
to an actor's equipped left or right spell. The first begin validates and
executes a costed fire-and-forget cast through `World::cast_charged_spell`, or
starts a held concentration cast through `World::begin_concentration_cast`.
Both paths dispatch the validated `OnSpellCast` event. A repeated begin for
that held hand returns false without charging or applying another effect.
Releasing the hand clears the request guard and stops an active concentration
cast for that actor, so a later press can cast again. This follows the CK
`GetEquippedSpell` hand sources, `InterruptCast` release behavior and
`OnSpellCast` start event, with cost and delivery fields read from xEdit's SPEL
`SPIT` definition. It does not retain held cast state in save data.

## Actor controller scale, CMB-33

`race::Attributes` reads two more pairs from `RACE`'s `DATA` prefix,
`height_male`/`height_female` at offsets 16 and 20 and `weight_male`/
`weight_female` at offsets 24 and 28, from `wbDefinitionsTES5.pas`. `npc::Npc`
reads the matching per-actor multipliers from `NPC_`'s `NAM6` (height) and
`NAM7` (weight) subrecords, both a single finite `f32`, defaulting to 1.0
when absent. A malformed or non-finite value in either rejects the record,
matching the existing RACE attribute fields.

`World::actor_controller_scale` combines them as `race.height[sex] *
npc.height`, the same formula `TESNPC::GetHeight()` uses in
CommonLibSSE-NG's `src/RE/T/TESNPC.cpp`. Both inputs come from the NPC that
supplies the Traits template group, the same source `actor_base_race` reads:
the Creation Kit's "Use Traits" flag groups race, sex, height and weight
together, so resolving all four from one template source keeps them
consistent with each other. An actor base with no race, or a race with no
`DATA`, uses scale 1.0. A non-finite or non-positive result rejects rather
than handing degenerate geometry to a caller.

This is the scale `soe_physics::generated_controller_shape` sizes a fallback
capsule with when a skeleton carries no authored controller phantom. See
`docs/physics.md`, "Runtime-generated actor controller geometry, CMB-33", for
the capsule generator itself and its open gaps. `npc::with_height_weight` is
a new test-record builder mirroring the existing `with_faction`/`with_spell`/
`with_item` helpers, kept separate from `build_record` rather than added as
a sixth positional argument, since roughly twenty existing test files across
`crates/game/tests/` depend on `build_record`'s current signature. No test in
this crate calls the new helper yet, matching the task's no-new-tests
instruction. `cargo clippy -p soe-game --all-targets` confirms every
dependent test file still compiles against the unchanged signature.

## Contact actors and melee reach, CMB-28

`world/contact.rs` gives a host the shared state it needs to present actor
collision without owning any of it.

`World::contact_actors` lists every resident actor reference (`is_actor`)
that is effectively enabled, not deleted or destroyed, has a base and a cell,
and whose cell is the loaded cell or an attached one (`cell_is_attached`).
Prefetch alone does not attach a cell. Each entry carries the
reference's position and rotation in game units and radians, and a scale
equal to `actor_controller_scale` times the reference scale. An actor whose
controller scale cannot be derived keeps 1.0 for that factor, so a template
error never drops a body. Dead actors stay listed because a corpse still
blocks a blade. The list is sorted by form so a host sync is deterministic.
Because it reads the placement tables directly, the list follows cell
changes, streaming, enable state and scripted moves with no extra hook.

PRF-31 splits contact placement from controller-height resolution.
`contact_actor_placements` returns current attached placements with reference
scale. `resolve_contact_actor_scales` resolves current NPC Traits templates and
race height for a selected list, once per base in that call. Repeated resolution
uses the reference scale again, so it does not multiply the previous result.
The full contact list uses both steps. Detection selects living participants
before resolving their height. These frame-local views add no saved state.
Effective enable state follows the shared XESP reader, including parent changes.
For attached exterior grids, an actor's current position must lie in an attached
grid cell of its owning worldspace. The worldspace's persistent cell is an
authored grouping, so attaching it does not make distant actors across that
worldspace physically resident. Parent-cell identity remains unchanged.
The existing `CellIndex` supplies XCLC coordinates and world persistent-cell
identity, following local UESP `CELL.wikitext` and the winning record hierarchy.
`cell_coordinate_space` provides that worldspace identity for exterior pair
selection and aging of distant known pairs. Interiors keep separate cell spaces.
The local CK `Is3DLoaded`, `OnCellAttach` and `IsDisabled` pages distinguish
attached host resources from prefetch and current enable state.

`World::melee_reach` returns the reach in game units for the weapon an
attacker holds in one hand: `fCombatDistance` times the weapon's DNAM reach
times the attacker's reference scale. The formula is the one UESP's WEAP page
gives (`engine-refs/uesp-modfileformat/WEAP.wikitext`) and the one skymp's
`GetReach` computes (`engine-refs/skymp/skymp5-server/cpp/server_guest_lib/ActionListener.cpp`).
A load order without the setting falls back to `DEFAULT_COMBAT_DISTANCE`,
141, the vanilla value skymp's `unit/HitTest.cpp` uses. A present but
invalid setting is an error. A missing hand weapon is `Error::Equipment`.

Open: unarmed reach, which reads the race's unarmed reach instead of a
weapon, is not handled, so a fist swing reports `Error::Equipment`. The
reach scale uses the reference scale only, the UESP formula's `NPCScale`,
and not the race height.

## Dialogue range exit, DLG-06

Every unpaused `Game::tick` checks the current conversation after AI movement
and pending Papyrus work. `Game::update_dialogue_range` also exposes that check
for a caller that needs a typed error. It reads both participants' current
reference positions and the resolved `fAIInDialogueModeWithPlayerDistance`
GMST, including runtime setting overrides. Distance is three dimensional.
The exact boundary remains in range. Distinct interiors or worldspaces are
out of range even if their coordinates coincide. Adjacent exterior cells in
the same worldspace share coordinates.

Leaving range finishes the active INFO through the existing named VMAD end
fragment dispatcher and closes the session. A multi-response INFO ends once,
even if its first response was interrupted. Shared responses keep the selected
INFO's fragment ownership. Between responses, closure does not replay the
previous INFO's end fragment. Later ticks have no session to end again.
Explicit `close_dialogue` remains a cancellation without fragment dispatch.
Invalid settings or non-finite positions return an error without consuming the
cursor. Ticks record these errors in the VM log. A failed end dispatch retains
the cursor for retry, and latent fragment execution follows the VM scheduler.

Sources read were local `ck-wiki/GetDialogueTarget_-_Actor.wikitext` and
`IsInDialogueWithPlayer_-_ObjectReference.wikitext` for conversation visibility,
`TES5Edit/Core/wbDefinitionsTES5.pas` for INFO flags, TWAT and named VMAD
OnBegin/OnEnd fragments, and `uesp-modfileformat/INFO.wikitext` for the same
fields. The broader CK Topic Info and Player Dialogue pages were absent from
the corpus and returned HTTP 403 on retrieval. The installed `Skyrim.esm`
GMST supplies the 500-unit fallback, also checked against the five-master
winning record. Range-driven dispatch is this row's required contract, not a
claim of measured original-engine interruption timing. These schema sources
supply no reusable Rust conversation driver, so the existing game interpreter
handles the exit without another parser or dependency.

`dialogue.rs` integration tests prefixed `dlg_06_` cover range exits during
either response, end-fragment quest effects and single dispatch, idle exits,
record and runtime range overrides, invalid input, menu pause, shared response
ownership and separate interiors. `dlg_06_installed_distance_record_drives_walk_away`
is gated on `SKYRIM_DATA` and applies the installed distance record to a
synthetic scripted conversation. It does not execute a shipped quest fragment.
The row's range-exit and recorded-end checks are covered. TWAT reply selection,
walk-away menu flags, voice cancellation and original-engine timing comparison
are outside this range-exit implementation.

## Trainer purchases, DLG-16

`World::purchase_training` reads the trainer reference's NPC_ base, resolves
its winning `CNAM` class, and uses CLAS DATA to select the taught skill and
trainer tier. The tier limits training to skill levels 25, 50, 75, 90 or 100.
One purchase raises that skill's base actor value by one and removes the price
from the player's Gold001 inventory, transferring it to the trainer. Price is
the current skill value times `fTrainingMultCost`, rounded up. The shared
inventory transaction queues the ordinary item count event for both actors.

The player can buy five training sessions per character level. The world save
section named `DLG-16` retains the session count and its level, and the
determinism hash includes both values. A level change begins a fresh count.
Invalid trainer data, an exhausted tier or session limit, invalid price data,
and insufficient gold are rejected before gold, skill or session state changes.

The field layout follows xEdit `wbRecord(CLAS)` and UESP CLAS. UESP describes
the taught skill as an actor value index minus six and the training level as
the trainer's maximum skill level. CommonLibSSE-NG `PlayerCharacter.h` names
the per-level session counter, and the CK `Game.ShowTrainingMenu` page
establishes the trainer actor passed to the menu. `Game.ShowTrainingMenu`
queues a transient host request for a valid trainer. PlayerNode consumes it
after a game tick and presents the skill, current value, tier cap, current
price and remaining sessions from the same World state used by
`purchase_training`. The offer remains visible at the skill or session cap,
while the purchase path rejects it without changing state.

## Real mod data application, MOD-07

The Devious Devices KID rule from `Devious Devices_KID.ini` is applied through
`Game::apply_mod_data`. Its fixture keeps the source filename and line number,
and the test checks the added keyword through the same world query used by
gameplay. The source keyword is supplied by a minimal load-order record so the
test does not require the external mod installation.

## Open Animation Replacer selection, ANM-09

`Game::apply_mod_data` retains OAR submods in descending priority order.
`Game::select_oar_animation` checks each submod's conditions against the actor
base and current race, then returns an HKX file with the requested clip filename
from the first passing submod. `IsFemale`, `IsActorBase` and `IsRace` are
evaluated. Unknown,
unresolved and plugin-registered conditions fail closed. JSON condition arrays
are ANDed. Legacy OR groups accept any passing member, while the groups are
ANDed. Legacy actor-base and race conditions resolve their quoted plugin and
hex form ID pairs through the load order. A condition that requires an
unavailable SKSE plugin cannot select a clip. This selects a path for a host
animation system, it does not replace a live Havok clip yet.

`anm_09_` tests use authored records and synthetic OAR directories to check
conditioned selection, priority order and unsupported-condition rejection.

## Saved static-object changes, SAV-42

`World::restore_vanilla_static_objects` maps CONT, MISC and FURN references
through the saved plugin names, then applies all prepared record overrides
together. The xEdit form-flags word is an inversion mask over the live record
flags. The value and full-name fields consume zero bytes, so their markers and
opaque suffix bytes remain saved metadata without replacing record subrecords.
Record lookup, names and container inventory read through the shared overridden
record table. Duplicate, missing or malformed changes do not install a partial
set. SOEW version 37 saves the decoded fields and reapplies the mask on load.
Tests `sav_42_` cover all three record types, independent queries, engine
save/load and rollback after a late invalid target.

## Saved list changes, SAV-52

`World::restore_vanilla_lists` applies decoded FLST, LVLN, LVLI and LVSP
changes to record overrides. Saved references map through plugin names before
any override is installed. The record retains decoded additions, including
duplicate and null references. FormList queries follow the CK saved-addition
rule: they skip duplicates and nulls, with created forms before authored forms.
The method preserves form flag words, optional section presence and leveled
entry chance bytes. Leveled additions become twelve-byte LVLO subrecords and
update LLCT when present. Warm form-list
caches receive the same visible entries as cold queries. Warm leveled-list
caches receive the saved additions. Malformed changes, duplicate targets,
missing records and count overflow leave
all four list types unchanged.

The record overrides feed form-list queries, leveled selection and other
record readers. SOEW version 35 stores the saved flag words and entry chance
bytes alongside the overridden records. The chance byte remains available as
saved data without altering leveled selection probability. The implementation
follows local xEdit `Core/wbDefinitionsTES5Saves.pas` change types 43 to 46,
CK `AddForm_-_FormList.wikitext` for import order and duplicate handling, and
FallrimTools `ChangeFormFLST.java`, `ChangeFormLeveled.java` and
`LeveledEntry.java`. Their Java readers cannot serve as Rust runtime code, so
the existing typed save decoder and record override layer perform the mapping.

## BOOK, NOTE and KEYM save restoration, SAV-36

`World::restore_vanilla_book_note_keys` maps saved plugin identities and
prepares every decoded BOOK, NOTE and KEYM override before installing any.
Saved form flags update the record header flags. The independent record query
and name lookup read the override. xEdit defines VALUE and FULL_NAME as
payload-free change markers, so their flags are retained as metadata with the
BOOK, NOTE and KEYM teaching and read markers. Opaque suffix bytes are also
retained. Duplicate changes, missing or wrong records, and records already
overridden by another restoration path fail without partial mutation. SOEW
version 37 persists the record flags and saved markers.

## Saved projectile changes, SAV-24

`World::restore_vanilla_projectiles` maps decoded PFLA, PHZD, PBAR, PCON and
PARW change forms to winning records and installs all changes atomically. The
runtime query `World::restored_projectile_extra` returns the change flag word
and exact stored payload bytes for each restored form. The `SAV-24` world
section persists the result within SOEW version 37. TES5Edit
`Core/wbDefinitionsTES5Saves.pas` leaves these subtype payload structures
empty, so the importer does not map payload bytes onto plugin subrecords or
claim subtype fields are implemented. PCON placement is also decoded by the
reference reconstruction path. PARW placement stays opaque because xEdit
omits type 47 from its initial-data dispatch and FallrimTools uses its raw
fallback.

## Scripted knockback and ragdoll requests, CMB-38

`ObjectReference.PushActorAway` queues a radial impulse from the calling
reference to the supplied actor, preserving negative force as a pull.
`KnockAreaEffect` validates a finite magnitude from 0 to 1 and a positive
finite radius, then queues impulses for enabled actor references inside that
radius. Zero vectors are discarded without changing actor state. Its linear
attenuation is an engine approximation: the CK page gives magnitude and radius
but does not specify a falloff formula. Nonzero impulses mark the target actor
unconscious in the shared actor state. The host consumes queued vectors once.
They are transient and do not enter the save.

`ForceAddRagdollToWorld` and `ForceRemoveRagdollFromWorld` store an explicit
ragdoll override in the actor state. The `CMB-38` named actor save section
persists that override without changing `SAVE_VERSION`. An absent override
continues to follow death or unconscious state. Removing a ragdoll preserves
the actor's independent unconscious state. The CK pages do not define an impulse
unit conversion or explosion game-setting evaluation, so those details remain
outside this implementation.

`cmb_38_area_magnitude_and_radius_follow_the_ck_argument_contract` checks the
documented magnitude and radius inputs. `godot/cmb_38_ragdoll_test.gd` loads a
vanilla actor, toggles its Jolt ragdoll through the host bridge, and checks
that queued push and area impulses change the root body's velocity before
removal restores the animated representation.

Source pages read: `engine-refs/ck-wiki/PushActorAway_-_ObjectReference.wikitext`,
`KnockAreaEffect_-_ObjectReference.wikitext`,
`ForceAddRagdollToWorld_-_ObjectReference.wikitext` and
`ForceRemoveRagdollFromWorld_-_ObjectReference.wikitext`.

## Trigger occupants, HST-14

`World::update_trigger_actor_position` reads winning placed `REFR` records in
the actor's current cell and recognizes XPRM box and sphere primitives. Their
bounds, placement transform and reference scale determine containment. The
world retains current actor membership as derived state. A membership change
queues `OnTriggerEnter(akActionRef)` or `OnTriggerLeave(akActionRef)` on the
trigger reference once, and `World::trigger_object_count` returns the current
count for a supported trigger. Unsupported primitive kinds and malformed
XPRM payloads are not treated as triggers. Calling
`GetTriggerObjectCount` on one records an error diagnostic and returns zero.

The XPRM field layout follows local xEdit `Core/wbDefinitionsTES5.pas`.
Event signatures and the count contract follow local CK wiki pages
`OnTriggerEnter_-_ObjectReference.wikitext`,
`OnTriggerLeave_-_ObjectReference.wikitext` and
`GetTriggerObjectCount_-_ObjectReference.wikitext`. Occupancy is derived and
is not included in saves. Movement updates must be supplied by the host or
another actor movement system.

## Saved APPA, IDLM, WOOP and MGEF changes, SAV-44 and SAV-50

`World::restore_vanilla_base_objects` maps and validates these changes before
installing record overrides. APPA and IDLM form-flag masks are XORed with the
winning record flags. WOOP and MGEF retain their six-byte form flags, value and
full-name markers, and unnamed change bits in the SAV-50 world section. xEdit
defines the value and full-name fields as zero-byte unions, so no record
subrecords are replaced. A malformed or duplicate later change leaves every
record unchanged. SOEW version 37 persists the metadata in named world sections.
The layouts follow local xEdit `Core/wbDefinitionsTES5Saves.pas` change types
19, 27, 34 and 35.

## Saved PACK and NAVM changes, SAV-48

`World::restore_vanilla_pack_navmesh` validates all decoded PACK and NAVM
changes before installing any record override. Package and navmesh records
remain readable through the shared world record table. The override retains
the optional six-byte form flags and the PACK waiting and never-run markers or
NAVM value and full-name markers. `restored_pack_extra` and
`restored_navmesh_extra` expose those saved fields to runtime queries. SOEW
persists this metadata in the `SAV-48` world section alongside the matching
record overrides. `SAV-48` follows the version 37 section table, so it does not
raise `SAVE_VERSION`. `ai::package` carries the saved waiting and never-run
markers on its parsed package view. The field order and marker meanings follow
xEdit `Core/wbDefinitionsTES5Saves.pas`.
The `sav_48_pack_navmesh_restore_is_atomic_and_survives_engine_save_load`
fixture exercises both record types, query results, failure atomicity and
engine save/load. `sav_48_installed_pack_and_navmesh_records_accept_restored_markers`
checks installed Skyrim.esm records when `SKYRIM_DATA` is set.

## Host screenshot requests, HST-19

`Debug.TakeScreenshot(path)` queues the caller's string in `World` for the
host. The queue is transient and is not part of saved state. `PlayerNode`
drains requests after its gameplay tick and asks the active viewport texture
for an image saved as PNG at each supplied path. A request made without an
active viewport or a path that Godot cannot write is reported to the Godot
log.

`hst_19_screenshot_request.rs` calls the native through a Papyrus probe and
checks that the path is queued once and consumed. The Godot check is
`godot/hst_19_screenshot_capture_test.gd`. It requires installed data to start
the host gameplay runtime, then checks the resulting PNG in a windowed run.

## Equipment conditions, AI-49

CTDA functions 41, 48, 102, 103, 108 and 109 read the subject actor's
equipped forms. Clothing value totals equipped ARMO records whose BOD2/BODT
type is clothing, gold counts `Gold001` in inventory, torch conditions require
an equipped LIGH with DATA flag 0x0002, and shield conditions check shield-slot
ARMO records. UESP `LIGH.wikitext` names 0x0002 as "Can be carried". The local
CK corpus has no condition page for `IsTorchOut`, so this follows that authored
light flag rather than treating every LIGH as a torch. The weapon
queries use the right hand before the left. The IDs and parameter types come
from xEdit `wbConditionFunctions` in
`TES5Edit/Core/wbDefinitionsTES5.pas`. The Papyrus Extender condition parser
also confirms the ActorValue and FormType parameter encodings. Armor and
weapon fields use the transient views in `combat/records.rs`.

The CK wiki dump has no contract pages for these CTDA function names.
`GetIsUsedItem` and `GetIsUsedItemType` therefore return
`conditions::Error::UsedItemContext`: runtime state does not retain the
action's used item, and the local CK corpus does not define a substitute
interpretation. Invalid subjects and malformed equipment records fail with
typed condition errors.

## Component mods, SCR-03

`Game::load_wasm_mod` compiles and initializes a Wasmtime component using the
versioned `soe:mod-api@0.1.0` WIT package in `crates/game/wit/mod-api.wit`.
Components can subscribe to `OnActivate` and request `SetStage` with a resolved
form file and object pair. The host queues the request and applies it through
`Game::set_stage`, so quest state and queued Papyrus work remain in the shared
world and VM. Each guest call has a 100,000 instruction fuel budget. The
linker provides the engine interface and WASI Preview 2 with an empty context,
without inherited stdio, environment, arguments or preopened directories.

`Game::activate` and `Game::activate_deferred` dispatch subscribed callbacks
after the normal activation succeeds. The Rust guest fixture in
`tests/fixtures/scr_03_mod` builds to `wasm32-wasip2`. Its integration test
subscribes during initialization and advances a synthetic quest from its
activation callback. The host API currently contains these two operations.

The component embedding follows [Wasmtime's component model
API](https://docs.wasmtime.dev/api/wasmtime/component/index.html) and the
[Rust component build
guide](https://component-model.bytecodealliance.org/language-support/creating-runnable-components/rust.html).
The project decision and acceptance check are in `ENGINE-PLAN.md`, round 4.

## CommonLib magic provider, SHM-13

`CommonLibMagic` adapts favorites, casting checks, active magic effects,
reference marker flags and default equipment requests through the MIT
`MagicWorldState` provider. Favorite forms are stored on their actor and
persisted by the `SHM-13` actor save section. Marker visibility and fast travel
use the existing reference flags. `ShowAllHidden`, unsupported cast checks
and non-default equip parameters return explicit unsupported-state errors.
Supported equipment requests use `World::equip_item`, and dispel requests use
the shared active-effect removal path.

## Deterministic tick work, PRF-14

`Game::last_work_counts` returns the ten operation counters from the last
`tick` or `tick_menu_mode`. Each tick starts a scoped work window. An enclosing
scenario or host window includes that tick once. CTDA counts charge each
attempted predicate, including errors, and exclude predicates skipped by OR
short circuit. Runtime record overrides and created base records charge one
lookup, while winning-record fallback charges through `LoadOrder`. Counters
are transient diagnostics and have no saved fields. Quest-burst and error
checks live in `tests/prf_14_work.rs`.

## AI planning slices, PRF-11

The package driver submits navigation requests through a transient per-world
planner keyed by resolved actor FormID. Each request processes at most 64
nodes per frame, and all requests share a 512-node frame budget. Pending actors
retain their current package step and position until the result is ready.
The scheduler rotates its starting actor when the frame budget is exhausted,
so later FormIDs continue making progress. Requests are cancelled when their
goal or start position changes, the package switches, navigation is invalidated,
or the actor unloads, dies or becomes unconscious. Explicit package reevaluation
also cancels a pending request. Forced fleeing retains the request while its
effect remains active.

Package travel, combat pursuit, forced fleeing and Orbit connectivity checks
use sliced A*. Flee destination selection also budgets its reachability flood
and candidate queries. Its heap orders candidates by decreasing threat distance,
then increasing NodeId, preserving the previous destination selection.
Normal package movement retains its empty-route result for disconnected
on-mesh endpoints and its straight-line fallback for off-mesh endpoints.
Combat queries keep their no-route result without that fallback.
Door states refresh once per driver tick, rather than once per actor request.
Graph query caches are prepared when the world builds its NAVM graph.

Requests and the last tick's measured `AiWorld::planning_time` are runtime
caches and are not serialized. Existing actor destinations, paths and package
state keep their saved layout. The design follows the local
`recastnavigation/Detour/Source/DetourNavMeshQuery.cpp` sliced-query contract,
implemented on the engine's authored NAVM graph through `soe-navmesh`.

Cell reference lookup reuses the load order's cell index already prepared by
cell loading, filters to winning REFR and ACHR identities, and returns them
in FormID order. It does not build a second world-sized index inside sandbox
planning. Combat acquisition starts from Combat-awareness entries in the
existing detection pools and retains its loaded-actor eligibility, nearest
position and FormID tie rules. Unobserved actors are rejected before actor
record queries. The last tick's `door_refresh_time` exposes the refresh portion
of the full AI timing for measurement.

Automatic package evaluation runs at most four actors per frame and at most
once per actor in that frame. Explicit reevaluation requests take priority,
then the oldest existing `last_evaluation` timestamp, then FormID. Actors
waiting for an evaluation slot continue their existing step. Movement retains
its FormID iteration order. Explicit `EvaluatePackage` calls still evaluate
immediately. `evaluation_time` measures automatic evaluations within the full
AI tick. Evaluation scheduling uses existing actor fields and adds no saved
state.

The transient `planning_phases` table resets on each AI tick and records inclusive
wall-clock durations for scheduling, resumed navigation, actor processing,
selection, conditions, switching, combat checks, movement and path requests.
The host exposes these alongside total planning time. Nested phase durations
overlap, so they are not a partition of total time.

### Navigation workers, PRF-17

Pending navigation requests resume on Rayon workers borrowing the world's NAVM
graph and mesh-to-cell table. The dispatcher reserves node budgets before each
batch and applies results in the previous rotating FormID order. Unused node
reservations feed the next batch. Both one-worker and multi-worker execution
retain the 64-node request and 512-node frame budgets. No request can advance
twice in a frame. Cancellation and graph revision checks precede dispatch.

`AiWorld::set_planning_workers` installs a dedicated pool with a nonzero size.
Pool creation errors preserve the existing pool and pending work. Without an
explicit pool, resumed requests use Rayon's shared pool, whose worker count
can be set through `RAYON_NUM_THREADS` before its first use.
`AiWorld::planning_workers` exposes completed jobs, busy time per worker,
caller dispatch/join time and utilisation for the last tick. These measurements
cover resumed navigation only. Busy time measures elapsed request execution,
not operating-system CPU accounting. Its `phase_busy` table sums worker search
durations and can exceed frame wall time. These sums stay separate from the
calling thread's `planning_phases` timings. PRF-14's active work recorder enters each job,
so worker path and triangle operations remain in the caller's frame totals.
The resumed navigation dispatch/join interval is charged once to PRF-12's
AI planning system, without adding worker busy durations to frame wall time.

The dispatcher uses exclusive request borrows, with no application mutex held
or acquired by a job. There is no per-lock wait measurement for Rayon's internal
scheduler. Worker configuration, requests and timing data are transient and do
not change saves. The pool is joined before world mutation resumes.

Initial request setup and its first slice still run on the calling thread,
as do package evaluation, marker selection, combat and movement preparation. Papyrus
execution is unchanged. [threading.md](threading.md) records the reference
evidence and the unimplemented parts of PRF-17. The dependency uses the already
locked Rayon, rayon-core, either and Crossbeam utilities.

### Process cadence and waypoint workers, PRF-36

With a placed player, the driver selects high, middle-high, middle-low and low
process levels. The distance bands are 4,096, 8,192 and 16,384 units. High actors
update every tick, middle-high every 0.1 seconds, middle-low every 0.5 seconds
and low every 5 seconds. Actors in separate interiors or worldspaces are low
even when their local coordinates overlap. Combat actors, teammates, scene
packages, ForceGreet and DialogueActivate use high processing. Explicit package
reevaluation bypasses the cadence. Without a placed player, every actor uses
high processing. Coarse updates accumulate simulation delta for movement and
stagger their first update by FormID. Actor-owned scheduling clocks retain the
elapsed remainder and next update in the PRF-36 actor save section. An absent
section leaves the clock uninitialized. Invalid or non-finite clock values are
load errors. Automatic selection retries use the one-second interval even
when the previous attempt selected no package.

The levels come from local CommonLibSSE-NG `AIProcess.h` and `ProcessLists.h`.
OpenMW `mwmechanics/actors.cpp` supplies a player-distance processing example.
Those sources do not define these distances or cadences, which are engine
policy. The driver retains package conditions, authored speeds, path ownership,
FormID movement order and immediate explicit EvaluatePackage calls.
Scheduling resolves coordinate spaces only for actors in another cell within
the widest distance band. Distant actors need no cell-space lookup, while
combat, teammate and scene overrides still select high processing.

Immutable winning PACK definitions and procedure trees are cached by resolved
FormID. A runtime record override bypasses the cache, including an override in
any template dependency. Waiting and never-run markers still come from the live
record table. These caches contain definitions, not actor progress.

Waypoint steering borrows the actor's detached AI state on the PRF-17 pool.
The caller joins before applying position, cell changes, arrival effects and
events in their existing order. No world or VM is sent to the worker. Patrol
radius stopping and the four-unit arrival tolerance retain their existing
rules. `movement_workers` reports jobs, worker busy time and caller dispatch
and join time separately. The caller's wait remains in PRF-12 main-thread AI
time. Empty routes do not dispatch a waypoint job. Process counts and updates
report each level. SAVE_VERSION stays at 37. No dependency or per-reference
host node is added.

Marker and bed base filtering reads existing reference state first, or the
winning placement's NAME, XLCN and cell when absent. It shares BOS base selection
with reference initialization, without choosing actor templates or initializing
inventory. Only eligible marker references need initialization.
The field layouts follow local xEdit `wbDefinitionsTES5.pas` REFR and ACHR
definitions and the existing `soe-esm` REFR reader.
Idle-marker reservation checks read already initialized actors. This avoids
initializing unrelated actors while looking for furniture or idle markers.
Navigation diagnostics also record request setup, search advancement, route
result conversion and fallback containment, flee search, procedure tree entry,
marker selection and Turn Undead checks.

Sandbox marker queries also retain their cursor and candidate FormIDs across
frames. Each request examines at most 64 references in a tick and the frame
shares 64 reference examinations. The scheduler rotates pending actors when
the shared budget is exhausted. Pending actors hold their current step and
consume no marker-choice RNG until the query completes. Candidate order remains
FormID order. A changed package or search goal cancels the old query, and unloaded
actors lose it. Marker requests are transient and restart after save restoration.
Cell lookup, base filtering, bed classification and reference initialization
have separate timing entries. `marker_resume` measures retained candidate scans.

Failed navigation requests retain their completed result, including the empty
route for disconnected on-mesh endpoints. Repeating the same start and goal
performs no endpoint lookup or search. Changed starts, goals or graph revisions
discard that result. Existing package switches, explicit reevaluation and
navigation invalidation also cancel it, and unloading removes the actor's
request. Successful routes are consumed as before. These are transient query
results, with no saved fields. The retained-path model in local
`openmw/apps/openmw/mwmechanics/aipackage.cpp`, `pathTo` and
`doesPathNeedRecalc`, reuses successful paths but rebuilds empty ones. The
engine caches empty results too because repeated endpoint lookup was the
measured cost, and uses its NAVM revision to retry when walkability changes.

Door refresh retains sorted door identities and the last position and open
state applied to the graph. Mutable reference access marks bound doors for
comparison on the next tick. Translation, reference reset, native-save load
and vanilla reference import also mark affected doors. Unmarked doors need no
state scan or footprint query. `door_checks` counts comparisons for the last
tick. Changed positions and open states rebind or
update blocking and invalidate actor routes when graph walkability changes.
The cache is rebuilt with the graph and is not saved. Door associations still
follow local xEdit NVNM Door Links and CommonLibSSE-NG `BSNavmesh.h`.
Actor diagnostics also separate state, Calm and evaluation-eligibility checks
from combat checks, procedure-tree entry, marker work and path requests.

Sandbox request setup reads only the existing index's slice lengths. Candidate
scans merge its sorted persistent, temporary and deleted slices through three
retained offsets. Both tag filtering and marker classification fit the reference
budget, including the first tick, without copying or sorting a whole cell per
actor. This retains the same FormID order as `references_in_cell` and reads
live marker state during each bounded scan.

Ordinary `World::load_cell` prepares the world NAVM graph and query caches
after prefetching references and before attaching the cell. The first AI tick
therefore reads the prepared graph without relying on a host measurement hook.

## Shared logging, TOOL-06

Runtime ticks emit trace events and AI reference-index construction emits debug
events through `soe-trace`. AI events include the resolved cell FormID.
`Console::run` accepts `log` to read the active filter and `log <directives>`
to reload it. Invalid directives leave the current filter in place. A missing
host subscriber returns a console error. The command uses the shared
[logging controls](trace.md) and changes no saved game state.

## Reference enable parents, RND-17

`World::reference_enabled` reads the winning REFR or ACHR XESP chain and
combines its opposite-state bits with the terminal parent's runtime enable
state. A reference without a parent uses its existing runtime enabled field,
or its Initially Disabled record flag before construction. A parent overrides
the child's initial flag. Deleted references, missing links, truncated XESP
fields and cyclic chains return false, including opposite-state chains.
Queries do not instantiate references or choose actor templates.
`ref_placement`, Papyrus `IsEnabled` and `IsDisabled`, and `Is3DLoaded` use
this effective state. Runtime `Enable` and `Disable` change the existing
reference field, so parent changes reach descendants without copied state.
The existing reference save body stores that field.

The sources are local `uesp-modfileformat/REFR.wikitext` and
`TES5Edit/Core/wbDefinitionsTES5.pas`, `wbXESP`, for the link and flag layout.
The corpus has no separate CK Enable Parent page. Its
`ck-wiki/Enable_-_ObjectReference.wikitext` and
`Disable_-_ObjectReference.wikitext` describe children popping with their
parent. This query interprets those records in the shared World rather than
introducing a second reference database.

## Record sky inputs, RND-18

`World::sky_frame(cell)` reads winning CELL, WRLD, REGN, CLMT and WTHR forms.
Climate lookup follows the owning WRLD/CNAM, including WNAM parents whose
PNAM climate inheritance bit is set. CELL/XCCM selects a REGN. Its WNAM world
supplies the climate and its RDWT weather entry replaces the climate weather
list when present. Interior Show Sky and Use Sky Lighting flags are separate.
A sky can draw while `interior_lighting` applies XCLL inheritance bits
separately to LGTM ambient, directional and fog fields. Ordinary interiors
return no weather frame.

Interior lighting accepts complete optional four-byte tails from 64 to 92
bytes in XCLL and LGTM/DATA. Template directional ambient colors come from
LGTM/DALC, independently of DATA's unused color block. Ambient inheritance
selects all six directional colors with the scalar ambient color. Missing
optional far fog color uses the near color, maximum fog opacity defaults to
one, and absent light fade distances default to zero. Fog clip and light
fade use their own inheritance flags. Malformed DALC and nonfinite numeric
lighting fields return errors.

An initial weather uses CLMT/WLST weights and optional GLOB chance values,
with a repeatable draw from the cell identity. A Weather.SetActive request
replaces that selection. The override flag retains it across climate changes.
The snapshot samples the four NAM0 color entries through the CLMT sunrise and
sunset intervals. FNAM day and night distances, power and maximum opacity
blend through those intervals. The solar orbit uses their midpoints.
Moon visibility follows the two TNAM flags. Eight moon texture phases advance
by TNAM's phase length, starting at full. The moon orbit uses a five-day
Secunda offset from Masser.

Cloud textures follow the sequential ASCII layer tags and old DNAM, CNAM,
ANAM and BNAM aliases. LNAM bounds the layer count, NAM1 excludes disabled
layers, PNAM supplies time colors and JNAM supplies alpha. Finite alpha values
saturate to the rendering range. RNAM and QNAM decode signed UV velocities
around byte 127. ONAM is the unused old speed field in xEdit.

Changing weather retains weighted source FormIDs and starts the destination's
DATA transition duration, scaled from its byte to 0 to 0.25 game hours.
Acceleration chooses the smallest nonzero encoded duration. Interrupted
transitions retain the current weights. Colors, fog and cloud contributions
are sampled from these forms each frame. World owns the transition, the host
retains no authoritative weather copy. Section RND-18 saves the requests,
climate or region selection, target, source weights and timing through
FormReader. SAVE_VERSION stays 37. A load failure restores the live weather state.

Sources are local UESP `WTHR.wikitext`, `CLMT.wikitext`, `CELL.wikitext`,
`LGTM.wikitext` and `REGN.wikitext`, xEdit `wbDefinitionsTES5.pas`, and OpenMW
`apps/openmw/mwrender/sky.cpp` and `skyutil.cpp`. OpenMW separates sky colors,
cloud scrolling, celestial bodies and weather blending. Its OSG renderer
cannot be used by the Godot host, so this engine samples Skyrim records and
uses a Godot sky shader. The named CK Weather page is absent from the local
corpus. Its [Weather documentation mirror](https://tesck.ru/index.php/Weather)
describes Trans Delta as a duration in game hours. The local format sources
supply the byte scaling.
The CELL XCCM target follows xEdit's Skyrim REGN definition, rather than the
older UESP CELL page's assumed climate interpretation.

The `rnd_18_` integration tests exercise time slots, cloud encodings, fog,
sun and moon inputs, weather changes, interruption, acceleration, interior
inheritance, winning overrides, save restoration and malformed inputs. The
installed-data survey reads all 108 live weather winners and nine climates
from the five masters, checks cells linked to lighting templates, then samples
Whiterun at noon and midnight against the winning NAM0 bytes.

## Walkthrough ambient speech and sounds, AUD-11

An unpaused tick selects a Misc Hello or Idle topic when an eligible loaded
actor enters dialogue range and no conversation is open. Numeric DIAL subtypes
79 and 94 and SNAM HELO and IDLE follow local xEdit
`TES5Edit/Core/wbDefinitionsTES5.pas`. Selection uses the existing quest,
INFO, speaker and CTDA checks, quest priority, topic priority and FormID order.
The same conversation session carries the response and fragment lifecycle.
Explicit player conversations finish the ambient INFO before opening their
new session. The range edge prevents speech from restarting every frame. Force
greetings retain their priority arbitration. Dead, combat, unconscious,
disabled, translucent non-ghost and PC-dialogue-disabled actors cannot start
speech. The local CK `AllowPCDialogue_-_Actor` page supplies the PC-dialogue
rule. An active PACK must enable PKDT interrupt bit 0 for Hello or bit 7 for
Idle chatter. Actors without an active package have no package restriction.

`ambience::descriptors_for_cell` follows CELL/XCAS to ASPC/SNAM for interior
ambient sound and interior-only ASPC/RDAT for regional sounds. CELL/XCLR also
supplies regions. REGN sound entries use RDSA weather masks and stored
chances, ordered by RDAT priority with override entries ending lower-priority
selection. It returns resolved descriptor identities, leaving SNDR conditions
and playback parameters to `sound::prepare_for`. Layout sources are local UESP
ASPC and REGN pages and xEdit `wbDefinitionsCommon.pas` `wbRegionSounds`.
The existing MUST track lookup remains available separately.

`music_type_for_cell` resolves CELL/XCMO, regional RDMO and owning WRLD/ZNAM
in that order, retaining an explicit null CELL music type as silence.
No saved fields or save version change are introduced.

## Papyrus frame health, PRF-19

`Game::papyrus_health` combines VM diagnostics with current update timers and
pending world events. Its totals count object timers once, while each attached
script row shows that object's registrations. Repeating, single, repeating
game-time and single game-time timers have separate counts. Repeated native
registration replaces only the same mode on that object, while unregistering
removes both modes in its clock domain. Quest stop removes quest and alias
timers. Active-effect removal uses the existing timer cleanup.

Script rows group under the plugin supplying their winning VMAD attachment.
For objects attached through the public VM API, the fallback reads their
reference base, form, quest alias owner or active-effect definition. Unknown
origins and global calls use `<unattributed>`. This is attachment ownership,
not PEX asset provenance. Pending event deliveries remain visible before pump,
then VM queues retain them until the first instruction executes. The report
contains per-script and per-plugin counts, frame CPU and suspended durations,
and separately metered session event peaks. It does not mutate world state.

The timer rules follow local CK `RegisterForUpdate_-_Form.wikitext`,
`RegisterForSingleUpdate_-_Form.wikitext`, their game-time variants and
`OnUpdate_-_Form.wikitext`. They describe replacement, clock domains, same-object
fanout and lifetime cleanup. Tests distinguish 1,000 registration calls on one
object from 1,000 registered objects, add 500 individual inventory items through
the notification path, verify shared-timer fanout and quest cleanup, and check
winning ownership on the installed player form when `SKYRIM_DATA` is set.

## Load timing phases, PRF-29

String-table loading records the language as an aggregate operation and each
companion table's parse under its VFS path. Script loading records the aggregate
scripts operation and each PEX parse. Placed-reference script binding records
the reference and base form as object ownership around its CPU binding phase.
The first shared navigation graph construction records a `navmesh_build` CPU
phase over winning NAVM records. Subsequent graph access adds no build phase.

Engine save loading records an aggregate `engine_save_load` and its parse/restore
phase, including validation failures. `soe-trace` reports aggregate budgets and
slowest retained cause phases. These measurements use the existing load/bind
contracts and PRF-08 boundaries in `docs/performance.md`. They add no saved
fields and leave world and VM layouts unchanged.

## Reference presentation changes, PRF-34

`World::reference_enabled` caches effective winning-record enable state. Its
XESP dependency edges invalidate cached descendants when runtime reference
state changes. The mutation accessor and internal placement, deletion,
inventory removal and cell eviction paths notify independent `ReferenceChanges`
observers. Each observer compares only notified reference placements, effective
visibility, destruction models and open state. An unchanged mutable access
produces no host update. Attaching a reference queues its initial presentation.
The caches and observers are transient. Loading world state invalidates them
without changing the saved reference layout.

The enable rules remain those described under RND-17, from local UESP REFR
and xEdit `wbXESP`. CK Enable and Disable pages describe children popping with
parents. The host consumes these derived views of the shared World.

AUD-13 compares the RDSA chance float directly with a normalized draw. Vanilla
region sounds store 1.0 for continuous ambience and fractions for occasional
sounds. Equal draws exclude an entry. The parser retains Mutagen's accepted
0 to 100 field range, so stored chances of at least 1 always pass. The source
layouts are UESP REGN and xEdit's `wbRegionSounds`, with the chance range from
Mutagen's Skyrim `Region.xml`. The installed city and tundra tests require a
selected ordinary loop with a draw of 0.25. No ambient state is added to saves.
