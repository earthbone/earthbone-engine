# soe-game: the runtime form table and the first natives

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
  `actors` (the `NPC_` view, actor state and values), `quests` (stages,
  objectives, aliases), `forms` (globals, form lists, leveled lists,
  locations, factions, actor bases, cells, game settings), `events` (what
  natives raise for listeners, scenes, sounds), `timers`, `rng`, `driver`
  (what the package driver reads), `save` (the `SOEW` format) and `hash`
  (the determinism hash).
- `vmad`: the `VMAD` subrecord. Scripts, their filled properties, quest stage
  fragments and alias scripts. Round-trips through `write`.
- `npc`: the `NPC_` view. `ACBS` configuration, `DNAM` stats and skills,
  spells, factions, items, race, class and template, with the template flags
  applied one level. `World::actor` builds actor state from it.
- `quest`: the `QUST` view. Flags, stages with log entries, objectives, aliases
  with forced references.
- `natives`: the base game's natives, closed over the shared world.
- `clock`: real seconds drive the scheduler and update timers, game time runs
  at `timescale` (20) for `GetCurrentGameTime`.
- `harness`: the conformance scenario runner, see `docs/validation.md`.
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

`vanilla_save::SavedFormMap` remaps ESS references through saved plugin names,
including regular and light slots. `World::restore_vanilla_globals` atomically
applies the global table to winning GLOB forms. Values reach the existing
Papyrus native path and engine save state. This is one restoration component,
not a full ESS load or resumed game. See [save.md](save.md) for mapping rules,
failure behavior and the remaining world and VM work.

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

## Conformance table

Counts from `natives::register`: 361 natives with state behind them and 180
quiet stubs (141 and 176 before the natives round of 2026-09-18, 294 and 137
before the DLC round). A stub returns the value the game gives when nothing
is going on (`IsBleedingOut` false, `ShakeCamera` nothing) so vanilla scripts
run past it.

