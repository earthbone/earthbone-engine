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
  front. Run-time forms from `PlaceAtMe` live in file slot
  `FF` like the game's own. `world.rs` holds the state types, the `World`
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

**Forms, not structs.** Every native takes and returns forms by
`GlobalFormId`. The VM sees them as objects created on first use with the
class the record's signature implies (`QUST` is a `Quest`, `ACHR` an `Actor`),
so a form with no script still answers its class's natives. The VM gained
`create_object_of` and `NativeCall::create_object` for this.

**Natives cannot touch the VM.** They run inside a VM thread. Anything they
want the VM to do, an `OnActivate` event, an `OnInit`, a stage fragment call,
goes into `World::pending`, and `Game::run_until_idle` alternates draining that
queue with running the VM until both are empty. Timers are the same: `tick`
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

Counts from `natives::register`: 349 natives with state behind them and 191
quiet stubs (141 and 176 before the natives round of 2026-09-18, 294 and 137
before the DLC round). A stub returns the value the game gives when nothing
is going on (`IsBleedingOut` false, `ShakeCamera` nothing) so vanilla scripts
run past it.

| Class | Implemented | Stubbed |
|---|---|---|
| Form | GetFormID, GetName, GetType, HasKeyword, GetKeywords, RegisterForSingleUpdate, RegisterForUpdate, RegisterForSingleUpdateGameTime, RegisterForUpdateGameTime, UnregisterForUpdate, UnregisterForUpdateGameTime, RegisterForSleep, UnregisterForSleep, RegisterForTrackedStatsEvent, UnregisterForTrackedStatsEvent, RegisterForAnimationEvent, UnregisterForAnimationEvent, RegisterForLOS, RegisterForSingleLOSGain, RegisterForSingleLOSLost, UnregisterForLOS | |
| ObjectReference | Enable, EnableNoWait, Disable, DisableNoWait, IsEnabled, IsDisabled, Delete, IsDeleted, GetBaseObject, GetParentCell, GetWorldSpace, IsInInterior, GetPositionX/Y/Z, GetAngleX/Y/Z, SetPosition, SetAngle, MoveTo, GetDistance, Activate, BlockActivation, IsActivationBlocked, GetLinkedRef, GetItemCount, AddItem, RemoveItem, RemoveAllItems, GetScale, SetScale, GetDisplayName, SetDisplayName, PlayAnimation, GetOpenState, SetOpen, Lock, IsLocked, GetLockLevel, SetLockLevel, PlaceAtMe, AddToMap, EnableFastTravel, SetDestroyed, IgnoreFriendlyHits, IsIgnoringFriendlyHits, GetCurrentLocation, GetEditorLocation, IsInLocation, MoveToMyEditorLocation, GetActorOwner, GetFactionOwner, SetActorOwner, SetFactionOwner, AddInventoryEventFilter, RemoveInventoryEventFilter, RemoveAllInventoryEventFilters, SetAnimationVariableFloat/Int/Bool, GetAnimationVariableFloat/Int/Bool, GetNthLinkedRef, PlaceActorAtMe | Is3DLoaded, IsFurnitureInUse, GetCurrentScene, SetMotionType, ApplyHavokImpulse, PushActorAway and 6 more, plus the DLC ragdoll, cart and translation: ForceAddRagdollToWorld, ForceRemoveRagdollFromWorld, TetherToHorse, IsFurnitureMarkerInUse, Reset, TranslateTo, SplineTranslateTo, StopTranslation, CreateDetectionEvent |
| Actor | GetActorValue, GetBaseActorValue, SetActorValue, ModActorValue, DamageActorValue, RestoreActorValue, GetActorValuePercentage (and the AV short names), IsDead, Kill, GetKiller, Resurrect, IsInCombat, StartCombat, StopCombat, GetCombatTarget, GetActorBase, GetLeveledActorBase, IsEssential, IsPlayerTeammate, SetPlayerTeammate, IsGhost, SetGhost, IsProtected, GetLevel, GetRace, IsUnconscious, SetUnconscious, SetRestrained, IsDoingFavor, SetDoingFavor, AddSpell, RemoveSpell, HasSpell, EquipItem, UnequipItem, IsEquipped, IsInFaction, GetFactionRank, AddToFaction, RemoveFromFaction, SetFactionRank, AddPerk, RemovePerk, HasPerk, SetAlert, IsAlerted, StartSneaking, IsSneaking, DrawWeapon, SheatheWeapon, IsWeaponDrawn, IsArrested, ClearArrested, SetAttackActorOnSight, SetDontMove, SetAllowPCDialogue, SetRelationshipRank, GetRelationshipRank, GetHighestRelationshipRank, GetLowestRelationshipRank, GetCrimeFaction, SetCrimeFaction, SetOutfit, and from the DLC: SendVampirismStateChanged, SendLycanthropyStateChanged, SetAllowFlying, SetAllowFlyingEx, IsAllowedToFly, CanFlyHere, StartDeferredKill, EndDeferredKill, KillSilent, SetRace, SetEyeTexture, SetCriticalStage, AttachAshPile, SetNoBleedoutRecovery, GetNoBleedoutRecovery, EnableAI, AddShout, RemoveShout, EquipShout, UnequipShout, GetEquippedShout, EquipSpell, UnequipSpell, SetSubGraphFloatVariable, PlaySubGraphAnimation, WornHasKeyword, IsPlayersLastRiddenHorse, SetAlpha, PlayIdle | IsAlarmed, IsBleedingOut, GetCombatState, EvaluatePackage, GetEquippedWeapon, SetLookAt and 34 more, plus the DLC ones without a system: IsFlying, HasMagicEffect, HasMagicEffectWithKeyword, DispelSpell, DispelAllSpells, GetWarmthRating, ClearExtraArrows, DoCombatSpellApply, MoveToPackageLocation |
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
| TopicInfo | | GetOwningQuest, None until the dialogue view lands |
| ActiveMagicEffect | RegisterForUpdate, RegisterForSingleUpdate, RegisterForUpdateGameTime, RegisterForSingleUpdateGameTime, UnregisterForUpdate, UnregisterForUpdateGameTime, RegisterForAnimationEvent, UnregisterForAnimationEvent | GetBaseObject, GetTargetActor, GetCasterActor, Dispel, the inventory event filters |
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
- `DispelSpell` and `DispelAllSpells` are stubs pending active effects. They
  preserve known and equipped spells. `DispelSpell` returns false because
  no effect was removed. The CK function pages require effect removal, and
  `DispelAllSpells` excludes abilities, diseases, worn or constant
  enchantments and addictions. Cast recording is not an effect lifecycle.
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
  Health reaching zero marks death unless essential or deferred, with no
  death-event or bleedout lifecycle yet. Healing alone does not resurrect.
