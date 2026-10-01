# Validation against the vanilla engine

The question every engine change has to answer is "does Skyrim do this?".
Launching Skyrim per test is not an option, so the oracle is captured once
from the real game and replayed as often as needed. The harness lives in
`crates/game/src/harness.rs`.

## The shape of a check

A scenario is a text file that drives the engine and states what vanilla
shows. One step per line, names are editor ids, `player` is the player:

```text
# The lever quest
expect running MQ01
activate LeverRef by player
expect stage MQ01 == 20
expect objective MQ01 10 completed
expect item player Gold001 == 5
expect notification "Lever pulled"
tick 3
expect traces-match captures/lever/Papyrus.0.log
```

`harness::run` executes it against a `Game` and returns a report with every
failed line. `expect traces-match` compares the engine's `Debug.Trace` output
with a Papyrus log captured in vanilla: every expected line must appear in
order, extra engine lines are allowed. The full grammar is in the module doc.
`Scenario::append_papyrus_log_expectations` adds a parsed capture to a scenario
as one ordered expectation. A mismatch reports every expected trace that was
not found, including later missing lines after an earlier divergence.

## Capturing the oracle in vanilla

One session per scenario, done once:

1. `Skyrim.ini`, `[Papyrus]`: `bEnableLogging=1`, `bEnableTrace=1`,
   `bLoadDebugInformation=1`. The log lands in
   `Documents/My Games/Skyrim Special Edition/Logs/Script/Papyrus.0.log`.
2. Install the scenario's test plugin and scripts (the same synthetic files
   the test writes, exported with `Plugin::to_bytes` and the PEX bytes from
   the assembler) as a loose mod. The plugin must be a master of nothing but
   `Skyrim.esm` so it loads in both engines the same way.
3. Play the scenario: pull the lever, walk through the door, open the chest.
   Save at the end.
4. Copy `Papyrus.0.log` into `crates/game/tests/captures/<scenario>/` next to
   the scenario file. Commit both.

The engine test then loads the same plugin and scripts, runs the scenario,
and diffs. Tracing in vanilla adds a `[date - time]` prefix the harness
strips.

## What the save adds

A vanilla save holds the ground truth the log does not print: every quest's
stage and objective state, every global, every changed reference (position,
enable state, inventory, ownership) and the Papyrus VM's own state (script
instances and their variables). The UESP Save File Format page is local at
`engine-refs/uesp-modfileformat/Save_File_Format.wikitext`. A `crates/save`
reader for the change forms and the globals table would turn a save into
scenario expectations mechanically instead of by hand. That is round 3 work,
the harness already has the checks a save would feed (`stage`, `objective`,
`item`, `enabled`, `cell`, `global`, `actor-value`).

## What "does Skyrim do this" means here

The engine is partly a redesign (`../research/project-intent.md`, Goal).
Vanilla behaviour is the oracle only for what the project chooses to preserve:
the record semantics plugins depend on, the Papyrus surface scripts call, the
observable results mods were written against. Where a better native system
removes a modding workaround (navmesh generation, LOD, the glue tools), the
new system is the spec and vanilla is at most a regression check that mods
built on the old path still work. Each scenario file therefore states which
it is in its first comment line, `# preserve: <what>` or `# redesign: <what>`,
and a redesign scenario cites the design doc it follows instead of a capture.
`docs/design-system.md` lists what the engine preserves.

## Scope of the claim

A passing scenario says the engine matches what vanilla logged and saved for
that flow. It says nothing about timing inside a frame, about the order two
unrelated scripts run in, or about anything the scenario did not touch. Keep
scenarios small and named after one behaviour, and add one per native as its
CK wiki page is reviewed, so the conformance table in `docs/game.md` can cite
the scenario that backs each row.

## Running

Build and run the validation binary with a portable MO2 profile and a directory
of `.scenario` files:

```bash
cargo run -p soe-game --bin soe-validate -- \
  /path/to/Skyrim\ Special\ Edition /path/to/MO2 Default crates/game/tests/scenarios
```

The profile is resolved through `soe-profile` using its normal plugin lists,
enabled mods and data-root order. Scripts, mod data and start-enabled quests
are loaded into a fresh game world before each scenario runs. Scenario files
can live in subdirectories.
Each file's first comment line must say `# preserve: ...` or
`# redesign: ...`. Paths in `expect traces-match` are relative to that scenario
file. The conformance report prints the claim, pass or failure state, and every
capture path cited by the scenario. Exit status is zero when all scenarios
pass, one for scenario failures or runner errors, and two for invalid arguments.

```bash
cargo test -p soe-game
```

The harness self-test builds the lever plugin, runs the scenario above with a
synthetic captured log, and checks that one wrong expectation is reported by
line number. Real captures go under `crates/game/tests/captures/` and are
picked up by a test per directory once the first one exists.

## Determinism