| Class | Implemented | Stubbed |
|---|---|---|
| Form | GetFormID, GetName, GetType, HasKeyword, GetKeywords, RegisterForSingleUpdate, RegisterForUpdate, RegisterForSingleUpdateGameTime, RegisterForUpdateGameTime, UnregisterForUpdate, UnregisterForUpdateGameTime, RegisterForSleep, UnregisterForSleep, RegisterForTrackedStatsEvent, UnregisterForTrackedStatsEvent, RegisterForAnimationEvent, UnregisterForAnimationEvent, RegisterForLOS, RegisterForSingleLOSGain, RegisterForSingleLOSLost, UnregisterForLOS | |
| ObjectReference | Enable, EnableNoWait, Disable, DisableNoWait, IsEnabled, IsDisabled, Delete, IsDeleted, GetBaseObject, GetParentCell, GetWorldSpace, IsInInterior, GetPositionX/Y/Z, GetAngleX/Y/Z, SetPosition, SetAngle, MoveTo, GetDistance, Activate, BlockActivation, IsActivationBlocked, GetLinkedRef, GetItemCount, AddItem, RemoveItem, RemoveAllItems, GetScale, SetScale, GetDisplayName, SetDisplayName, PlayAnimation, GetOpenState, SetOpen, Lock, IsLocked, GetLockLevel, SetLockLevel, PlaceAtMe, AddToMap, EnableFastTravel, SetDestroyed, IgnoreFriendlyHits, IsIgnoringFriendlyHits, GetCurrentLocation, GetEditorLocation, IsInLocation, MoveToMyEditorLocation, GetActorOwner, GetFactionOwner, SetActorOwner, SetFactionOwner, AddInventoryEventFilter, RemoveInventoryEventFilter, RemoveAllInventoryEventFilters, SetAnimationVariableFloat/Int/Bool, GetAnimationVariableFloat/Int/Bool, GetNthLinkedRef, PlaceActorAtMe | Is3DLoaded, IsFurnitureInUse, GetCurrentScene, SetMotionType, ApplyHavokImpulse, PushActorAway and 6 more, plus the DLC ragdoll, cart and translation: ForceAddRagdollToWorld, ForceRemoveRagdollFromWorld, TetherToHorse, IsFurnitureMarkerInUse, Reset, TranslateTo, SplineTranslateTo, StopTranslation, CreateDetectionEvent |
| Actor | GetActorValue, GetBaseActorValue, SetActorValue, ModActorValue, DamageActorValue, RestoreActorValue, GetActorValuePercentage (and the AV short names), IsDead, Kill, GetKiller, Resurrect, IsInCombat, StartCombat, StopCombat, GetCombatTarget, GetActorBase, GetLeveledActorBase, IsEssential, IsPlayerTeammate, SetPlayerTeammate, IsGhost, SetGhost, IsProtected, GetLevel, GetRace, IsUnconscious, SetUnconscious, SetRestrained, IsDoingFavor, SetDoingFavor, AddSpell, RemoveSpell, HasSpell, EquipItem, UnequipItem, IsEquipped, IsInFaction, GetFactionRank, AddToFaction, RemoveFromFaction, SetFactionRank, AddPerk, RemovePerk, HasPerk, SetAlert, IsAlerted, StartSneaking, IsSneaking, DrawWeapon, SheatheWeapon, IsWeaponDrawn, IsArrested, ClearArrested, SetAttackActorOnSight, SetDontMove, SetAllowPCDialogue, SetRelationshipRank, GetRelationshipRank, GetHighestRelationshipRank, GetLowestRelationshipRank, GetCrimeFaction, SetCrimeFaction, SetOutfit, and from the DLC: SendVampirismStateChanged, SendLycanthropyStateChanged, SetAllowFlying, SetAllowFlyingEx, IsAllowedToFly, CanFlyHere, StartDeferredKill, EndDeferredKill, KillSilent, SetRace, SetEyeTexture, SetCriticalStage, AttachAshPile, SetNoBleedoutRecovery, GetNoBleedoutRecovery, EnableAI, AddShout, RemoveShout, EquipShout, UnequipShout, GetEquippedShout, EquipSpell, UnequipSpell, SetSubGraphFloatVariable, PlaySubGraphAnimation, WornHasKeyword, IsPlayersLastRiddenHorse, SetAlpha, PlayIdle, HasMagicEffect, HasMagicEffectWithKeyword, DispelSpell, DispelAllSpells | IsAlarmed, IsBleedingOut, GetCombatState, EvaluatePackage, GetEquippedWeapon, SetLookAt and 34 more, plus the DLC ones without a system: IsFlying, GetWarmthRating, ClearExtraArrows, DoCombatSpellApply, MoveToPackageLocation |
| ActorBase | GetRace, GetClass, GetSex, GetDeadCount, IsEssential, IsProtected, IsInvulnerable, IsUnique, SetEssential, SetProtected, SetInvulnerable, SetOutfit | GetGiftFilter |
| Quest | Start, Stop, Reset, IsRunning, IsCompleted, IsActive, SetActive, CompleteQuest, CompleteAllObjectives, FailAllObjectives, SetStage, SetCurrentStageID, GetStage, GetCurrentStageID, GetStageDone, IsStageDone, SetObjectiveDisplayed, SetObjectiveDisplayedNoWait, SetObjectiveCompleted, SetObjectiveFailed, IsObjectiveDisplayed, IsObjectiveCompleted, IsObjectiveFailed, ModObjectiveGlobal, UpdateCurrentInstanceGlobal, GetAlias, GetAliasById, GetAliasByName, GetID, GetPriority, GetQuest | IsStarting, IsStopping |
| Alias, ReferenceAlias, LocationAlias | GetOwningQuest, GetID, GetName, GetReference, GetRef, GetActorReference, GetActorRef, ForceRefTo, ForceRefIfEmpty, Clear, GetLocation, ForceLocationTo, AddInventoryEventFilter, RemoveInventoryEventFilter, RemoveAllInventoryEventFilters | |
| LeveledItem, LeveledActor, LeveledSpell | AddForm, Revert, GetNumForms, GetNthForm, GetNthLevel, GetNthCount, GetChanceNone, SetChanceNone (the last six are SKSE's, the state is there anyway) | |
| Location | GetKeywordData, SetKeywordData, IsCleared, SetCleared, IsChild, HasCommonParent, IsSameLocation | IsLoaded, HasRefType, GetRefTypeAliveCount, GetRefTypeDeadCount |
| Faction | SetAlly, SetEnemy, GetReaction, SetReaction, GetCrimeGold, GetCrimeGoldViolent, GetCrimeGoldNonViolent, SetCrimeGold, SetCrimeGoldViolent, ModCrimeGold, CanPayCrimeGold, PlayerPayCrimeGold, GetInfamy, GetInfamyViolent, GetInfamyNonViolent, IsPlayerExpelled, SetPlayerExpelled, SetPlayerEnemy, IsFactionInCrimeGroup | SendAssaultAlarm, SendPlayerToJail, ModReaction |
| Game | GetPlayer, GetForm, GetFormEx, GetFormFromFile, EnablePlayerControls, DisablePlayerControls, GetGameSettingFloat/Int/String, SetGameSettingFloat/Int/Bool/String, GetRealHoursPassed, IncrementStat, QueryStat, IsFastTravelEnabled, EnableFastTravel, SetPlayerAIDriven, SetInChargen, SetBeastForm, TeachWord, UnlockWord, IsWordUnlocked, AddAchievement, GetPlayersLastRiddenHorse, AddPerkPoints | ShakeCamera, FadeOutGame, RequestSave, ForceFirstPerson, ShowRaceMenu, AdvanceSkill and 16 more, plus the DLC crime and sky functions: SendWereWolfTransformation, ServeTime, ClearPrison, SetSunGazeImageSpaceModifier, GetSunPositionX/Y/Z |
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
| ActiveMagicEffect | RegisterForUpdate, RegisterForSingleUpdate, RegisterForUpdateGameTime, RegisterForSingleUpdateGameTime, UnregisterForUpdate, UnregisterForUpdateGameTime, RegisterForAnimationEvent, UnregisterForAnimationEvent, GetBaseObject, GetTargetActor, GetCasterActor, Dispel | The inventory event filters |
| EffectShader, VisualEffect | | Play, Stop |
| ImageSpaceModifier | | Apply, ApplyCrossFade, Remove, RemoveCrossFade, PopTo |
| ShaderParticleGeometry | | Apply, Remove |
| MusicType, Topic | | Add, Remove |
| Weather, Weapon, Scroll | | SetActive, Fire, Cast |

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
  enchantments. Worn enchantment application is not implemented yet.
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
- Health, magicka and stamina use race starting values plus NPC offsets for
  manual stats. Auto-calculated actors use their authored DNAM cache pending
  live class/level calculation. Stats and traits follow separate template
  chains with links resolved in each winning file. Leveled templates return
  an error pending spawn selection. Missing race DATA retains the old
  DNAM-plus-offset fallback for synthetic records. See [actor values](actor-values.md).
- PlayerRef keeps form 0x14 and binds to the Player NPC base when available.
  A base-less synthetic player starts at 100 for the three main values.
  Health reaching zero uses the shared death lifecycle, including essential
  checks, deferred requests and OnDying/OnDeath delivery. Bleedout remains
  unfinished. Healing alone does not resurrect.
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
  other in the notification queue, and returns button 0. `Scene.Start` and
  `Stop` deliver `OnBegin` and `OnEnd` to the scene's scripts. `Sound.Play`
  hands out instance ids. `Spell.Cast` records the cast, no effect applies.
- `SetGameSetting*` writes an overlay `GetGameSetting*` reads first.
- A quest start delivers `OnInit` to its alias objects too, the ones a
  script is attached to, since the alias kind (`ALST` or `ALLS`) now decides
  the class (`ReferenceAlias` or `LocationAlias`) of the VM object.

## Tests

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

- Unit tests per module: VMAD round trip, object format 1, version 3, cuts at
  every offset and a hostile array count. QUST view. Clock. Scenario grammar
  and the log diff. The native registry count against the hand-kept constant.
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
template flags say so). `ai::select` walks it and the first package whose
schedule and conditions hold wins. `Once Per Day` skips a package that ran
to completion on the current game day. `ai::evaluate` switches the actor
when the winner changed and queues `OnPackageEnd`, `OnPackageChange` and
`OnPackageStart` for its scripts. `Actor.EvaluatePackage` calls it at
once, `Actor.GetCurrentPackage` returns the running package, and the
driver re-evaluates every actor once a real second and whenever it has no
package. The natives table rows above still list both natives as stubbed,
they moved to the implemented column with this section.

