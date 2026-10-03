# soe-probe: original runtime observations

`crates/probe` is a GPL-3.0-or-later tooling crate and workspace member. Its
`soe-probe` binary builds a vanilla quest plugin and PEX runner, reads Papyrus
logs and executes the same generated PEX through `soe-game` for comparison.
Transport stays in this crate. It adds no engine save fields.

## Build and capture

From the repository root:

```text
rtk cargo run -p soe-probe -- build ./probe-out
rtk cargo run -p soe-probe -- build ./probe-out crates/probe/probes/quest_wait.toml
```

The optional third argument selects an authored sequence. Without it, build
uses the bundled `quest_wait` sequence. Build writes:

- `probe-out/SoeProbe.esp`
- `probe-out/Scripts/SoeProbeRunner.pex`
- `probe-out/Seq/SoeProbe.seq`

The ESP has a TES4 header with five masters in order: Skyrim.esm, Update.esm,
Dawnguard.esm, HearthFires.esm and Dragonborn.esm. Its only record is the
start-game-enabled, run-once `SoeProbe` quest, local object ID `000800`.
VMAD attaches `SoeProbeRunner`, which extends Quest. The quest has stage zero
and each stage the sequence sets on `runner`. It has no objectives or aliases.
SEQ contains its file-relative FormID, `05000800`, as a little-endian word.
The TES4 record count includes the quest and its top-level group.

The runner uses Skyrim PEX 3.2, game ID 1 and big-endian encoding, a deduplicated
string table, per-instruction debug lines and the hidden/conditional flag
table. The object is Hidden. State helpers come from the existing bounded
PSC compiler. PEX bytes come from `soe_papyrus::asm::write`.

## Install in Steam and Proton

Close Skyrim before installing. The user's installed Data directory is:

```text
/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data
```

1. Copy `probe-out/SoeProbe.esp` to `Data/SoeProbe.esp`.
2. Copy `probe-out/Scripts/SoeProbeRunner.pex` to
   `Data/Scripts/SoeProbeRunner.pex`. Create `Scripts` if absent.
3. Copy `probe-out/Seq/SoeProbe.seq` to `Data/Seq/SoeProbe.seq`.
   Create `Seq` if absent.
4. In the Proton prefix for Steam app 489830, edit
   `drive_c/users/steamuser/AppData/Local/Skyrim Special Edition/plugins.txt`.
   Preserve its existing lines and append `*SoeProbe.esp` after the enabled
   official content. The usual prefix location is
   `steamapps/compatdata/489830/pfx` in the Steam library.
5. In that prefix's
   `drive_c/users/steamuser/Documents/My Games/Skyrim Special Edition/Skyrim.ini`,
   add or update these entries in the existing Papyrus section:

   ```ini
   [Papyrus]
   bEnableLogging=1
   bEnableTrace=1
   ```

6. Launch Skyrim normally through Steam and Proton. Start a disposable new
   game with the plugin enabled. OnInit starts the sequence once. For the
   example, leave gameplay running with menus closed until both one-second
   waits finish. Search the log for `read_stage`, phase `end`, result stage 20.
   The example changes only its own quest. A sequence that names other quests,
   actors or spells executes its authored mutations on those forms.
7. Close Skyrim to flush the log. Copy or read
   `Documents/My Games/Skyrim Special Edition/Logs/Script/Papyrus.0.log` from
   that prefix. Use one run per input file. Restarting the game rotates logs.

The bundled load order includes the two free CC ESMs, Fish and AdvDSGS, plus
SurvivalMode, Curios and the installed ResourcePack ESL. Before building, make
`load_order` match the enabled profile order. Every listed plugin needs a form
anchor. The bundled anchors are existing local IDs from the installed masters.
Runtime FormIDs reveal their actual regular or light slots. Vanilla Papyrus
cannot enumerate all plugins, so the names in `load_order` are the declared
profile, while `forms` contains measured identities. An undeclared plugin
without an effect on the recorded slots cannot be discovered by this runner.