A scenario that replays the same way twice can be bisected, recorded and
compared across machines. The detector for that is a hash of the whole
run-time state after every tick: `Game::state_hash` folds
`World::state_hash` (`crates/game/src/world/hash.rs`) and `Vm::state_hash`
(`crates/papyrus/src/vm/hash.rs`) into one `TickHash` with twenty-three
named parts, seventeen for the world (clock, RNG, object ids, references,
actors, quests, globals, form lists, pending queue, timers, output, AI
counters, and since the world split: the form tables built on first use
(leveled lists, locations, factions, actor bases, cells), settings and the
INI overlay, listener registrations, the player flags and tracked stats,
scenes with sound instances and spell casts) and six for the VM (objects,
instances, threads, latents, log, native call counts). A unit test in
`world/hash.rs` mutates every field the natives round added and asserts that
exactly its part moves, so a field a later change forgets to hash fails
there rather than passing as silent agreement. The hasher is FNV-1a over 64 bits with no per-process seed, maps are
visited in key order and floats go in as bit patterns, so the hash itself
never reports `HashMap` iteration noise. `Game::record_hashes` turns the log
on, `Game::hash_log` reads it, and `determinism::compare` names the first
tick and the parts that differ:

```text
installed game: runs diverge at tick 0 in world.objects, world.refs, ...
```

`crates/game/tests/determinism.rs` runs the quest flow and the travel package
twice in one process and once more in a child process (the test binary
spawns itself with `SOE_HASH_DUMP` set, the child writes its log as text,
the parent compares), and the gated tests run the first real `start` twice
for 600 ticks at 60 a second.

### Findings (2026-09-18)

| Run | Ticks | Result |
|---|---|---|
| Quest flow, twice in one process and across processes | 34 | agree |
| Travel package, twice in one process and across processes | 36 | agree |
| Installed game, `Game::start` then 600 ticks | 601 | diverge at tick 0 in object ids, references, quests, timers, output and every VM part but latents |
| Installed game, quests started in form id order, then 600 ticks | 601 | agree |

### Known sources

Confirmed by the detector and fixed:

1. `Game::start` started the Start Game Enabled quests in the order
   `LoadOrder::winners_of` yielded them, and that walked the load order's
   `HashMap`. Two runs started the 423 quests in different orders, so alias
   object ids, `OnInit` thread order, timers, the log and the native call
   counts all differed from the first tick. Fixed on the `world-split`
   branch: the load order keeps every winner in id order (owning file
   position, then object) in a list built by the first walk and dropped
   when a file is added, and `winners`, `winners_of` and `overridden` all
   read it. The one-time sort is traced as `esm.sort_winners`. The gated
   test `the_installed_game_hashes_the_same_over_six_hundred_ticks` runs
   un-ignored and passes. The `CellIndex` build that reads `winners` got
   faster, the sorted list walks records in file order: `soe-bench
   cell-index WhiterunBanneredMare` median 0.275 s before, 0.059 s after
   (five runs, the first run pays the sort and the median excludes it), and
   the Whiterun 3 by 3 block, whose exterior resolve walks `winners_of`, moved
   from 13 ms to 15 ms on 24 threads, within noise.
2. `LoadOrder::winners` and `LoadOrder::overridden` walked the same map.
   Closed by the same change, every walk is id ordered now.
3. Runtime systems use separate seed-derived xorshift streams through
   `World::random_*_for`. AI, behavior trees, dialogue, mod data, actor
   templates, leveled items and leveled spells each retain their own state.
   A new draw in one of these systems does not shift another system's replay.
   `World::random_*` remains the general Papyrus stream.

Candidates the detector has not caught yet, kept here because the same
pattern is in the code and a longer or wider scenario can reach them:

1. `NativeRegistry::calls` is a `HashMap` of counters. It is hashed in key
   order here, and the live run summaries sort it before printing. Any new
   reader that walks it unsorted prints a different order per run.
2. The Godot host's threaded builders join in master order, which the
   detector does not cover, the host is outside `Game`.

`TickHash::with_host_builder_hash` lets a host caller add its builder result
hash to each game tick as `host.builders`. The
`tst_02_host_runs_hash_threaded_builder_results_per_tick` test runs the same
seeded worker batches twice, compares combined hashes for each tick, and checks
that a changed host hash names that part as the divergence.

What the detector cannot see: floating point differences between machines
or compiler versions (the hash compares bit patterns, so a different build
of the same code can hash differently and still behave the same), and
anything the world does not store, such as a native that reads the wall
clock and does not write the result into state.

## Debug commands (2026-09-18, branch `debug-commands`)

Some behaviour cannot be checked against the installed game because the state
it needs is unreachable from a cold `Game::start`, which runs the quests
flagged Start Game Enabled and stops. The Dawnguard vampire turn is the
example: it needs dialogue and a player actor that start never produces, so
its check had to build a synthetic plugin. Skyrim answers the same problem
with its console, and `crates/game/src/debug.rs` is that command layer
without a UI. A scenario reaches it through the `debug` step, a Rust test
through `Console::run`.