**Schedules.** `hour` `-1` is any time. A window starts at `hour:minute`
and lasts `duration` minutes, wrapping past midnight, a zero duration
lasts until midnight. Day of week 0 is Sundas, the day the game starts on,
7 to 10 are the Creation Kit's weekday groups. The date and month checks
count 31 day months from the 17th, an approximation no vanilla package
uses (every shipped `PSDT` says any for both).

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
GetActorValuePercent. Any other function fails its condition and is
counted in `World::ai.unsupported_functions` by index, so a live run
prints what the next round should add.

**Procedures.** The template tree is not walked as a tree. Most vanilla
templates are a sequence of a `Travel` to the place and what happens
there, so `ai::procedure_of` takes the strongest procedure it knows from
the chain, `Sleep` over `Sandbox` over `Travel`, with the first location
input that procedure reads (falling back to the first location input of
the chain). The driver then walks to the place itself. `Travel` walks to
the location and holds, marking the package complete on arrival.
`Sandbox` picks an idle marker or a piece of furniture (beds excluded)
within the radius of the location, walks there, dwells 0.1 to 0.4 game
hours, and picks the next, or a random point in the radius when the cell
has no markers. `Sleep` walks to the bed the location names, or the
nearest furniture in that cell whose base carries a keyword with `bed` in
its editor id, and holds with `sleeping` set. Every other procedure name
is `Procedure::Unsupported` and the actor stands where it is. Location
kinds near reference, in cell, near package start, near editor location,
near linked reference, alias reference and near self resolve, alias
location does not (no `LCTN` model yet).