To repeat, start another disposable new game from before this plugin was
present. The runner's `started` variable prevents overlapping or repeated
execution in the same quest instance. For removal, close Skyrim, remove
`*SoeProbe.esp` from plugins.txt and delete the three installed probe files.
Restore the prior logging settings if desired. Return to a save made before
installing the probe. The build tool does not install, launch or remove files.

## Parse and compare

```text
rtk cargo run -p soe-probe -- parse /path/to/Papyrus.0.log > observations.json
rtk cargo run -p soe-probe -- compare crates/probe/probes/quest_wait.toml observations.json
```

`compare` requires `SKYRIM_DATA` to name the installed Data directory. It also
accepts the Papyrus log directly as its final argument. It loads exactly the
authored plugin list, generates and appends SoeProbe.esp, loads the runner,
binds VMAD and starts only that quest through `Game`. It uses the existing
natives and scheduler, without loading unrelated vanilla scripts or starting
the other start-game-enabled quests. Initial TimeScale is set to the authored
value in the shared world. No game assets or saves are written by comparison.

Exit status zero means a matching comparison. Status two means differences,
with one JSON entry per changed field, request boundary and declared tolerance.
Status one reports authoring, input, I/O or execution failure. Missing, extra
and reordered boundaries fail comparison. The parser rejects malformed marker
lines, duplicate boundaries, unknown fields and logs with no observations.
Unrelated Papyrus lines are ignored. Version, sequence, pause and numeric
context are validated even for structured JSON input.

## Authored sequence

The TOML format requires `name`, `load_order`, `timescale`, `tick_seconds`,
`timeout_seconds`, `[tolerances]` and at least one `[[steps]]`. Names and request
IDs are ASCII tokens. IDs are unique within a sequence. Up to 64 steps, 32
authored forms and 128 menu changes are accepted. Invalid numbers, missing or
mistyped form aliases, unknown properties and unsupported commands are errors.

`runner` and `timescale` are reserved form aliases. Other `[forms.<alias>]`
tables specify `plugin`, `object` and `kind`. `object` is the local object ID,
without a load-order prefix, written as a decimal or hexadecimal TOML integer.
The kinds are `quest`, `actor`, `player`, `object_reference`, `spell`, `magic_effect`,
`global_variable` and `form`. The runner resolves them with
`Game.GetFormFromFile`, casts to the declared Papyrus type and stops on a
missing form. Engine execution checks the record types before starting.
The `player` kind requires Skyrim.esm object `000014`. It resolves through
Game.GetPlayer in both runtimes and has Papyrus type Actor. It is a runtime
reference, so the engine preflight uses World.player without requiring a
placed ACHR record. Other kinds still resolve through Game.GetFormFromFile.

| Operation | Additional step fields | Result |
| --- | --- | --- |
| `set_stage` | `quest`, `stage` | `accepted` and highest completed `stage` |
| `reset_quest` | `quest` | `stage` after Reset |
| `quest_stage` | `quest` | Highest completed `stage` |
| `wait_real` | `seconds` | `issued` after Utility.Wait |
| `wait_menu` | `seconds` | `issued` after Utility.WaitMenuMode |
| `wait_game` | `seconds` | `issued` after Utility.WaitGameTime, seconds converted to hours |
| `animation` | `actor`, `event` | `issued` after Debug.SendAnimationEvent |
| `animation_events` | `actor`, `events` | `issued` after 1 to 16 ordered Debug.SendAnimationEvent calls |
| `animation_int` | `actor`, `variable`, optional `set` integer | `value` from GetAnimationVariableInt, after SetAnimationVariableInt when supplied |
| `animation_float` | `actor`, `variable`, optional finite `set` float | `value` from GetAnimationVariableFloat, after SetAnimationVariableFloat when supplied |
| `equip_item` | `actor`, `item` | `issued` after silent AddItem of one copy and EquipItem |
| `third_person` | None | `issued` after Game.ForceThirdPerson |
| `wait_animation` | `actor`, `event` | Boolean `value` from ObjectReference.WaitForAnimationEvent |
| `cast` | `spell`, `source`, `target` | `issued` after Spell.Cast |
| `actor_value` | `actor`, `value` | Numeric `value` from Actor.GetActorValue |
| `has_effect` | `actor`, `effect` | Boolean `value` from Actor.HasMagicEffect |
| `form_property` | `form`, `property` | `value` for `type`, `form_id` or `global_value` |
| `save` | None | `issued` after Game.RequestSave |

