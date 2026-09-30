# soe-game: the runtime form table and the first natives

The combat integration includes race-based unarmed damage, difficulty selection,
template-backed hits,
armor slot replacement and explicit weapon hands. SOEW version 20 retains
both difficulty and hands, with separate readers for the version 16 and 17
branch layouts. The authored contact bridge reaches the shared Game through
animation events. Live host collision synchronization and attack input are
separate unfinished rows. See [combat](combat.md) and BACKLOG.csv.

Crate `crates/game`. Round 2 of `ENGINE-PLAN.md`, written 2026-09-14. The map's
week 6 to 9 gate.

## What is in it

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
- `vmad`: the `VMAD` subrecord. Scripts, their filled properties, quest stage
  fragments, INFO and PACK begin, end and change fragments, SCEN begin, end
  and phase fragments, and alias
  scripts. Round-trips through `write` at the version and object format the
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
  whole batch of observations before it changes any pool. See
  [combat AI](combat-ai.md) for the formula sources and what remains.

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
  reference's current cell and position with the supplied actor owner. Sound
  levels above 100 remain valid. Invalid actors, references and negative levels
  leave the queue unchanged. The host takes queued events once per observation
  step. Emitted sound uses the source's distance and line of sight, independently
  of the owner's position. The queue is transient and adds no save field.

- `quest`: the `QUST` view. Flags, stages with log entries, objectives,
  aliases with forced references, and each objective target's alias, lock
  marker flag and CTDA conditions, plus local or external alias dependencies.
- `natives`: the base game's natives, closed over the shared world.
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
  It parses plugins, BSA/BA2 members, NIF, HKX, DDS, PEX, FUZ/LIP, xWMA and
  WAV containers. Wabbajack's local README describes modlists as reproduced
  without bundling assets, so the sweep reads the installed files in place.
- `Game`: owns the VM and the world. `load_scripts_from_data` builds the load
  order's `soe_bsa::Vfs` (each plugin's `<stem> - Misc.bsa` or `<stem>.bsa`
  archive if the install shipped one, loose files on top) and loads every
  `.pex` it answers under `scripts/`, so an install that ships its scripts
  packed rather than loose still has something for `bind` to attach. `bind`
  attaches every script the load order names and fills its properties, `start`
  also starts the quests flagged Start Game Enabled, `tick` advances time,
  `activate` and `send_event` do what a player or the engine would, `pump`
  drains what natives queued.

## Design

### MCM script lifecycle, UI-05

`Game::start` registers MCM scripts after quest initialization.
`Game::mcm_configs` scans running quest forms in winning load-order order and
reads `SKI_ConfigBase.ModName` and `Pages` from their attached Papyrus scripts.
It registers at most 128 configs, delivers `OnConfigRegister` once per config,
and returns the current page labels. `mcm_open`, `mcm_select_page` and
`mcm_close` call the script's `OpenConfig`, `SetPage` and `CloseConfig`
functions. Those functions deliver the initial empty-page reset, selected-page
reset, open and close callbacks. The registry and active menu selection are
session UI state, while page labels remain script properties. This bridge
does not execute SkyUI's Scaleform panel or its option APIs. The lifecycle
order and limit come from the local SkyUI `SKI_ConfigBase.psc` and
`SKI_ConfigManager.psc` sources.
The tests assemble equivalent lifecycle methods in Papyrus bytecode. They do
not execute an unmodified SkyUI script, so full script compatibility is
unverified.

### Furniture activation, HST-09

Activating a FURN reference selects the first unreserved Sit marker from its
FNPR entries whose corresponding low MNAM bit is active. The actor reserves
that marker and moves to the furniture reference transform. Activating the
same furniture again, or calling `World::stand_from_furniture`, releases the
reservation. `World::furniture_seat` exposes the actor's current furniture and
marker, and `World::furniture_marker_in_use` reports active reservations.
SOEW version 37 stores the furniture reference and marker with actor state.
The FURN field layout follows local xEdit `wbRecord(FURN)` and
`uesp-modfileformat/FURN.wikitext`. Marker types and the active MNAM mask come
from those definitions. Marker transforms are not read from the furniture
model by this path.

Patrol marker data reads `XPRD`, `INAM` and typed `PDTO` entries from each
linked `REFR`, as defined by xEdit `wbRecord(REFR)` and Mutagen `Patrol.xml`. A
resolved `INAM` IDLE form emits its editor ID through the actor's animation
event path once when the actor arrives. A marker whose base is FURN uses the
existing Sit-marker reservation. The reservation is released before the route
advances, and the Patrol dwell deadline prevents the idle event from firing
again after save and restore. PDTO references and subtype fields are
validated, but they do not start a conversation. AI-23's UseIdleMarker
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
including regular and light slots. `Game::restore_vanilla_world_and_vm` runs
the supported restore passes in dependency order and restores the prior SOEG
snapshot if any pass fails. The passes cover globals, NPC changes, reference
placement, actor templates, inventories, reference extras, quests, lists and
Papyrus execution. The individual world passes validate before applying their
own changes. Saved reference-only objects are created using their saved
runtime type. Instances whose signed
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

Reference placement reserves created identities found in saved change forms,
the FormID array and the four Created Objects tables before runtime placement
can allocate another identity. Regular and light plugin IDs map by their saved
owner names. Their masks follow Mutagen's `FormID.cs`, while the FormID array
and Created Objects table order follow UESP's save format. The reservation
keeps identities stable, but it does not materialize saved enchantment, potion
or poison definitions as live records. Inventory restoration accepts a
missing base identity listed in those tables, applies item deltas by saved
identity and reports items without live records. Created Objects decoding
follows UESP `Created_Objects` and skymp's reader and writer field order in
`soe-save`.

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

**Natives cannot touch the VM.** They run inside a VM thread. Anything they
want the VM to do, an `OnActivate` event, an `OnInit`, a stage fragment call,
goes into `World::pending`, and `Game::run_until_idle` alternates draining that
queue with running the VM until no runnable work remains. Deferred death
requests stay queued until EndDeferredKill. Timers are the same: `tick`
moves due registrations into the queue as `OnUpdate` events.

**Provider collisions.** `Game::add_natives` runs a second provider (an
extender in the PAPER mould) and returns the functions it lost. First
registration wins, the registry never replaces silently. This is the answer to
the map's third open seam.

**Semantics.** Consult the local Creation Kit function pages and shipped PSC
wrappers before implementing natives. `engine-refs/ck-wiki/` contains 402
wikitext pages as of 2026-09-19. When an individual page is absent, class pages
and `engine-refs/skse64/scripts/vanilla` can establish the contract.
[The reference audit](reference-audit.md) records missed contracts and gaps.

`ObjectReference.GetCurrentScene` searches active SCEN records and resolves
their participant alias IDs through the owning quest. It returns the first
matching scene in sorted FormID order, or None if the reference is not an
active participant. The implementation follows the local
`GetCurrentScene_-_ObjectReference.wikitext` return contract. Animation
completion and placed-reference Havok behavior remain stubs because the world
has no latent animation event bridge or movable-reference host physics path.