**Walking.** `ai::nav_graph` builds `soe_navmesh::vanilla` over every
winning `NAVM` on first use and remembers which cell each mesh sits in.
A route is `find_path_between` from the actor to the place, the actor
follows the waypoints at 110 units per real second (walk), 160 (fast
walk), 180 (jog) or 375 (run) when the package's preferred speed flag is
set, values from the default movement type since race movement data is
not read yet. Each pulled waypoint carries its triangle owner. Reaching a portal into another
mesh moves the actor into that mesh's cell, which is how travel across a cell
link changes `RefState::cell`. Explicit gap departures retain the source cell
until the arrival waypoint. Off the graph the route is a straight line. The driver
skips the player, the dead and the unconscious.

**Live run (2026-09-18).** Every one of the 5,961 `PACK` records in
`Skyrim.esm` parses (104 templates, 2,384 with conditions, 1,545 with a
scheduled hour) in 7.9 s including the plugin parse. 4,635 packages
resolve to a procedure the driver runs: 2,467 `Travel`, 1,841 `Sandbox`,
327 `Sleep`. The rest by template procedure: Patrol 378, Sit 300,
HoldPosition 141, Follow 128, UseIdleMarker 61, UseMagic 52, Orbit 47,
Escort 45, Activate 40, ForceGreet 31, Shout 26, Guard 24, Hover 14, Flee
13, Find 11, Wait 11, DialogueActivate 2, UseWeapon 2. The `Prisoner`
base carries no `PKID` list at all, so the innkeeper `Hulda` stands in:
her stack is `WhiterunInnkeeperPackage`, which resolves to `Sandbox` at
every hour tried, and no condition function on the way was unsupported.
The synthetic tests (`tests/packages.rs`) walk a guard across two
navmeshed cells on a travel package once a quest stage unlocks it, sandbox
to an idle marker, sleep on schedule and wake at 08:00, take an alias
package over the base list, switch at once from a script's
`EvaluatePackage`, and keep walking after a save and load.