Every step also requires `id` and `op`. `issued` records return from a native
that supplies no success value. It does not establish save completion,
animation acceptance or spell application. Follow casts with `has_effect` or
actor-value queries to measure application. `wait_animation` needs a loaded
animation graph and an external event. The headless comparator has no host
animator and reports its timeout if the event never arrives. Timeout bounds
engine execution, not the vanilla latent native.

Optional `startup_wait_seconds` defaults to zero. A positive value calls
Utility.Wait after form resolution, before sampling the sequence's starting
clocks or executing its steps. It must be finite, nonnegative and less than
`timeout_seconds`. Event batches preserve native-call order in one step, but
do not force the calls into a single vanilla animation update.
An event batch can also provide `set_int`, a map of up to eight graph-variable
names to signed integer values. The runner writes those variables in sorted
name order before sending any events, without an intervening observation.

Waits emit a `begin` sample with an empty result and an `end` sample on return.
Ordinary waits exclude menu time. Menu waits include it. Game waits use game
hours and follow TimeScale. Repetition uses separate request IDs, as in the two
waits in the bundled example.

Optional `[[menus]]` entries have `at` and `paused`, in increasing real seconds
from sequence start. They drive the headless host's menu state. In Skyrim,
the user supplies the corresponding menu openings and closings. The runner
reports the observed state at each boundary, rather than forcing a menu.
Declare clock tolerances before collecting traces. Pause flags remain exact.

## Observation fields

Each trace line begins `SOEPROBE ` followed by one JSON object.

| Field | Meaning and comparison |
| --- | --- |
| `schema` | Protocol version 1, exact |
| `sequence` | Authored name, exact |
| `request_id` | Step ID, exact |
| `phase` | `begin` or `end`, exact and in authored order |
| `runtime_version` | Debug.GetVersionNumber in Skyrim, soe-game crate version in the comparator |
| `skse_version` | Null for this vanilla transport, exact |
| `load_order` | Declared enabled profile names in order, exact |
| `forms` | Alias to measured signed Papyrus Form.GetFormID integer, exact |
| `timescale` | Current TimeScale global, absolute tolerance `tolerances.timescale` |
| `real_seconds` | Utility.GetCurrentRealTime minus its sequence-start sample |
| `game_seconds` | Utility.GetCurrentGameTime minus its start sample, multiplied by 86400 |
| `paused` | Utility.IsInMenuMode, encoded as zero or one, exact |
| `result` | Numeric fields for the command, each exact unless a tolerance is declared |

Negative FormIDs preserve Papyrus's signed integer representation, including
FE light-plugin slots. Interpret them as unsigned 32-bit words for display.
All numeric tolerances are absolute and nonnegative. Clock tolerances are
`real_seconds` and `game_seconds`. `[tolerances.result]` can assign tolerances
to `accepted`, `stage`, `issued` or `value`. Missing result keys still fail.
The report retains both runtime versions as provenance. Versions must remain
constant within each run. Comparing Skyrim's version string to the engine's
version string does not measure gameplay behavior.

The engine currently advances World.clock.real_seconds outside menus, while
the VM's WaitMenuMode clock includes menus. The bridge preserves that native
readback. Menu traces can therefore expose a clock disagreement with vanilla.
The example has no menus, animation or combat. It starts with the generated
quest at stage zero and TimeScale 20. Other live state is not imported from an
ESS file by this tool.

`compare <probe.toml> <Papyrus.0.log> <expected.json>` reads explicit engine
observations instead of running the Game comparator. This path does not need
`SKYRIM_DATA`. It uses the same boundary, context and tolerance validation and
retains the engine version from the expectation file. An expectation file is
an engine result, not evidence of a Skyrim capture.
The comparator checks each trace's declared load order, form aliases, local
object IDs and null SKSE version against the authored sequence, even when the
two input traces agree. Regular or light runtime prefixes remain measured
values compared between the engine and capture.