Every command is an ordinary operation on `World` and the VM. The ones that
run a Papyrus function assemble a one-function probe script and execute a
real `CallMethod`, because `Vm::call` finds only functions a loaded script
defines and natives are dispatched by the interpreter's own opcode handler.
So the instruction under test is the one a vanilla script compiles to, and no
command reaches a state the engine could not reach on its own.

### Moved from synthetic to live

`crates/game/tests/debug_live.rs`, gated on `SKYRIM_DATA`.

| Check | Was | Is now |
|---|---|---|
| Vampirism state change | A synthetic actor with a watcher script | `player.setvampire` on the installed game, plus the werewolf sender |
| A quest advancing through its stages | A synthetic quest with six stages | A dormant vanilla quest found in the load order, `startquest` then every stage it defines (`dunEldergleamT03`, 6 of 6 set) |
| Package evaluation | A synthetic NPC with a travel package | `pridbase Hulda` then `evp`, which resolves package `0:018959` |

`Hulda` names an actor base, not a placed reference, and a vanilla `ACHR`
usually carries no editor id of its own. `pridbase` walks from the base to
the reference that places it, which is what makes an actor selectable by
name at all.

### What is still synthetic, and why

The event delivery half of the vampirism check. Three loaded scripts define
`OnVampirismStateChanged` (`ActiveMagicEffect`, `Actor`, `ReferenceAlias`)
and all three are base classes declaring the signature, not subscribers. No
bound actor reference carries a script that handles it, so on a cold start
the event has nowhere to land and `player.setvampire true` adds no log line.
The live test proves the native runs and changes state over vanilla records.
That the event reaches a listening script stays proved by the synthetic
scenario in `natives_dlc.rs`, which attaches one. Both halves are needed and
neither replaces the other.

### A claim to check against a real save, not verified behaviour

The user's saves under `engine-refs/saves/` cover the Dawnguard vampire path,
and 23 and 24 bracket the moment the player becomes a vampire lord. Once
`crates/save` can read them, diff the list below against what actually
changed between those two saves. Everything here is what the engine does
today, stated so the gap can be found, not a claim that it matches.

`setvampire true` on a target does exactly this and nothing else:

- Runs `Actor.SendVampirismStateChanged(true)` through a real `CallMethod`.
- The native sets one field, `ActorState.vampire`.
- The native fires one event, `OnVampirismStateChanged(abIsVampire)`, to the
  scripts attached to that actor.

It does not touch race, spells, perks, keywords, actor values, eye texture,
the sun damage effect, the vampire feed state, any faction, or any global.
In vanilla none of those is the native's job either: the native announces,
and the Dawnguard quest scripts do the rest. So the honest reading of a diff
against saves 23 and 24 is a question about those scripts, not about this
command. What to look for:

1. Which of the changes between 23 and 24 the vanilla scripts make, and
   whether those scripts run here when the event is delivered.
2. Which changes the real engine makes outside Papyrus, since anything in
   that group needs engine work rather than a script.
3. Whether `ActorState.vampire` corresponds to anything the save records, or
   whether vanilla keeps the state only in a quest variable and a race.

Until that diff exists, treat `setvampire` as "the announcement fires and one
flag moves", which is what the tests assert, and nothing more.

## Script and quest coverage

After loading scripts, call `game.vm.start_coverage()`, then start the game
and run the scenario. `harness::run` includes `Report::coverage` when recording
is enabled. `Game::coverage_report` can also snapshot the current interval.
Its `to_text` method writes sorted per-script and per-quest percentages plus
executed function identities. The structured report also carries every
unexecuted body.

Per-script coverage uses all loaded non-native bodies. Per-quest coverage
uses the winning QUST record's active VMAD attachments and alias attachments,
including inherited definitions, with duplicate body identities counted once.
Deleted script attachments are excluded. A hit counts for that quest only if
its self object is the quest or one of its aliases. Shared scripts executed
on another quest do not count. Unowned global functions remain in per-script
coverage. External reference scripts and transitive helper calls are not
assigned to a quest just because that quest triggered them.

Missing PEX files and invalid VMAD records are reported explicitly. The
percentage describes known loaded bodies, so missing attachments make the
scope incomplete. A zero denominator displays `n/a`, never 100 percent.
Function entry is not proof of quest completion or behavioral compatibility.

The installed-game test enables coverage before binding and startup, then
runs a 0.1 second tick through the scenario harness. To save its report:

```sh
rtk proxy env SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" SOE_COVERAGE_REPORT=/tmp/coverage.txt cargo test -p soe-game --release --test real_files -- --nocapture
```

The first five-master survey recorded 323 of 54,023 non-native bodies across
14,026 loaded scripts and reported all 2,274 winning quests. These are startup
coverage results, not completion targets. Synthetic tests prove shared-script
isolation, alias and inherited-body attribution, deleted-attachment filtering,
missing scripts and malformed VMAD diagnostics. The VM tests verify that
coverage does not change save bytes or simulation hashes.