**Not done.** Procedure trees as trees (sequence, random, simultaneous,
stacked branches with their own conditions), the other 18 procedures,
package fragments (`VMAD` on `PACK`), the `On Begin`, `On End` and `On
Change` idles and topics, once per day for packages that end without
arriving, `Must Complete`, interrupt flags, combat overrides, door
unlocking flags, race movement speeds, the `Package Data` and `Event Data`
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
and `CanFlyHere`, which wants the permission and an exterior cell), the
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
- `CanFlyHere` reads the interior flag of the actor's cell. The real engine
  also asks the navmesh and the weather.

A second pass added the reference animation graph (`SetAnimationVariable`
and `GetAnimationVariable` in all three types, one store per type on the
reference), `SetAlpha`, `PlayIdle`, `GetNthLinkedRef` and `PlaceActorAtMe`,
with quiet stubs for the shader, visual effect, image space modifier, music
and translation calls the DLC scripts lean on hardest. It also fixed a real
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
table, magic `SOEW`, version 11. Versions 4 to 6 use legacy actor values,
version 5 adds active magic, version 6 adds dialogue, version 7 adds actor-value
components, version 8 adds dialogue to those components, and version 9 adds
active magic. Version 10 adds per-reference leveled template choices.
Version 11 adds quest journal instances and mapped runtime metadata.
Older formats load with empty state for absent subsystems.
Legacy current and base values must agree. Otherwise their modifier domains
are ambiguous and restoration returns an explicit error.
The state uses the same `Writer`/`Reader` encoding as
`soe_papyrus::vm::state`, an `ArrayPool` and `ArrayTable` of its own ahead of
the body for the `Value` arguments a pending event or call is carrying.

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

`Game::save_state(&self) -> Result<Vec<u8>, VmError>` and
`Game::load_state(&mut self, bytes) -> Result<(), VmError>`
(`crates/game/src/save.rs`) wrap `World::save_state`/`load_state` and
`soe_papyrus::Vm::save_state`/`load_state` behind one envelope, magic
`SOEG`, version 1, each inner blob length-prefixed. Neither the load order
nor the loaded scripts travel in the file, `load_state` expects a `Game`
built against the same load order with the same scripts already loaded, the
way `Game::new` and `Game::load_script` already ask for them, and it should
not have been bound or started first since it overwrites object and instance
state wholesale.

A test in `tests/quest_flow.rs` calls the new `Game::activate_no_settle`
(the part of `Game::activate` before `run_until_idle`) on the lever so the
`OnActivate` event is queued but not yet run, saves, loads the bytes into a
freshly built `Game` with the same scripts and no binding done, then runs
until idle and checks it reaches the same end state as
`a_quest_advances_on_activation_end_to_end`: the quest completed at stage 20,
the objective completed, the gold given, the lever disabled, the
notification fired, and the alias resolved to the lever.

## Not done, and known gaps

- Race starting attributes are read. Live class/level calculations, racial
  skill bonuses and encounter-zone selection context remain unfinished. Runtime
  template groups share persistent leveled choices, with the remaining
  inheritance boundaries documented in [actor-values.md](actor-values.md).
- Weapon combat remains unfinished. Dialogue and magic have the limited
  execution paths documented below. Packages run three
  procedures, see the Packages section for the rest. Scenes play as a flag
  with `OnBegin` and `OnEnd`, no phases or actions. Perks are a set on the
  actor, no entry point applies. Leveled lists are never rolled. Spell casts
  are recorded, no effect applies. Crime has bounty and infamy ledgers, no
  jail or arrest.
- Faction relations from `XNAM`, location keyword data, perks and the actor
  flags were tested on the synthetic master and reached by the vanilla
  scripts at start, not checked against the game's runtime values.
- Quest aliases are filled only from `ALFR`. Conditions, `ALCO`, location and
  reference fill types are not evaluated.
- `SetStage` does not display the log entry or play the quest sound.
- Localized strings come back as `<string N>` until the bsa and strings
  readers are wired in.
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
OnItemAdded for the output. The next game pump executes handlers. Reference
and running quest alias recipients have independent filters, applied when
events are queued. Filter lists inspect immediate members only. Empty lists
suppress events, and later filter changes do not cancel pending calls.
The script-facing AddItem, RemoveItem and RemoveAllItems stack paths now
queue these events too. Silent removal still notifies scripts. AddItem rejects
unsupported reference, list and leveled-list inputs instead of reporting them
as inventory base forms. Their full semantics and active-effect recipients
remain pending with item-instance and magic execution.