`crates/probe/probes/anm_56.toml` equips the player with a vanilla iron sword,
forces third person, draws the sword, sets `weaponSpeedMult` to zero for
attack-window observations and
reads `i1HMState` around labelled trigger entry and exit, initiate entry,
same-event bounds, pending-request reset and a changed `iBlockState` guard.
The sequence declares the five official masters, Fish and AdvDSGS in their
profile order. Its forms come only from those seven ESMs and its own runner.
The profile's official light plugins do not affect these regular FormIDs and
are not probed. The closed initiate-window case reads back after 1, 5, 30 and 60 seconds.
Build this sequence with the usual optional sequence argument. Start a
disposable game with `coc qasmoke` from the main-menu console,
leave the left hand empty and keep menus closed without combat or input for
about 75 seconds, until `finished_state` is logged. The runner waits three
gameplay seconds before starting its recorded clock, and restores attack speed
to one at the end. Compare once with `rtk cargo run -p soe-probe -- compare
crates/probe/probes/anm_56.toml /path/to/Papyrus.0.log
crates/probe/probes/anm_56.expected.json`. The expectation file comes from the
HKX test's isolated vanilla `1HM_Behavior` replay, with no clip library or
master-graph equipment simulation. It uses `Interpreter::current_state` for
the state readback. The capture compares Papyrus's graph variable instead.
Finite fixed-time boundaries and graph-unload reset are not exercised by this
sequence. No original capture has been compared, so ANM-56 remains partial.

## Sources and validation

Read before implementation:

- `docs/reference-audit.md`, Original-runtime probes.
- tes4skyrim revision `29777351936fb5253f6cb2d1b14d11a71dd17aaf`,
  `game_bridge/README.md`, `game_bridge/docs/protocol.md` and
  `tools/live/game_bridge.py`. The requested clone was absent from the local
  corpus, so these files were read from the pinned upstream revision.
  Request/result pairing and explicit failure informed this file protocol.
  Its SKSE DLL and Windows named pipes are not runtime dependencies.
- Local `Champollion/Pex/FileReader.cpp`, `Caprica/Caprica/pex/PexFile.cpp`,
  `PexWriter.h` and `PexDebugInfo.cpp`. Caprica's Skyrim branch confirms the
  existing writer's debug layout, without the newer-game property groups.
- Local `TES5Edit/Core/wbDefinitionsTES5.pas` QUST and
  `TES5Edit/xEdit/xeMainForm.pas` GenerateSEQFileForFile, plus UESP
  `QUST.wikitext` and `TES4.wikitext` for record ordering and header counts.
- Local CK `Debug_Script.wikitext`, `Utility_Script.wikitext` and the shipped
  vanilla Quest, Utility, Game and Spell PSC declarations for native
  signatures, highest-stage queries and wait units.
- ANM-56 also reads CK `EquipItem_-_Actor.wikitext`, the integer and float
  `GetAnimationVariable` pages and `SetAnimationVariableInt` page, plus local
  `skse64/scripts/vanilla/ObjectReference.psc` for the float setter and AddItem
  signatures. These are vanilla declarations, without an SKSE dependency.
- CK `Game_Script.wikitext` and `skse64/scripts/vanilla/Game.psc` declare
  Game.GetPlayer and ForceThirdPerson. The existing `World.player` path supplies
  the engine's reserved runtime player reference.
- Existing `crates/game/tests/quest_flow.rs`, game quest/VMAD builders and
  `crates/papyrus/src/asm.rs`. These already provide the Rust writers needed
  here. No replacement binary-format writer or upstream code copy was added.

The new direct format dependency is `toml` with parsing only. Its added
packages are toml, toml_edit, toml_datetime, serde_spanned and winnow. Serde,
serde_json and thiserror were already in the workspace lockfile. `cargo tree
-p soe-probe --depth 2` also shows the existing soe-game dependency on
Wasmtime/WASI. The bridge adds no separate runtime or native transport library.

`tests/tool_04.rs` covers plugin/PEX/SEQ output, VM execution, strict parsing,
every comparison field, numeric tolerance boundaries, repeated waits, menu
clock domains, form validation, timeouts and CLI exit statuses. The
SKYRIM_DATA-gated case executes the same sequence against the installed
official records and checks CLI matching and differing comparisons.
`tests/fixtures/Papyrus.0.log` is an authored synthetic trace, not a vanilla
capture. Its profile and FormIDs follow the synthetic plugin fixture and its
small scheduling delays fit the tolerances declared before the tests run.
No live Skyrim probe has been captured by these tests.