- `GetAngle*` return degrees, the record stores radians.
- Update timers run on real seconds, the game-time variants on game hours.
- `Game.GetForm` accepts the runtime id under the current load order, light
  plugins included. `GetFormFromFile` takes the low 24 bits and a file name.
- A leveled list is its `LVLO` entries in record order plus what `AddForm`
  appended. `Revert` truncates back to the record's entries and chance.
  Nothing rolls a list yet, that is the inventory and spawn work.
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
class. Its own getters and `Dispel` are stubs until there is an active
effect model.

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
table, magic `SOEW`, version 7 (version 2 added the package driver state
to every actor and the natives round state, version 3 the DLC state:
vampirism and lycanthropy, flying, the deferred kill, eye texture, critical
stage, bleedout, shouts, the behavior graph floats and the player's perk
points, version 4 the resident cell, version 7 actor-value components).
Version 4 reads only when each actor's old current and base values agree.
A difference cannot identify which modifier produced it, so restoration
returns an explicit ambiguity error. Versions 5 and 6 belong to separate
magic and dialogue worktrees and need format reconciliation before integration.
The state uses the same `Writer`/`Reader` encoding as
`soe_papyrus::vm::state`, an `ArrayPool` and `ArrayTable` of its own ahead of
the body for the `Value` arguments a pending event or call is carrying.

A form's record-derived data is not stored twice. A quest's definition (its
stages, objectives and aliases as `QUST` and `VMAD` wrote them) is read back
from the load order the same `quest_state` call already uses to build it the
first time, `load_state` keeps only the mutable half a running game changes:
whether it is running, completed, failed or active, the current and highest
stage, the done set, per-objective displayed and completed and failed flags,
and where each alias currently points. Everything with no record behind it,
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
  skill bonuses and leveled-template selection remain unfinished. Other NPC
  template groups still use the older one-level view and need correction.
- No combat, magic, dialogue. Their natives are stubs. Packages run three
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