The condition evaluator implements functions 47, 56, 58, 59, 72, 74, 136,
182, 264, 289, 359, 448, 543, 560, 629, 630 and 659. This is a bounded set,
not the entire CTDA contract. It supports ordered AND/OR groups, numeric or
global comparisons, explicit references and caller-supplied quest aliases.
Reserved PlayerRef resolves to the runtime player even without a placed record.
New runtime players retain reserved FormID 00000014 and use the Player NPC
base when present, so forced player aliases and inventory notifications agree.
Conditional Papyrus variables come from current attached instances, with
ambiguous or nonnumeric values reported as errors. Unavailable targets,
unsupported flags/functions, nonzero HasPerk parameter 2 and missing tempering
context are explicit errors. Crafting does not invent a target or quest context.
Float comparisons are exact and have not been compared against vanilla
near-equality behavior. Inventory instance queries remain unfinished.

Sources consulted were local xEdit `wbDefinitionsTES5.pas` for CTDA, COBJ and
FURN WBDT, UESP COBJ notes for missing outputs, skymp `CraftService.cpp` and
`unit/CraftTest.cpp` for bench matching and DLC cases, Mutagen
`Skyrim/Constants.cs` for PlayerRef, and CK inventory event/filter pages.
The skymp evaluator's unknown-function success and unresolved target mapping
were not adopted. Mutagen labels HasPerk's second integer unused, while xEdit
exposes it, so nonzero values need further evidence. Caprica's property
compiler confirms conditional auto-property flags on the backing variable.

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

Tempering, alchemy, enchanting, skill advancement, crafting story events,
item instance state and host menus remain unfinished. The full requested
execution scope is tracked in [execution-plan.md](execution-plan.md).

The game crate now directly uses the workspace's existing thiserror 2.0.20
for typed execution errors. `cargo tree -p soe-game -i thiserror` confirms
the dependency was already present through esm, bsa, navmesh and profile.
No package versions or new transitive dependencies were added.

## Dialogue execution, 2026-09-20

`Game::begin_dialogue`, `dialogue_choices`, `choose_dialogue`,
`dialogue_line`, `advance_dialogue` and `close_dialogue` provide a conversation
lifecycle over winning DIAL, INFO, DLBR and QUST records. Normal top-level
branches supply the initial choices. Quest priority and topic priority sort
those choices. Inactive quests and failed quest/INFO conditions suppress them.
Selection checks conditions again, so a stale menu cannot start its fragment.
Unknown conditions return diagnostics instead of becoming true.

INFO ownership comes from the winning TopicChildren group, then the owning
DIAL's QNAM. TPIC is a previous-topic link. PNAM determines INFO ordering,
including deleted anchors and inserted overrides. Shared INFO supplies response
data without transferring conditions, fragment scripts or quest ownership.
TCLT supplies follow-up DIAL links. Goodbye closes after response completion.
Random groups draw from the saved world RNG only during selection. Say Once
and reset hours use saved history, with reset time measured in game hours.
Original-engine comparison of random-group selection remains open.

INFO VMAD begin/end fragments run on their named script attachment through
`Vm::call_script`. Both methods are checked before starting a response. The
speaker reference is the fragment argument. Begin runs before the first line,
and end is dispatched only after the host acknowledges the final response.
Closing an interrupted conversation does not dispatch its end fragment. A
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
46, 47, 56, 58, 59, 67, 69, 70, 71, 72, 73, 74, 80, 136, 182, 264, 289, 359,
403, 448, 543, 560, 566, 606, 629, 630 and 659. Run-on subject, target,
explicit reference and quest alias are supported when their context exists.
Conditions use current shared actor, quest, inventory and location state.
Papyrus variable queries require conditional flags and reject ambiguous names.
The evaluator does not complete missing actor-value, alias or package systems.

