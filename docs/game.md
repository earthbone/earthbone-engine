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
- `Game`: owns the VM and the world. `bind` attaches every script the load
  order names and fills its properties, `start` also starts the quests flagged
  Start Game Enabled, `tick` advances time, `activate` and `send_event` do what
  a player or the engine would, `pump` drains what natives queued.

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

Counts from `natives::register`: 141 natives with state behind them and 176
quiet stubs. A stub returns the value the game gives when nothing is going on
(`IsSneaking` false, `ShakeCamera` nothing) so vanilla scripts run past it.

| Class | Implemented | Stubbed |
|---|---|---|
| Form | GetFormID, GetName, GetType, HasKeyword, GetKeywords, RegisterForSingleUpdate, RegisterForUpdate, RegisterForSingleUpdateGameTime, RegisterForUpdateGameTime, UnregisterForUpdate, UnregisterForUpdateGameTime | |
| ObjectReference | Enable, EnableNoWait, Disable, DisableNoWait, IsEnabled, IsDisabled, Delete, IsDeleted, GetBaseObject, GetParentCell, GetWorldSpace, IsInInterior, GetPositionX/Y/Z, GetAngleX/Y/Z, SetPosition, SetAngle, MoveTo, GetDistance, Activate, BlockActivation, IsActivationBlocked, GetLinkedRef, GetItemCount, AddItem, RemoveItem, RemoveAllItems, GetScale, SetScale, GetDisplayName, SetDisplayName, PlayAnimation, GetOpenState, SetOpen, Lock, IsLocked, GetLockLevel, SetLockLevel, PlaceAtMe | Is3DLoaded, IsFurnitureInUse, IsInLocation, GetCurrentLocation, GetActorOwner, SetMotionType, ApplyHavokImpulse and 14 more |
| Actor | GetActorValue, GetBaseActorValue, SetActorValue, ModActorValue, DamageActorValue, RestoreActorValue, GetActorValuePercentage (and the AV short names), IsDead, Kill, Resurrect, IsInCombat, StartCombat, StopCombat, GetActorBase, GetLeveledActorBase, IsEssential, IsPlayerTeammate, SetPlayerTeammate, IsGhost, SetGhost, IsProtected, GetLevel, GetRace, IsUnconscious, SetUnconscious, IsDoingFavor, SetDoingFavor, AddSpell, RemoveSpell, HasSpell, EquipItem, UnequipItem, IsEquipped, IsInFaction, GetFactionRank, AddToFaction, RemoveFromFaction, SetFactionRank | IsAlerted, IsSneaking, IsWeaponDrawn, GetCombatState, EvaluatePackage, SetAlert, GetEquippedWeapon and 50 more |
| Quest | Start, Stop, Reset, IsRunning, IsCompleted, IsActive, SetActive, CompleteQuest, CompleteAllObjectives, FailAllObjectives, SetStage, SetCurrentStageID, GetStage, GetCurrentStageID, GetStageDone, IsStageDone, SetObjectiveDisplayed, SetObjectiveCompleted, SetObjectiveFailed, IsObjectiveDisplayed, IsObjectiveCompleted, IsObjectiveFailed, GetAlias, GetAliasById, GetAliasByName, GetID, GetPriority, GetQuest | IsStarting, IsStopping, UpdateCurrentInstanceGlobal, ModObjectiveGlobal, SetObjectiveDisplayedNoWait |
| Alias, ReferenceAlias | GetOwningQuest, GetID, GetName, GetReference, GetRef, GetActorReference, GetActorRef, ForceRefTo, ForceRefIfEmpty, Clear | |
| Game | GetPlayer, GetForm, GetFormEx, GetFormFromFile, EnablePlayerControls, DisablePlayerControls, GetGameSettingFloat/Int/String, GetRealHoursPassed | ShakeCamera, AddAchievement, FadeOutGame, RequestSave, ForceFirstPerson, IsFastTravelEnabled and 36 more |
| Debug | Trace, TraceUser, TraceConditional, TraceStack, Notification, MessageBox, SendAnimationEvent | OpenUserLog, CloseUserLog, QuitGame, CenterOnCell, SetGodMode and 14 more |
| Utility | RandomInt, RandomFloat, GetCurrentGameTime, GetCurrentRealTime, GameTimeToString, SetINI\* and GetINI\* (eight) | IsInMenuMode, the frame rate and budget functions (eleven). `Wait`, `WaitMenuMode`, `WaitGameTime` are the VM's own |
| GlobalVariable | GetValue, GetValueInt, SetValue, SetValueInt, Mod | |
| Keyword | GetString | |
| Cell | IsInterior | IsAttached, SetPublic, SetActorOwner, SetFactionOwner, Reset |
| FormList | GetSize, GetAt, HasForm, Find, AddForm | |

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

A new gated test, `the_installed_game_binds_its_vanilla_scripts` in
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
The next step to validate that path, mirroring the fix already applied to the
papyrus crate's gated test, is to open `Skyrim - Misc.bsa` with `soe_bsa` and load
every `scripts/*.pex` straight out of it when no loose folder is present.

## Save and load

`World::save_state(&self) -> Vec<u8>` and `World::load_state(&mut self,
bytes) -> Result<(), VmError>` write and read the run-time half of the form
table, magic `SOEW`, version 1, in the same `Writer`/`Reader` encoding as
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
- No packages, AI, combat, magic, dialogue, scenes, perks. Their natives are
  stubs.
- Quest aliases are filled only from `ALFR`. Conditions, `ALCO`, location and
  reference fill types are not evaluated.
- `SetStage` does not display the log entry or play the quest sound.
- Localized strings come back as `<string N>` until the bsa and strings
  readers are wired in.
- `bind` attaches every script in the load order at start. The game does this
  lazily per cell. On the five base masters this is tens of thousands of
  instances and has not been timed.
- No native has been checked against its CK wiki page yet, see above.
