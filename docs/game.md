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
  front. Run-time forms (`PlaceAtMe`, the fallback player) live in file slot
  `FF` like the game's own.
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

**Semantics.** From the Creation Kit wiki's function pages and play
behaviour. The wiki is behind a Cloudflare challenge for plain fetches, so
`engine-refs/ck-wiki/` is empty until a browser session fills it, and every
native below is marked for review against its page once that happens.

## Conformance table

Counts from `natives::register`: 294 natives with state behind them and 137
quiet stubs (141 and 176 before the natives round of 2026-09-18). A stub
returns the value the game gives when nothing is going on (`IsBleedingOut`
false, `ShakeCamera` nothing) so vanilla scripts run past it.

| Class | Implemented | Stubbed |
|---|---|---|
| Form | GetFormID, GetName, GetType, HasKeyword, GetKeywords, RegisterForSingleUpdate, RegisterForUpdate, RegisterForSingleUpdateGameTime, RegisterForUpdateGameTime, UnregisterForUpdate, UnregisterForUpdateGameTime, RegisterForSleep, UnregisterForSleep, RegisterForTrackedStatsEvent, UnregisterForTrackedStatsEvent, RegisterForAnimationEvent, UnregisterForAnimationEvent, RegisterForLOS, RegisterForSingleLOSGain, RegisterForSingleLOSLost, UnregisterForLOS | |
| ObjectReference | Enable, EnableNoWait, Disable, DisableNoWait, IsEnabled, IsDisabled, Delete, IsDeleted, GetBaseObject, GetParentCell, GetWorldSpace, IsInInterior, GetPositionX/Y/Z, GetAngleX/Y/Z, SetPosition, SetAngle, MoveTo, GetDistance, Activate, BlockActivation, IsActivationBlocked, GetLinkedRef, GetItemCount, AddItem, RemoveItem, RemoveAllItems, GetScale, SetScale, GetDisplayName, SetDisplayName, PlayAnimation, GetOpenState, SetOpen, Lock, IsLocked, GetLockLevel, SetLockLevel, PlaceAtMe, AddToMap, EnableFastTravel, SetDestroyed, IgnoreFriendlyHits, IsIgnoringFriendlyHits, GetCurrentLocation, GetEditorLocation, IsInLocation, MoveToMyEditorLocation, GetActorOwner, GetFactionOwner, SetActorOwner, SetFactionOwner, AddInventoryEventFilter, RemoveInventoryEventFilter, RemoveAllInventoryEventFilters | Is3DLoaded, IsFurnitureInUse, GetCurrentScene, SetMotionType, ApplyHavokImpulse, PushActorAway and 6 more |
| Actor | GetActorValue, GetBaseActorValue, SetActorValue, ModActorValue, DamageActorValue, RestoreActorValue, GetActorValuePercentage (and the AV short names), IsDead, Kill, GetKiller, Resurrect, IsInCombat, StartCombat, StopCombat, GetCombatTarget, GetActorBase, GetLeveledActorBase, IsEssential, IsPlayerTeammate, SetPlayerTeammate, IsGhost, SetGhost, IsProtected, GetLevel, GetRace, IsUnconscious, SetUnconscious, SetRestrained, IsDoingFavor, SetDoingFavor, AddSpell, RemoveSpell, HasSpell, EquipItem, UnequipItem, IsEquipped, IsInFaction, GetFactionRank, AddToFaction, RemoveFromFaction, SetFactionRank, AddPerk, RemovePerk, HasPerk, SetAlert, IsAlerted, StartSneaking, IsSneaking, DrawWeapon, SheatheWeapon, IsWeaponDrawn, IsArrested, ClearArrested, SetAttackActorOnSight, SetDontMove, SetAllowPCDialogue, SetRelationshipRank, GetRelationshipRank, GetHighestRelationshipRank, GetLowestRelationshipRank, GetCrimeFaction, SetCrimeFaction, SetOutfit | IsAlarmed, IsBleedingOut, GetCombatState, EvaluatePackage, GetEquippedWeapon, SetLookAt and 34 more |
| ActorBase | GetRace, GetClass, GetSex, GetDeadCount, IsEssential, IsProtected, IsInvulnerable, IsUnique, SetEssential, SetProtected, SetInvulnerable, SetOutfit | GetGiftFilter |
| Quest | Start, Stop, Reset, IsRunning, IsCompleted, IsActive, SetActive, CompleteQuest, CompleteAllObjectives, FailAllObjectives, SetStage, SetCurrentStageID, GetStage, GetCurrentStageID, GetStageDone, IsStageDone, SetObjectiveDisplayed, SetObjectiveDisplayedNoWait, SetObjectiveCompleted, SetObjectiveFailed, IsObjectiveDisplayed, IsObjectiveCompleted, IsObjectiveFailed, ModObjectiveGlobal, UpdateCurrentInstanceGlobal, GetAlias, GetAliasById, GetAliasByName, GetID, GetPriority, GetQuest | IsStarting, IsStopping |
| Alias, ReferenceAlias, LocationAlias | GetOwningQuest, GetID, GetName, GetReference, GetRef, GetActorReference, GetActorRef, ForceRefTo, ForceRefIfEmpty, Clear, GetLocation, ForceLocationTo, AddInventoryEventFilter, RemoveInventoryEventFilter, RemoveAllInventoryEventFilters | |
| LeveledItem, LeveledActor, LeveledSpell | AddForm, Revert, GetNumForms, GetNthForm, GetNthLevel, GetNthCount, GetChanceNone, SetChanceNone (the last six are SKSE's, the state is there anyway) | |
| Location | GetKeywordData, SetKeywordData, IsCleared, SetCleared, IsChild, HasCommonParent, IsSameLocation | IsLoaded, HasRefType, GetRefTypeAliveCount, GetRefTypeDeadCount |
| Faction | SetAlly, SetEnemy, GetReaction, SetReaction, GetCrimeGold, GetCrimeGoldViolent, GetCrimeGoldNonViolent, SetCrimeGold, SetCrimeGoldViolent, ModCrimeGold, CanPayCrimeGold, PlayerPayCrimeGold, GetInfamy, GetInfamyViolent, GetInfamyNonViolent, IsPlayerExpelled, SetPlayerExpelled, SetPlayerEnemy, IsFactionInCrimeGroup | SendAssaultAlarm, SendPlayerToJail, ModReaction |
| Game | GetPlayer, GetForm, GetFormEx, GetFormFromFile, EnablePlayerControls, DisablePlayerControls, GetGameSettingFloat/Int/String, SetGameSettingFloat/Int/Bool/String, GetRealHoursPassed, IncrementStat, QueryStat, IsFastTravelEnabled, EnableFastTravel, SetPlayerAIDriven, SetInChargen, SetBeastForm, TeachWord, UnlockWord, IsWordUnlocked, AddAchievement, GetPlayersLastRiddenHorse | ShakeCamera, FadeOutGame, RequestSave, ForceFirstPerson, ShowRaceMenu, AdvanceSkill and 16 more |
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

Semantics worth stating because they were decided here, not read:

- `SetStage` on a quest that is not running starts it (`OnInit`, then the
  stage), records the stage in the done set, updates the current and highest
  stage, applies the log entry flags (Complete Quest stops the quest), and
  queues every fragment registered for that stage on the quest's own scripts.
- `Start` delivers `OnInit` and then sets the start-up stage.
- `Activate` runs default processing unless the reference blocks it: a door
  with `XTEL` moves the activator to the far door's cell and landing
  transform, a container marks itself opened, then `OnActivate` goes to the
  target's scripts with the activator. `Activate(x, true)` skips the event.
- `GetStage` is the highest stage set, `GetCurrentStageID` the last one.
- Inventories start from the base record's `CNTO` list.
- Actor values come from the base `NPC_`: `DNAM` health, magicka, stamina
  and the 18 skills plus the `ACBS` offsets, with the `TPLT` template applied
  one level where the template flags say so. Race starting values are not
  read yet, so they add nothing. `GetLevel` applies the PC level multiplier
  against `World::player_level` and clamps to the calc range. An actor with
  no base (the fallback player) starts at 100 for the three main values.
  Health reaching zero kills a non-essential actor.
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

`tests/npc.rs` against the installed Skyrim.esm: the player base (`Player`,
form 7) reads level 1, health 150, magicka 150, stamina 50, OneHanded 20,
Sneak 15, three spells, four factions, sixteen items, race NordRace.
`MQ101Alduin` reads through its template (flags 0xf63): level 10, essential,
health 1550, magicka 80, four spells. `Prisoner` is a `CLAS` record, not an
actor base.

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
(`Actor.SendVampirismStateChanged`, a Dawnguard script function called on an
actor whose script is not attached in that flow), 87 log lines, in 0.3
seconds in a debug build. The most called natives are now
`Location.SetKeywordData` 149, `Game.GetPlayer` 100,
`GlobalVariable.GetValue` 93, `Debug.OpenUserLog` and `Game.GetForm` 77 each,
`Quest.IsStageDone` 60, `ObjectReference.GetLinkedRef` 56,
`LeveledItem.AddForm` 54, `Alias.GetReference` 45,
`ObjectReference.GetCurrentLocation` and `Quest.SetObjectiveDisplayed` 42
each, `Faction.SetAlly` 37. The remaining log lines are calls on `None`
objects (`cannot call RegisterForUpdate() on a None object` from
`QF_MS14` and the caravan fragments), which is the game's own behaviour for
an alias or property that is empty at start.

## Save and load

`World::save_state(&self) -> Vec<u8>` and `World::load_state(&mut self,
bytes) -> Result<(), VmError>` write and read the run-time half of the form
table, magic `SOEW`, version 2 since the natives round (version 1 files are
refused, nothing shipped them), in the same `Writer`/`Reader` encoding as
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

- Race records are not read, so race starting stats and the skill bonuses
  they carry are missing from actor values.
- No packages, AI, combat, magic, dialogue. Their natives are stubs. Scenes
  play as a flag with `OnBegin` and `OnEnd`, no phases or actions. Perks are
  a set on the actor, no entry point applies. Leveled lists are never rolled.
  Spell casts are recorded, no effect applies. Crime has bounty and infamy
  ledgers, no jail or arrest.
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