SOEW version 9 stores the conversation cursor, sorted INFO history and active
magic effects alongside actor-value components. Formats without dialogue load
an empty conversation state. Dialogue state contributes to the
simulation hash. The derived record index is rebuilt from the load order.
This is engine-save support, not vanilla ESS conversation restoration.

Sources are local xEdit DIAL/INFO/DLBR/QUST and wbConditionFunctions, UESP
VMAD_Field, CK GetDialogueTarget and IsInDialogueWithPlayer, shipped
Location.psc, and Mutagen's DialogTopic reader. The PNAM ordering algorithm
adapts GPL Wrye Bash `Mopy/bash/brec/record_groups.py`,
`_DialChildren._sort_group`, with explicit cycle rejection. It remains in the
GPL game crate. The schema sources do not provide a reusable Rust interpreter.

The gated five-master test checks all 40,678 live INFO records for ownership,
7,799 fragment declarations and 19,368 ordered topic groups. INFO fragment
bytes survive re-encoding exactly. Script headers follow the existing writer's
version 5/object format 2 normalization. The survey also exercises an authored
Hulda topic through the conversation API. That topic has no VMAD fragment,
so this run does not verify shipped fragment execution. Synthetic tests
verify named fragments changing quest stages. The text remains a localized string
identifier, not a displayed subtitle or an audible line.

Remaining dialogue work includes missing conditions, blocking/exclusive
branches, automatic greetings and ForceGreet, refusal/invisible continuation,
favor spending, do-all-before-repeating, walk-away behavior, scenes, persuasion,
services, complete alias behavior and interruption rules. Godot activation,
menus, string-table resolution, voice assets, LIP and idle playback are not
connected to this API. This increment does not establish playable dialogue
or complete the broader gameplay goal.

## Active magic applications

The runtime provides `World::cast_spell` for direct scripted casts
of non-hostile Script, Value Modifier and Dual Value Modifier effects. Definitions
come from winning SPEL and MGEF records, with each FormID resolved in its winning
plugin. Self and target-actor delivery are supported for constant and fire-and-forget
casts. `Spell.Cast` credits an actor source. `RemoteCast` credits its explicit
blame actor. Player charging, costs and hand input are separate unfinished work.

Each application gets an `EffectId` and a distinct `ActiveMagicEffect` VM object.
MGEF scripts are attached when an effect is applied, rather than attached to the
MGEF form at startup. VMAD properties use the winning MGEF's plugin context.
The queue binds the scripts before sending `OnEffectStart`, and expiry or dispel
sends `OnEffectFinish`. The getters return the base MGEF, target actor and caster
while the application is live. Finish handlers may observe absent native state,
as allowed by the shipped ActiveMagicEffect.psc contract.

Recover modifiers change the target once and reverse their own delta at the end.
Other value modifiers integrate magnitude over unpaused simulation seconds, with
the final tick clipped to the remaining duration. Zero-duration effects apply
once. Healing health, magicka and stamina is limited to the maximum, including permanent and temporary modifiers. No Recast prevents a second live application of the
same MGEF on the same actor. Death removes effects unless No Death Dispel is set.
SOEW version 9 stores identity, source, caster, target, magnitude, elapsed time,
duration and value deltas, including pending script attachment. Loading never
reapplies a saved modifier. The deterministic scenes hash includes this state.