**Quest alias dependencies.** The `QUST` reader retains local alias IDs from
`ALFA` and external quest and alias IDs from `ALEQ` plus `ALEA`.
`World::resolve_alias_ref` follows those local and external ID chains in
dependency order. A cycle returns `AliasResolutionError::Cycle` with the cycle
path, and resolution does not write partial alias values.
`World::alias_ref` preserves its optional result and returns `None` when a
dependency cycle prevents resolution. The layout follows `wbRecord(QUST)` in
`engine-refs/TES5Edit/Core/wbDefinitionsTES5.pas` and
`engine-refs/uesp-modfileformat/QUST.wikitext`. The CK `Alias_Script` page
establishes that aliases are objects owned by their quest. Fill selection for
location reference types and `ALFI` forced-into copying are not implemented.

**World reference placement.** `World::ref_placement` (in
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
parsed. A faction outside the perpetrator's own never gets charged, which
is narrower than vanilla when a `CRGR` link exists. A `FACT` with no
`CRVA` subrecord prices every crime at zero rather than failing, since a
faction can legally exist without one. No Papyrus native calls this
function yet, `SendAssaultAlarm` and similar witness-driven natives are
later work that can build on it.

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
Functions 602, 627 and 676 return an unavailable-state diagnostic because ward
state, dual-cast animation state and active voice variation are not tracked.

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

## Conformance table

`natives::register` has 367 stateful registrations, including
`CreateDetectionEvent`. Quiet stubs still return an idle value so vanilla
scripts can continue when their owning system has no behavior yet.

| Class | Implemented | Stubbed |
|---|---|---|
| Form | GetFormID, GetName, GetType, HasKeyword, GetKeywords, RegisterForSingleUpdate, RegisterForUpdate, RegisterForSingleUpdateGameTime, RegisterForUpdateGameTime, UnregisterForUpdate, UnregisterForUpdateGameTime, RegisterForSleep, UnregisterForSleep, RegisterForTrackedStatsEvent, UnregisterForTrackedStatsEvent, RegisterForAnimationEvent, UnregisterForAnimationEvent, RegisterForLOS, RegisterForSingleLOSGain, RegisterForSingleLOSLost, UnregisterForLOS | |
| ObjectReference | Enable, EnableNoWait, Disable, DisableNoWait, IsEnabled, IsDisabled, Delete, IsDeleted, GetBaseObject, GetParentCell, GetWorldSpace, IsInInterior, GetPositionX/Y/Z, GetAngleX/Y/Z, SetPosition, SetAngle, MoveTo, GetDistance, Activate, BlockActivation, IsActivationBlocked, GetLinkedRef, GetItemCount, AddItem, RemoveItem, RemoveAllItems, GetScale, SetScale, GetDisplayName, SetDisplayName, PlayAnimation, GetOpenState, SetOpen, Lock, IsLocked, GetLockLevel, SetLockLevel, PlaceAtMe, AddToMap, EnableFastTravel, SetDestroyed, IgnoreFriendlyHits, IsIgnoringFriendlyHits, GetCurrentLocation, GetEditorLocation, IsInLocation, MoveToMyEditorLocation, GetActorOwner, GetFactionOwner, SetActorOwner, SetFactionOwner, AddInventoryEventFilter, RemoveInventoryEventFilter, RemoveAllInventoryEventFilters, SetAnimationVariableFloat/Int/Bool, GetAnimationVariableFloat/Int/Bool, GetNthLinkedRef, PlaceActorAtMe, GetCurrentScene, CreateDetectionEvent | Is3DLoaded, IsFurnitureInUse, PlayAnimationAndWait, PlayGamebryoAnimation, SetMotionType, ApplyHavokImpulse, PushActorAway and 6 more, plus the DLC ragdoll, cart and translation: ForceAddRagdollToWorld, ForceRemoveRagdollFromWorld, TetherToHorse, IsFurnitureMarkerInUse, Reset, TranslateTo, SplineTranslateTo, StopTranslation |
| Actor | GetActorValue, GetBaseActorValue, SetActorValue, ModActorValue, DamageActorValue, RestoreActorValue, GetActorValuePercentage (and the AV short names), IsDead, Kill, GetKiller, Resurrect, IsInCombat, StartCombat, StopCombat, GetCombatTarget, GetActorBase, GetLeveledActorBase, IsEssential, IsPlayerTeammate, SetPlayerTeammate, IsGhost, SetGhost, IsProtected, GetLevel, GetRace, IsUnconscious, SetUnconscious, SetRestrained, IsDoingFavor, SetDoingFavor, AddSpell, RemoveSpell, HasSpell, EquipItem, UnequipItem, IsEquipped, IsInFaction, GetFactionRank, AddToFaction, RemoveFromFaction, SetFactionRank, AddPerk, RemovePerk, HasPerk, SetAlert, IsAlerted, StartSneaking, IsSneaking, DrawWeapon, SheatheWeapon, IsWeaponDrawn, IsArrested, ClearArrested, SetAttackActorOnSight, SetDontMove, SetAllowPCDialogue, SetRelationshipRank, GetRelationshipRank, GetHighestRelationshipRank, GetLowestRelationshipRank, GetCrimeFaction, SetCrimeFaction, SetOutfit, and from the DLC: SendVampirismStateChanged, SendLycanthropyStateChanged, SetAllowFlying, SetAllowFlyingEx, IsAllowedToFly, CanFlyHere, StartDeferredKill, EndDeferredKill, KillSilent, SetRace, SetEyeTexture, SetCriticalStage, AttachAshPile, SetNoBleedoutRecovery, GetNoBleedoutRecovery, EnableAI, AddShout, RemoveShout, EquipShout, UnequipShout, GetEquippedShout, EquipSpell, UnequipSpell, SetSubGraphFloatVariable, PlaySubGraphAnimation, WornHasKeyword, IsPlayersLastRiddenHorse, SetAlpha, PlayIdle, HasMagicEffect, HasMagicEffectWithKeyword, DispelSpell, DispelAllSpells | IsAlarmed, IsBleedingOut, GetCombatState, EvaluatePackage, GetEquippedWeapon, SetLookAt and 34 more, plus the DLC ones without a system: IsFlying, GetWarmthRating, ClearExtraArrows, DoCombatSpellApply, MoveToPackageLocation |
| ActorBase | GetRace, GetClass, GetSex, GetDeadCount, IsEssential, IsProtected, IsInvulnerable, IsUnique, SetEssential, SetProtected, SetInvulnerable, SetOutfit | GetGiftFilter |
| Quest | Start, Stop, Reset, IsRunning, IsStarting, IsStopping, IsCompleted, IsActive, SetActive, CompleteQuest, CompleteAllObjectives, FailAllObjectives, SetStage, SetCurrentStageID, GetStage, GetCurrentStageID, GetStageDone, IsStageDone, SetObjectiveDisplayed, SetObjectiveDisplayedNoWait, SetObjectiveCompleted, SetObjectiveFailed, IsObjectiveDisplayed, IsObjectiveCompleted, IsObjectiveFailed, ModObjectiveGlobal, UpdateCurrentInstanceGlobal, GetAlias, GetAliasById, GetAliasByName, GetID, GetPriority, GetQuest | |
| Alias, ReferenceAlias, LocationAlias | GetOwningQuest, GetID, GetName, GetReference, GetRef, GetActorReference, GetActorRef, ForceRefTo, ForceRefIfEmpty, Clear, GetLocation, ForceLocationTo, AddInventoryEventFilter, RemoveInventoryEventFilter, RemoveAllInventoryEventFilters | |
| LeveledItem, LeveledActor, LeveledSpell | AddForm, Revert, GetNumForms, GetNthForm, GetNthLevel, GetNthCount, GetChanceNone, SetChanceNone (the last six are SKSE's, the state is there anyway) | |
| Location | GetKeywordData, SetKeywordData, IsCleared, SetCleared, IsChild, HasCommonParent, IsSameLocation | IsLoaded, HasRefType, GetRefTypeAliveCount, GetRefTypeDeadCount |
| Faction | SetAlly, SetEnemy, GetReaction, SetReaction, GetCrimeGold, GetCrimeGoldViolent, GetCrimeGoldNonViolent, SetCrimeGold, SetCrimeGoldViolent, ModCrimeGold, CanPayCrimeGold, PlayerPayCrimeGold, GetInfamy, GetInfamyViolent, GetInfamyNonViolent, IsPlayerExpelled, SetPlayerExpelled, SetPlayerEnemy, IsFactionInCrimeGroup | SendAssaultAlarm, SendPlayerToJail, ModReaction |
| Game | GetPlayer, GetForm, GetFormEx, GetFormFromFile, EnablePlayerControls, DisablePlayerControls, GetGameSettingFloat/Int/String, SetGameSettingFloat/Int/Bool/String, GetRealHoursPassed, IncrementStat, QueryStat, IsFastTravelEnabled, EnableFastTravel, SetPlayerAIDriven, SetInChargen, SetBeastForm, TeachWord, UnlockWord, IsWordUnlocked, AddAchievement, GetPlayersLastRiddenHorse, AddPerkPoints, ShowRaceMenu | ShakeCamera, FadeOutGame, RequestSave, ForceFirstPerson, AdvanceSkill and 16 more, plus the DLC crime and sky functions: SendWereWolfTransformation, ServeTime, ClearPrison, SetSunGazeImageSpaceModifier, GetSunPositionX/Y/Z |
| Debug | Trace, TraceUser, TraceConditional, TraceStack, Notification, MessageBox, SendAnimationEvent | OpenUserLog, CloseUserLog, QuitGame, CenterOnCell, SetGodMode and 14 more |
| Utility | RandomInt, RandomFloat, GetCurrentGameTime, GetCurrentRealTime, GameTimeToString, SetINI\* and GetINI\* (eight) | IsInMenuMode, the frame rate and budget functions (eleven). `Wait`, `WaitMenuMode`, `WaitGameTime` are the VM's own |
| GlobalVariable | GetValue, GetValueInt, SetValue, SetValueInt, Mod | |
| Keyword | GetString | SendStoryEvent, SendStoryEventAndWait |
| Cell | IsInterior, GetActorOwner, GetFactionOwner, SetActorOwner, SetFactionOwner, SetPublic | IsAttached, Reset |
| FormList | GetSize, GetAt, HasForm, Find, AddForm | |
| Message | Show, ShowAsHelpMessage | ResetHelpMessage |
| Scene | Start, ForceStart, Stop, IsPlaying | IsActionComplete |
| Sound | Play, StopInstance, SetInstanceVolume | |
| Spell | Cast, RemoteCast | |
| Scene, Package | GetOwningQuest | |
| TopicInfo | GetOwningQuest | |
| ActiveMagicEffect | RegisterForUpdate, RegisterForSingleUpdate, RegisterForUpdateGameTime, RegisterForSingleUpdateGameTime, UnregisterForUpdate, UnregisterForUpdateGameTime, RegisterForAnimationEvent, UnregisterForAnimationEvent, GetBaseObject, GetTargetActor, GetCasterActor, Dispel, AddInventoryEventFilter, RemoveInventoryEventFilter, RemoveAllInventoryEventFilters | |
| EffectShader | Play, Stop | |
| VisualEffect | | Play, Stop |
| ImageSpaceModifier | Apply, ApplyCrossFade, Remove, RemoveCrossFade, PopTo | |
| ShaderParticleGeometry | | Apply, Remove |
| MusicType, Topic | | Add, Remove |
| Weather, Weapon, Scroll | | SetActive, Fire, Cast |

Condition functions 84 (`GetDeadCount`), 323 (`GetCombatState`) and 338
(`GetIgnoreFriendlyHits`) use the local CK contracts and xEdit function
identities. Dead count reads the number of runtime-dead actors for an NPC
base, increasing on death and decreasing on resurrection. Combat state reports
0 when the actor is not engaged, 1 when it has a current combat target, and 2
when it remains engaged without a target and is searching. Ignore-friendly-hits
reads the reference flag used by its Papyrus native. The condition functions 63,
66, 118, 288 and 361 remain unsupported with explicit function diagnostics
because the local CK corpus does not provide usable condition semantics for them.

Current behavior and compatibility limits:

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
- `DispelSpell` removes active applications from the named spell and reports
  whether anything was removed. Neither dispel native changes known or equipped
  spells. `DispelAllSpells` preserves diseases, abilities, addictions and constant
  enchantments. Equipping an ARMO with an EITM link applies the linked ENCH
  effect entries to the wearer as indefinite active effects. Unequipping the
  item or replacing it in an overlapping armor slot ends those effects and
  reverses their actor-value changes. Unsupported or malformed effects are
  logged by `Actor.EquipItem`. Weapon enchantments remain outside this path
  and are not applied during combat yet.
- `tick_menu_mode(dt, true)` pauses world time and ordinary waits while
  `WaitMenuMode` continues. `tick(dt)` remains the unpaused entry point.
  The UI must supply the pause state when menus are implemented.
- Inventories start from the base record's `CNTO` list.
- Actor values keep base, permanent, temporary and damage components in one
  store. Set changes base, Mod changes permanent, Force adjusts permanent
  to reach the requested current value, Damage subtracts from the damage
  component and Restore heals that component without changing maximum.
  GetActorValueMax/GetAVMax includes permanent and temporary modifiers.
  GetActorValuePercentage uses that maximum and follows the CK exceptions
  for zero maximum and CarryWeight.
- Actor initialization resolves separate stats and traits template chains.
  Manual attributes use race starting values plus NPC offsets. Manual skills
  preserve the DNAM cache while race-bonus semantics remain unresolved.
  Fixed-level auto-calculated skills
  use class weights, actor level and race boosts. `PC_LEVEL_MULT`, missing
  class and zero-weight cases keep the authored DNAM skill cache. SOEW saves
  initialized actor values, so reload does not seed bonuses again. Leveled
  templates return an error pending spawn selection. Missing race DATA
  retains the synthetic-record fallback. See [actor values](actor-values.md).
- `Game.AdvanceSkill` records player skill use from the winning AVIF `AVSK`
  multipliers and offsets. The improve multiplier and offset plus
  `fSkillUseCurve` determine experience needed at each current skill level.
  Earned levels update the shared actor-value base and stop at 100. Partial
  skill experience is stored per actor in the `PRG-01` section of SOEW version
  37 actor records. This path applies only explicitly recorded skill use.
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
- A leveled list is its `LVLO` entries in record order plus what `AddForm`
  appended. `Revert` truncates back to the record's entries and chance.
  Actor templates roll LVLN during initialization. The PRG-05 follow-up rolls
  ordinary LVLI quantities through `World::roll_leveled_items`.
- Location keyword data is run-time only and starts at zero, the record has
  no field for it. `IsChild` walks `PNAM` upward from the candidate,
  `HasCommonParent` intersects the two ancestor chains and applies the filter
  keyword to the shared ancestor, `IsSameLocation` is identity or a filtered
  shared parent.
- Faction reactions start from `XNAM` and are written in both directions by
  `SetAlly` and `SetEnemy` (friend and neutral through the two flags).
  `ModCrimeGold` adds to the violent or non-violent bounty and to the infamy
  of that kind, `PlayerPayCrimeGold` removes the bounty from the player's
  `Gold001` and clears it. Jail and the assault alarm are stubs.
- Perks, the crime faction and the outfits are seeded from `PRKR`, `CRIF`,
  `DOFT` and `SOFT`. `SetCrimeFaction` also joins the faction, per its page.
  `StartSneaking` toggles, per its page. `SetRestrained` and
  `SetUnconscious` clear each other. `Kill(akKiller)` records the killer for
  `GetKiller`, `StartCombat(target)` the target for `GetCombatTarget`.
- `GetCurrentLocation` is the reference's `XLCN`, else its cell's.
  `GetEditorLocation` reads the same from the record and ignores `MoveTo`.
  Ownership is one `XOWN` slot on the reference or cell, `GetActorOwner` and
  `GetFactionOwner` answer by the owner's record type.
- The listener registrations (`RegisterForSleep`, `RegisterForTrackedStatsEvent`,
  `RegisterForAnimationEvent`, `RegisterForLOS` and the single variants) keep
  lists of VM objects. `Game.IncrementStat` fires `OnTrackedStatsEvent`,
  `PlayAnimation` and `Debug.SendAnimationEvent` fire `OnAnimationEvent` to
  listeners on that sender and name, `World::sleep_start`, `sleep_stop` and
  `line_of_sight` are the host's hooks for the other two.
- `Message.Show` puts a `DNAM` message-box message in the box queue and any
  other in the notification queue, and returns button 0. A scene starts at
  phase zero. `World::complete_scene_action` marks an action slot complete,
  and `World::advance_scene_phase` waits for every action ending in the current
  phase before advancing. Reaching the last phase stops the scene and delivers
  `OnEnd`. `Scene.Start` and `Stop` deliver `OnBegin` and `OnEnd` to the
  scene's scripts. SOEW version 36 stores the current phase and completed
  action slots. Version 37 stores each reference's persistent flag. Action
  slots use zero-based positions from the SCEN action list because two vanilla
  scenes reuse action numbers. The SCEN layout follows
  `TES5Edit/Core/wbDefinitionsTES5.pas` and
  `uesp-modfileformat/SCEN.wikitext`. Timer expiry, CTDA evaluation, automatic
  action dispatch, and the scene flags are not implemented. `Sound.Play`
  hands out instance ids. `Spell.Cast` records the cast, no effect applies.
- `SetGameSetting*` writes an overlay `GetGameSetting*` reads first.
- A quest start delivers `OnInit` to its alias objects too, the ones a
  script is attached to, since the alias kind (`ALST` or `ALLS`) now decides
  the class (`ReferenceAlias` or `LocationAlias`) of the VM object.
  Transitional QUST start and stop flags restore pending promotion tasks.
  `IsStarting` and `IsStopping` report the task direction, and `IsRunning` stays
  false until the task completes. SOEW version 37 retains those tasks.

## Tests

Installed integration tests use `tests/common/installed.rs`. Each test
process caches parsed plugin files and load orders by their installed master
set, then gives each `Game` its own mutable world over the shared immutable
load order. `tst_14` checks in `combat_hits.rs` and `dialogue.rs` verify the
cache reuse and world isolation.

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

AI-66 adds `GetWithinDistance` (639), using the run-on subject's position,
the parameter reference's position and the parameter float as an inclusive
distance threshold. xEdit's `wbConditionFunctions` and UESP's
`Raw_Function_Data.wikitext` define its reference and float parameters.
Furniture entry state (614), path target state (619 to 622), movement speed
(623) and flying state (633) remain explicit unsupported-function diagnostics.
The shared world has no current entry choice, path target values or flight
state. The AI-31 Hover drafts are absent from this worktree, so those states
cannot be read from the runtime yet. The CK `GetFlyingState` page defines
states 0 to 4, but that contract alone does not provide the missing runtime
state.

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
table. Every other procedure name is `Procedure::Unsupported` and the actor
stands where it is. Location kinds near reference, in cell, near package start,
near editor location, near linked reference, alias reference, alias location
and near self resolve.

The local CK `EvaluatePackage - Actor` and `GetCurrentPackage - Actor` pages
define package evaluation and observation, not Flee movement. PACK branch
completion uses xEdit's `PRCB` flags and the UESP PACK layout. Route selection
uses the shared combat target and vanilla NAVM graph already used by combat
movement. No procedure-specific source in the local corpus defines Flee's
destination choice.
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
scheduled hour) in 7.9 s including the plugin parse. 5,013 packages
resolve to a procedure the driver runs: 2,467 `Travel`, 1,841 `Sandbox`,
327 `Sleep`, 300 `Sit`, 52 `UseMagic` and 26 `Shout`. The rest by template
procedure: Patrol 378, HoldPosition 141, Follow 128, UseIdleMarker 61, Orbit 47,
Escort 45, Activate 40, ForceGreet 31, Guard 24, Hover 14, Flee
13, Find 11, Wait 11, DialogueActivate 2, UseWeapon 2. The `Prisoner`
base carries no `PKID` list at all, so the innkeeper `Hulda` stands in:
her stack is `WhiterunInnkeeperPackage`, which resolves to `Sandbox` at
every hour tried, and no condition function on the way was unsupported.
The synthetic tests (`tests/packages.rs`) walk a guard across two
navmeshed cells on a travel package once a quest stage unlocks it, sandbox
to an idle marker, sleep on schedule and wake at 08:00, take an alias
package over the base list, switch at once from a script's
`EvaluatePackage`, and keep walking after a save and load.

**Not done.** Simultaneous and stacked branches (AI-14 and AI-15),
the other 16 procedures and the Patrol modes tracked by AI-88 through AI-91,
calling the `On Begin`, `On End` and `On Change` package fragments at
package boundaries (`vmad::Vmad` decodes them from `VMAD` on `PACK`, the
driver does not yet call them, AI-04), idles and topics, once per day for
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
the survival warmth rating, werewolf crime, jail time and the sun.

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
with quiet stubs for visual effects, image space, music and translation. It
also fixed a real
gap next door: the `ACHR` reader ignored `XLKR`, so an actor reference had
no linked references at all. Actor references carry them the same way a
`REFR` does, and the DLC scripts use them to find a coffin or a cart.

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
sites were asking for. `TopicInfo.GetOwningQuest` answers None, because an
info takes its quest from the `DIAL` above it and the record view cannot
reach a parent group yet. The `ActiveMagicEffect` registration set went in
next: an active effect is not a `Form`, so the registrations the registry
holds on `Form` were out of its reach, and since they key off the calling
object rather than a form the same behaviour registers cleanly on the effect
class. The later active-effect implementation below supplies its getters and
`Dispel` using a separate object for each effect application.

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
Versions 32 to 36 add assigned hand spells, summoned references, encounter-zone
visit times, saved list flags and chances, active scene phase state, and BOS
record flags on instantiated references. Version
37 persists INFO said-once markers, SMQN last-run float bit patterns, and SCEN
active markers and opaque active data in record overrides. The
`World::restore_vanilla_dialogue_scene_changes` path validates all targets
before applying them. INFO state updates dialogue history, SCEN state updates
the playing-scene set, and the record accessors expose the saved values. INFO
has no saved timestamp, so its said-once marker uses the current game day.
Version 37 also retains PACK and NAVM change markers in record overrides.

SAV-59 stores opaque unresolved record payloads and plugin-relative links in a
world section. These records are not installed into the live form table.
Papyrus SOEV version 4 carries script source fingerprints so missing or
changed definitions keep saved VM state suspended.

SOEW versions 21 and later retain each driven actor's package procedure
cursor, including the owning procedure tree, branch index and active or
finished state. Loading an engine save lets the driver continue that branch.
Vanilla ESS package state is not connected to this cursor: named Sequence and
Stacked procedure payloads are not decoded, so a saved actor cannot yet resume
its vanilla procedure.
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
same fields. The SAV-59 world section preserves these values without resolving them
through the current load order, so removed plugins and compacted slots cannot
alias these retained records. This storage API does not populate itself from
ESS change forms. Whole-game `Game::load_state` still requires matching plugin
provenance, and ordinary world state does not yet remap identities for removed
plugins.

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
reads this evidence. `load_state` checks it before replacing state, rejecting
missing or changed plugins, slot movement and changed script fingerprints.
This prevents a reordered plugin from silently retargeting saved form IDs.
Programmatically loaded scripts have unknown source fingerprints. Each world
and VM blob remains length-prefixed. The caller loads the same definitions
before restoring into a game that has not been bound or started.

`Game::preflight_dependencies` compares native save provenance with the
current load order and scripts. Its deterministic findings identify missing
or added plugins and scripts, plugin kind or slot changes, changed
fingerprints, and fingerprints that cannot be compared because source bytes
are unknown. Each finding identifies whether it affects world or VM state.
The method reads the save envelope and does not mutate the game.

`Game::preflight_vanilla_dependencies` compares ESS plugin names and Papyrus
script names from definitions, instances, references, active frames, queued
messages and suspended calls with the current definitions. It reports missing
plugins and scripts, regular/light kind changes and unknown provenance for
each saved plugin or script because ESS stores no content fingerprints. A
malformed Papyrus block returns an error. Findings have a stable ordering.

`Game::write_file` serializes and validates native state before replacing an
existing file. `Game::write_state_file` accepts previously serialized bytes and
checks them against the current load order and scripts in scratch state. Both
use the [atomic file protocol](save-resilience.md#sav-62-file-replacement),
including read-back validation, file and directory synchronization, a prior
link during replacement and explicit error paths. Validation executes no
callbacks and does not mutate the running game.

`GenerationStore` adds [verified native generations](save-resilience.md#sav-63-verified-generations).
`Game::write_generation` validates a new file before bounded rotation.
`select_generation` reports damaged or incompatible candidates and returns the
newest one loadable under the current definitions. `recover_generation` loads
those verified bytes. Retention preserves loadable fallbacks before intact
incompatible files, with a minimum configured limit of two generations.
The `sav_63_` tests also corrupt a newly published generation after an
interrupted rotation, then recover the older verified state and finish a
bounded retry.

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
  is action-gated, but action execution, phase and scene CTDA evaluation,
  timer lifetimes, and repeat-while-conditions behavior are not implemented.
  Perks are a set on the
  actor, no entry point applies. Leveled lists are never rolled. Spell casts
  are recorded, no effect applies. Crime has bounty and infamy ledgers, no
  jail or arrest.
- Faction relations from `XNAM`, location keyword data, perks and the actor
  flags were tested on the synthetic master and reached by the vanilla
  scripts at start, not checked against the game's runtime values.
- Quest aliases are filled only from `ALFR`. Conditions, `ALCO`, location and
  reference fill types are not evaluated.
- `SetStage` does not display the log entry or play the quest sound.
- Missing localized entries come back as `<string N>`. Selected-language
  string tables now load through the VFS, as described below.
- `bind` attaches every script in the load order at start. The game does this
  lazily per cell. On the five base masters this is tens of thousands of
  instances, timed at 0.2 seconds against the installed game (2026-09-17),
  not a performance concern.
- No native has been checked against its CK wiki page yet, see above.

## Cell transitions (2026-09-18, branch `transitions`)

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
The script-facing AddItem, RemoveItem and RemoveAllItems stack paths now
queue these events too. Silent removal still notifies scripts. AddItem expands
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

The condition evaluator implements these CTDA function IDs:
1, 14, 18, 24, 35, 45, 46, 47, 53, 56, 58, 59, 61, 67,
69, 70, 71, 72, 73, 74, 77, 79, 80, 91, 98, 99, 106, 107,
110, 111, 116, 123, 125, 127, 128, 136, 142, 182, 185, 214, 219, 223,
225, 230, 248, 249, 250, 264, 277, 280, 286, 287, 289, 310, 313, 315,
318, 325, 327, 339, 359, 360, 372, 378, 403, 408, 414, 426, 430, 432,
434, 435, 444, 445, 446, 447, 448, 470, 491, 494, 500, 501, 519, 528,
543, 550, 560, 561, 562, 563, 565, 566, 567, 568, 569, 570, 591, 592,
598, 600, 601, 603, 604, 605, 606, 610, 625, 626, 629, 630, 635, 636,
639, 640, 651, 655, 659, 660, 675, 683, 691, 692, 693, 696, 698, 699,
704, 706, 711, 713, 718, 721, 723, 724, 730, 731, 734, 735.

This is a bounded set, not the full CTDA contract. Evaluation supports ordered
AND/OR groups, numeric or global comparisons, explicit references and
caller-supplied quest aliases. Reserved PlayerRef resolves to the runtime
player even without a placed record.

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
state. `IsAllowedToFly` reads the actor's `SetAllowFlying` flag. `IsInFurnitureState`,
`GetPathingCurrentSpeedAngle`, `IsBeingRidden` and `IsOnFlyingMount` return an
unavailable-state diagnostic. xEdit supplies their function and parameter
definitions, but the engine has no furniture animation, integer graph variable,
rider relation or flying mount state. The CK page for `IsBeingRidden` defines
its result, but no shared rider relation currently backs it. CK semantic pages
for the other four functions were absent from the local corpus, so their
numeric results are not inferred from the function names. These conditions
are not included in `supports_function`.
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
CommonLib form-type code. First-person camera state, shared combat-group counts
and the grabbed reference are unavailable runtime contexts, so functions 391,
415, 416 and 463 return explicit condition errors. They do not silently match
either boolean result. Local xEdit `wbDefinitionsTES5.pas` and UESP
`Raw_Function_Data.wikitext` supply the function indices and parameter types.
Mutagen's Skyrim condition data types confirm the typed FLST, placed-reference,
VTYP and form-type links. The CK corpus does not document the four unavailable
runtime states, and CMB-17 currently stores actor combat targets without a
shared combat-group identity.

Sources consulted were local xEdit `wbDefinitionsTES5.pas` for CTDA, COBJ and
FURN WBDT, CK `GetItemCount_-_ObjectReference.wikitext` for the query forms,
UESP COBJ notes for missing outputs, skymp `CraftService.cpp` and
`unit/CraftTest.cpp` for bench matching and DLC cases, Mutagen
`Skyrim/Constants.cs` for PlayerRef, and CK inventory event/filter pages.
For AI-67, xEdit `wbConditionFunctions` supplies the function indexes and
parameter types. The local CK corpus supplies
`IsBeingRidden_-_Actor.wikitext`. The remaining requested CK pages are absent.
GetGraphVariableInt (675) reads a named integer in the actor's live graph state,
case-insensitively, and reports missing names and non-actor subjects. Its
integer state is transient host input and is not saved.
The skymp evaluator's unknown-function success and unresolved target mapping
were not adopted. Mutagen labels HasPerk's second integer unused, while xEdit
exposes it, so nonzero values need further evidence. Caprica's property
compiler confirms conditional auto-property flags on the backing variable.

AI-65 implements CTDA 447, `GetGraphVariableFloat`, from the actor's saved
behavior-graph float map. The condition's `CIS1` name is matched without case
sensitivity. Missing names and non-actor subjects return explicit errors. CTDA
568 reads the transient sprint state slot for a live movement driver. This
worktree does not yet connect host movement input to that slot. The
other AI-65 functions remain unsupported because their transition or animation
state is not available in this worktree. Local xEdit identifies their
parameters, while the CK corpus only documents `IsSprinting` and `IsFlying`
as live state queries. The AI-31 draft prerequisites are absent here, so no
cell-crossing, last-idle, flight, movement, relative-angle or furniture
animation state is inferred from unrelated saved flags.

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

Enchanting, skill advancement, routing crafting inputs through Story Manager
quest selection, item instance state and host menus remain unfinished. The full
requested execution scope is tracked in [execution-plan.md](execution-plan.md).

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

`Game::send_story_event` is an explicit runtime API. Gameplay actions do not
generally emit story events automatically. The AI-69 draft was not present in
this worktree. Its condition functions `GetScriptVariable`, `GetQuestVariable`,
`GetDestructionStage`, `IsInCriticalStage`, `GetEventData`,
`GetIsCrashLandRequest` and `GetIsHastyLandRequest` remain a dependency of
conditioned trees. Unsupported or unavailable predicates return a typed
routing error. Unknown event codes queue `OnStoryEvent` because this branch
has no authored Papyrus event name for them.

Successful `Game::craft` transactions enqueue one `CraftItem` Story Manager
input with bench, current location and created item arguments. Rejected
transactions enqueue none. `World::take_story_manager_events` transfers these
inputs to the scheduler. This submission path does not automatically route
every gameplay action, and event routing remains dependent on the explicit
Story Manager API and supported predicates above.

Tempering, alchemy, enchanting, skill advancement, routing crafting inputs
through Story Manager quest selection, item instance state and host menus
remain unfinished. The full requested execution scope is tracked in
[execution-plan.md](execution-plan.md).

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

Remaining dialogue work includes missing conditions, ForceGreet approach
packages, walk-away topic selection, scene action execution and timers,
barter transactions, complete alias behavior and wider interruption rules. Godot activation,
menus, voice assets, LIP and idle playback are not connected to this API. This increment does not establish playable dialogue
or complete the broader gameplay goal.

DLG-02 starts a greeting when a driven, living actor enters the current
`fAIInDialogueModeWithPlayerDistance` range of the player in the same cell or
exterior worldspace. The DIAL must use the Player category and ForceGreet
subtype, encoded as legacy DATA subtype 1 or SNAM `PFGT`. The normal dialogue
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
LastBribedIntimidated actor value that also backs SetBribed, and the success
check compares the player's Speechcraft against the subject's Confidence
(AIDT, 0 Cowardly to 4 Foolhardy) scaled onto the skill range. This is an
approximation of the CK wiki's "Mod Player Intimidation" perk entry point
formula, which the engine does not run yet. The Papyrus native
`WillIntimidateSucceed` is still a stub in `natives.rs`. No engine state sets
LastBribedIntimidated yet either, so `IsIntimidatedbyPlayer` reads false until
a bribe or intimidate native writes it.

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

### Calm archetype, MAG-29

MGEF archetype 6 applies a temporary reduction to the target's `aggression`
actor value, regardless of the MGEF's serialized primary actor value or
detrimental flag. Recoverable effect state restores the prior value when the
effect expires or is dispelled. The combat driver drops an existing combat
target when the actor's live aggression falls below 1, matching its target
acquisition threshold. This supports Calm delivery through the existing spell
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
No EffectShader page exists in the local CK wiki corpus. The host currently
draws one temporary emissive sphere at the target reference placement. It
does not parse EFSH textures, colors, blend modes or fade curves.

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

`World::grab_telekinesis_reference` now accepts one enabled, non-deleted,
non-actor reference while that caster holds a concentration spell containing
archetype 20. A second grab is rejected. Releasing the concentration cast,
magicka exhaustion or a cast error clears the transient reference. The World
does not serialize the grab because the host body is transient. The host must
also confirm that the reference has a resident movable body. The MAG-20
equip-ability grant is absent in this worktree, so the shipped spell's
Telekinesis actor-value bonus is not applied.

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

The typed HAZD view from FMT-28 is available, but the game world has no owned
hazard state or expiry path. Archetype 40 therefore returns a specific
unsupported error before any effect or world state changes.
`mag_61_valid_hazard_cast_rejects_without_owned_lifecycle` checks this
boundary. The row's acceptance check remains incomplete: no hazard is spawned
at a target, and no owned collision or effect is removed on expiry. Target
selection, contact application and save continuation also remain unimplemented.
The survey establishes record dependencies and the direct-cast rejection
boundary.

### Direct active-effect foundation

The runtime provides `World::cast_spell` for direct scripted casts
of Script, Value Modifier, Absorb, Dual Value Modifier and Dispel effects, including hostile hits. Definitions
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
collision result is supplied by `godot-host` and is not saved.

Each SPEL effect group's CTDAs run before that group is prepared. MGEF CTDAs
run separately, with FormIDs resolved against their owning MGEF. Both use the
delivery recipient as subject and the credited caster as target. A false group
skips only its effect. The currently implemented CTDA function set is shared
with dialogue and crafting. Variable-backed predicates return a typed error
until direct casting can borrow the VM for that lookup.
The [Creation Kit Magic Effect page](https://ck.uesp.net/wiki/Magic_Effect)
calls the flag "Swap Subject and Target" useful for running a condition on
the spellcaster. The common SPEL/MGEF context is inferred from that statement
and the shared Effect Item layout, with the recipient as default subject and
the caster as default target.

An active spell application checks its SPEL and MGEF conditions again at each
whole second of its elapsed lifetime. A false condition ends that application.
The elapsed lifetime is already part of SOEW magic state, so a save between
checks retains the time remaining until the next check. Concentration builds
fresh effect applications for each 0.1-second cast slice and evaluates their
conditions during that reapplication. Ingestibles do not use the spell
reevaluation cadence.

Each application gets an `EffectId` and a distinct `ActiveMagicEffect` VM object.
MGEF scripts are attached when an effect is applied, rather than attached to the
MGEF form at startup. VMAD properties use the winning MGEF's plugin context.
The queue binds the scripts before sending `OnEffectStart`, and expiry or dispel
sends `OnEffectFinish`. The getters return the base MGEF, target actor and caster
while the application is live. Finish handlers may observe absent native state,
as allowed by the shipped ActiveMagicEffect.psc contract.

An active effect targeting an inventory holder receives `OnItemAdded` and
`OnItemRemoved` only when its own filters admit the changed base item. The
filters follow the same direct form-list matching as reference and alias
recipients. Expiry and dispel remove the effect's filters before later changes
are queued. `active_effect_inventory_filters_forward_matching_events_until_expiry`
in `tests/magic.rs` covers allowed additions and removals, rejected items and
the absence of later events after expiry.

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

MGEF's `Resist Value` field and its enum come from local
`TES5Edit/Core/wbDefinitionsTES5.pas`. Its actor-value indices map through
`crates/game/src/actor_values.rs`. The local skymp magic action handler leaves
magic-effect application as a TODO, so it supplies no resistance formula. The
85 percent cap and order are the runtime's selected behavior. Keeping magic
and specific resistance as separate terms preserves their independent actor
values.

The source contracts are CK [Magic Effect](https://ck.uesp.net/wiki/Magic_Effect),
local `ck-wiki/AddInventoryEventFilter_-_ObjectReference.wikitext`,
`DispelSpell_-_Actor.wikitext` and
`DispelAllSpells_-_Actor.wikitext`, the shipped
`skse64/scripts/vanilla/ActiveMagicEffect.psc`, xEdit's SPEL/MGEF definitions, and
CommonLibSSE-NG `ActorValues.h` and `MagicSystem.h`. The skymp
`ActiveMagicEffectsMap` was read for comparison. Its one-effect-per-actor-value
map and wall-clock expiry do not fit independent applications or menu pause, so
those policies were not reused. `thiserror` is now a direct game dependency for
typed cast errors. It was already present transitively, with no new package.

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

Unsupported casts return a typed error before applying any of their effects, and Papyrus casts log that error. Unsupported cases include concentration, taper, perk adjustments, equip abilities and other unsupported archetypes. Aimed casts require projectile contact and location casts use the selected point for area targeting. Area selection is covered separately under Area targeting, MAG-06. Absorb and Cure Disease are supported below.

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
Automatic ability application from AddSpell or NPC spell lists, effect artwork and sounds, actor-event forwarding, effect inventory filters and VM object reclamation remain unfinished.
This is an active-effect foundation, not complete magic execution.

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
its local `OnMagicEffectApply_-_ObjectReference.wikitext` and
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

`mag_39_summon_check_spawns_authored_actor_with_caster_ownership_and_dismisses_on_expiry`
exercises the row's spawn, ownership and expiry check with synthetic records.
`mag_39_rejects_an_actor_base_without_the_summonable_flag` checks the sourced
ACBS requirement.
`mag_39_saved_summon_keeps_its_actor_link_until_expiry` checks the extended
save state. The check names no vanilla records, so it has no
`SKYRIM_DATA`-gated case. Summon combat behavior, command and banish effects,
visuals, sounds, and host presentation remain outside this check.

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
projectiles, contact and location delivery and taper stay separately
unsupported as before.

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
distribution is keyed by the NPC base and comes from the current load-time
mod-data pass. Per-actor SPID lifecycle and deterministic seeding remain
unsupported.

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
increases it, with a two-degree minimum. CK lock pages and xEdit's XLOC layout
define lock-level ranges, not the menu's angular rule, so the angle behavior
is runtime policy. The `ui_08` tests cover opening after a successful turn,
pick consumption after a failed turn and the key-only lock restriction. No
saved state was added.

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
separate requirements. PCON and PARW saved placements use the same reference
table and record-backed base queries. Their runtime state uses the existing
SOEW reference encoding. See [save.md](save.md#reference-placements) for
sources, validation and the limits of this intermediate reconstruction pass.

For reference headers with the generic form-flags bit, the pass applies the
`Mask Invert Flags` word to authored `INITIALLY_DISABLED` and `PERSISTENT`
record bits with XOR. It accepts only ESS header version 12, form version 78
and change-form version 78 when that mask is present. The second 16-bit word
is retained in the report and does not affect these two runtime properties.
The effective persistence value feeds cell-unload retention and
encounter-zone reset decisions. SOEW version 37 stores it with each reference.
Older SOEW states derive persistence from the current winning record.

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
request to the host. See [ranged release](combat.md#ranged-release-cmb-08).
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

## Saved ownership and locks, SAV-16, blocked

The partial reference-extra pass writes the shared owner, locked bit and
lock level. GetActorOwner, GetFactionOwner, IsLocked and GetLockLevel observe
those values immediately. GetKey and default door/container activation prefer
the saved key, including explicit null, over the authored XLOC link. The
activator's key is retained after unlocking. Restoration queues no events. A
synthetic restoration check applies placements first, then extras, before
checking native queries and keyed activation.

Rank and global ownership metadata and unassigned lock fields persist in the
SOEW version 20 extension and contribute to the world hash. Older saves
retain authored-key behavior. The full corpus remains blocked on 72 Sequence
and nine Stacked PackageData lists whose unknown boundaries may hide later
ownership or lock extras. The importer rejects them without changing the world.
See [save.md](save.md#reference-ownership-and-locks-sav-16-blocked) for sources,
independent enumeration and the superseded filtered-survey evidence. No new
host nodes or dependencies were added. SAV-07 supplies its form namespace and is
complete. SAV-12 and SAV-21 must finish before SAV-16 can pass its full-corpus
checks. This is a blocked implementation checkpoint.

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

`condition_actor_values` checks both evaluators against independent expected
values, high indices, invalid indices and subjects, resource fractions and the
existing kill/resurrection lifecycle. No new authoritative actor state,
dependency or save format was added.

## Reference state conditions AI-76

The gameplay CTDA evaluator implements IsProtected (476) from the subject's
NPC base state and PlayerKnows (493) for spells held in the player's spell
set. IsProtected rejects parameters and non-actor subjects. PlayerKnows
rejects malformed forms and extra parameters, compares known spells with the
player's current spell set, and returns false for an MGEF with no represented
known state. It returns a state contract diagnostic for other form types.

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
and explicit diagnostics for the uncontracted functions.

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
Their defaults are not established, so those branches report Unsupported.
Public parameter names do not determine execution.

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
describes the parameter types. These sources provide no reusable runtime
walker, so this implementation extends the existing actor driver.

The row was split before implementation. AI-88 covers marker idle, furniture
and topic actions and depends on AI-23 and HST-09. AI-89 covers mounted Patrol.
AI-90 establishes StaticPathing and omitted-input defaults. AI-91 resolves a
PTDA alias through the package's owning quest and selects the alias's reference
as the route start. That resolved global form identity is saved with Patrol
progress, so an alias into an overriding plugin retains its namespace after
reload. Object ID targets resolve in the package's defining plugin. A target
that names a FormList searches its entries in authored order. Object Type 8,
Miscellaneous, searches winning MISC bases in load-order order. Both paths save
the selected reference identity with Patrol progress. Other Object Type values
and Find-derived starts remain unsupported. AI-33's Find procedure, which owns
that selected-reference result, is not present in this worktree.

xEdit `wbDefinitionsTES5.pas` defines Object Type 8 as Miscellaneous and PTDA
Object ID as a form link. The UESP PACK description confirms the target and
count fields. The CK package target pages were not present in the local corpus.

`tests/package_patrol.rs` exercises linked order, final dwell, open and circular
repeat, nearest entry, radius budgets, menu timing, save/resume during travel,
dwell and reversal, winning-template namespaces, live marker changes, action
field contexts, invalid state, and
`ai_91_alias_route_start_keeps_winning_namespace_and_saved_target`. The
adversarial AI-91 tests check Object ID, FormList, and Object Type 8 selection
and save/load of each selected start.
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

`tests/image_space.rs` exercises all five natives, interruption, menu pause,
animated expiry, invalid-call atomicity, save continuation and corrupt-save
rejection. Its installed-record test is gated on `SKYRIM_DATA`. The host
snapshot test checks order, fade weights and clearing on world replacement.
MAG-71 separately owns authored color, blur, depth and HDR curve evaluation
and rendering. MAG-25 adds no visible post-processing and claims no rendered
parity with Skyrim.

## Image-space rendering prerequisite audit, MAG-71

MAG-71 is blocked on MAG-25 and MAG-72. Its acceptance check still requires
rendered animated color, blur, depth and HDR fixtures at the active stack
weights. MAG-72 owns the missing shared base image-space inputs and renderer.
No curve evaluator or partial rendering path was added by this audit.

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
sunrise, day, sunset and night. The game and host currently have no consumer
for these selected base values. MAG-72 must resolve winning records through
their owning load-order namespace and expose the resulting shared base state.
Its weather input is an explicitly selected weather and time blend.
Autonomous climate selection, weather scheduling and Papyrus weather natives
are outside that row. SCR-54 needs the owning weather API before binding
`Weather.SetActive`, which is still a quiet stub.

Several rendering contracts need further evidence within MAG-71. The local
UESP `IMAD.wikitext` describes normalized times but leaves interpolation
unspecified. The corpus contains 39 key timestamps outside zero to one.
Five pairs labelled unused by xEdit contain non-neutral values in
`ISMExterior`. CommonLib's `TESImageSpaceModifier.h` supplies interpolator
storage, while `ImageSpaceModifierInstanceForm.h` calls binary entry points
instead of defining interpolation or weighted composition. Target flags are
set in 84 radial-blur records and 4 DOF records. The instance type stores a
scene target, but the five Papyrus natives expose no target argument.
Fallback target behavior, curve interpolation, unused channels and exact
strength composition are not recovered contracts. They must not be inferred
from similarly named Godot properties or a linear crossfade weight test.

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
Learned enchantments, poison use and host menus belong to their separate
backlog items. Tempering uses this exact-selection API and does not change
Papyrus RemoveItem selection policy. These tests do not compare selection
policy against the original runtime or load installed save files.

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

Eleven synthetic tests cover actor-level filtering, nested counts, duplicate
weighting, Use All Spells, saved selections and empty outcomes, winning links,
template inheritance, traversal bounds, malformed inputs and RNG rollback
after earlier successful draws, plus older saved placeholders. The five-master
survey executes all 33 winning
LVSP lists at levels 1, 10 and 50, with 171 concrete spell results over 99
selections. The survey checks record execution, not comparison against a
running original game. Live actor leveling and later spell reevaluation retain
their progression scope.

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
purchase path does not reinterpret it. The full CK Perk page was absent from
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
actor's owned winning PERK records in load-order order, then each record's
complex entry sections in file order. It passes the PRKE rank as a one-based
value, so a stored zero is rank one, and includes priority and EPFT/EPFD data
for the handler. The dispatcher does not sort by priority. The local sources
do not establish equal-priority execution order or exact versus cumulative
effect-rank selection.

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
This path retains the dispatcher's winning-record and source-section order.
It does not claim the unresolved priority-tie, acquisition-order or exact
versus cumulative rank behavior documented above.

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
that is enabled, not deleted, has a base and a cell, and whose cell is the
loaded cell or a streamed one (`cell_is_resident`). Each entry carries the
reference's position and rotation in game units and radians, and a scale
equal to `actor_controller_scale` times the reference scale. An actor whose
controller scale cannot be derived keeps 1.0 for that factor, so a template
error never drops a body. Dead actors stay listed because a corpse still
blocks a blade. The list is sorted by form so a host sync is deterministic.
Because it reads the placement tables directly, the list follows cell
changes, streaming, enable state and scripted moves with no extra hook.

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
raise `SAVE_VERSION`.
`ai::package` carries the saved waiting and never-run markers on its parsed
package view.
The `sav_48_pack_navmesh_restore_is_atomic_and_survives_engine_save_load`
fixture exercises both record types, query results, failure atomicity and
engine save/load. `sav_48_installed_pack_and_navmesh_records_accept_restored_markers`
checks installed Skyrim.esm records when `SKYRIM_DATA` is set. The field order
and marker meanings follow xEdit `Core/wbDefinitionsTES5Saves.pas`.