## Transition tie capture, ANM-54

`crates/probe/probes/anm_54.toml` observes the player's vanilla third-person
`1HM_Behavior` through `i1HMState`. It labels the local/wildcard
`streamingQuery_attackStart` tie, equal-priority `blockStart` interrupted by
`recoilStart`, and both orders of delayed `blockStop` and `attackStart`.
Each delayed batch writes `iBlockState = 1`, closes `InitiateWinEnd`, issues
both requests, samples the pending state and opens `InitiateWinBegin`.
Build and install with the commands above, selecting this sequence. From the
main-menu console use `coc qasmoke` for a disposable default male player,
without equipping items. Keep gameplay unpaused and leave the controls alone.
The runner waits five seconds, selects third person and draws the player's
hands. Allow roughly 10 to 15 seconds and wait for `delayed_last_stop_result`
before closing Skyrim to flush the log. The sequence names only the five
masters, Fish, AdvDSGS and its generated quest. The official light plugins in
`quest_wait.toml` have no aliases or mutations in this sequence. The checked
`anm_54.expected.json` contains engine predictions, not a vanilla capture.

```text
rtk cargo run -p soe-probe -- build ./probe-out crates/probe/probes/anm_54.toml
rtk cargo run -p soe-probe -- compare crates/probe/probes/anm_54.toml /path/to/Papyrus.0.log crates/probe/probes/anm_54.expected.json
rtk cargo run -p soe-probe -- replay crates/probe/probes/anm_54.toml
```

An optional final `expected.json` argument makes `compare` read reviewed
observations instead of executing the sequence. That mode needs no
`SKYRIM_DATA`. `replay` writes the engine observations as JSON and requires
installed data. Missing, extra and reordered boundaries still fail, as do
state differences. The comparator checks the authored load order, alias names
and local object IDs even when the two supplied traces agree with each other.
`animation_batch` accepts 1 to 16 ordered event tokens
and an optional `ints` table of at most eight integer variable setters. It
runs setters before events without trace calls or waits between them.
`animation_int` reads `GetAnimationVariableInt`, `set_animation_int` writes
`SetAnimationVariableInt`, `third_person` calls `Game.ForceThirdPerson` and
`draw_weapon` calls `Actor.DrawWeapon`. A form of kind `player` must name
Skyrim.esm object `0x14` and resolves through `Game.GetPlayer`.

The optional `[animation_graph]` table selects an actor alias and an archive
HKX path for the comparison host. It loads the graph, referenced behaviors,
readable clips and the actor skeleton through the existing BSA/HKX readers.
An absent clip fails if activated. Relative shared-clip paths resolve parent
components within the archive. The host forwards ordered script events,
reads changed script variables and publishes graph variables to the same
world reference maps read by Papyrus. Graph reports are delivered to listeners
without reinjection as script requests. This replay selects the unmodified
`1hm_behavior.hkx` graph as its root. It does not reproduce the surrounding
master graph, character control inputs or physical contacts from Skyrim.
The capture comparison includes ready, pending and reset samples to expose
state or timing disagreement. It does not infer success from event issuance.

Sources are the local CK `GetAnimationVariableInt`, `SetAnimationVariableInt`,
`DrawWeapon` and `Game_Script` pages, the serde-hkx state-machine/transition
class descriptions and the Havok state-machine declaration already linked in
[hkx.md](hkx.md). No executable disassembly or Havok implementation was used.
The probe adds the existing `soe-hkx` and `soe-bsa` workspace dependencies,
with no new external runtime dependency. `hkx/tests/anm_54.rs` covers provisional
ties, state-ID output and an installed replay of the checked predictions.
Its `SOE_PROBE_CAPTURE` case reads `Papyrus.0.log` from the named directory and
compares it against the installed HKX replay. It skips without capture data.
ANM-54 remains partial until the user's original-runtime capture is compared.