The source contracts are CK [Magic Effect](https://ck.uesp.net/wiki/Magic_Effect),
local `ck-wiki/DispelSpell_-_Actor.wikitext` and
`DispelAllSpells_-_Actor.wikitext`, the shipped
`skse64/scripts/vanilla/ActiveMagicEffect.psc`, xEdit's SPEL/MGEF definitions, and
CommonLibSSE-NG `ActorValues.h` and `MagicSystem.h`. The skymp
`ActiveMagicEffectsMap` was read for comparison. Its one-effect-per-actor-value
map and wall-clock expiry do not fit independent applications or menu pause, so
those policies were not reused. `thiserror` is now a direct game dependency for
typed cast errors. It was already present transitively, with no new package.

Unsupported casts return a typed error before applying any of their effects,
and Papyrus casts log that error. This includes hostile hit resolution,
projectiles, contact and location delivery, concentration, conditions, area,
taper, perk adjustments, equip abilities, keyword dispelling and other archetypes.
Actor-value indices above 45 are not yet supported. Recover effects use the temporary modifier domain. Other effects damage or
restore the damage component without changing permanent modifiers.
Automatic ability application from AddSpell or NPC spell lists, enchantments,
potions, resistance and absorption, effect artwork and sounds, actor-event
forwarding, effect inventory filters and VM object reclamation remain unfinished.
This is an active-effect foundation, not complete magic execution.

Validation on the installed five-master load order found 1,354 non-deleted
SPEL winners. The direct path accepted 178 spells and created 205 applications.
The survey checks definitions, identity and removal without running their scripts.
The rest returned explicit unsupported reasons. The installed startup still ran
423 quests through 1,388 native calls with zero unresolved calls. Binding now
attached 27,563 scripts, with seven missing scripts and 88 missing properties,
because MGEF script attachments wait until an application exists.

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
Equipment is the current actor state's equipped set. Outfit auto-equipping,
armor template inheritance and runtime skin overrides
are not supplied by this path. The World footstep path supplies the selected trait source for leveled actors.
Direct record-only actor_sets calls still require a non-leveled trait chain.

Eight actor-selection tests pass, including installed-data checks for Nord
bare feet, light boots, heavy boots and wolf skin. The same run resolves all
161 non-deleted race skins into 41 distinct sets across the five masters.
Synthetic cases cover live equipment/race changes, implicit player identity,
sex-specific slot priorities, skin replacement, additional and inherited races,
owner-relative overrides, malformed links and cyclic armor-parent chains.

## Sound preparation (2026-09-19)

`sound::prepare` resolves SNDR files, signed frequency shift, frequency and
attenuation variation, SNCT parent gain and menu defaults, underwater muting,
and SOPM output parameters. Slider overrides are explicit inputs. Descriptor,
category and output links use their winning owners. Category cycles, malformed
fields, invalid random draws and unsupported descriptor types return errors.
Conditional descriptors return an unsupported error until connected to the
shared condition evaluator. Loop modes and alternate links are preserved.

Sources are local xEdit SNDR/SNCT/SOPM definitions, UESP's corresponding pages,
and CommonLib `BGSStandardSoundDef.h`, `BGSSoundCategory.h` and
`BGSSoundOutput.h`. The three caller-supplied random draws select a file,
uniform signed pitch variation and uniform additional attenuation. This
distribution is engine policy, not a measured vanilla random sequence.
Player footsteps draw from the world's saved RNG.

Five synthetic tests cover file/path choice, pitch and decibels, category
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

Lockpicking, difficulty scaling, key-use messages, crime handling, reciprocal
door lock synchronization and `Lock`'s abAsOwner public/private-cell effect
remain pending. Lock events currently target reference scripts, without alias
forwarding. Logical closing does not animate or update the host collision of
a swinging door. The ordinary unloaded/load-door resource path is unchanged.

## Vanilla reference placement restoration

`World::restore_vanilla_reference_placements` applies saved identities, bases
and transforms to a fresh reference table, reserves dynamic IDs and restores
the player location without queuing events. It validates the entire pass
before changing the world. Its report identifies missing dependencies and
unapplied state. Inventory, actor values, extra data and saved execution remain
separate requirements. See [save.md](save.md#reference-placements) for sources,
validation and the limits of this intermediate reconstruction pass.

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
the actor's shout collection separately from spells. Leveled spell lists are
retained, but their execution remains a separate requirement.

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
The combined full gate passes 1,163 Rust tests and 11 Python tests, with no
warnings or skipped optional tools. Installed-data validation passes 12 melee
and 11 death-lifecycle tests. It decodes 3,262 winning weapons and 3,675 armors,
then executes 184 supported weapons against synthetic actors. The remaining
weapons reject unsupported types, templates or enchantments. Real armor records
were decoded, while armor damage calculations use synthetic fixtures.
These changes await commit and merge review on `combat-hits`.
