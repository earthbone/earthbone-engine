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

Confirmed by the detector, to fix in round 5:

1. `Game::start` starts the Start Game Enabled quests in the order
   `LoadOrder::winners_of` yields them, and that walks the load order's
   `HashMap` (`crates/esm/src/load_order.rs`, "in no particular order" by its
   own doc line). Two runs start the 423 quests in different orders, so alias
   object ids, `OnInit` thread order, timers, the log and the native call
   counts all differ from the first tick. Sorting the ids before starting,
   which the sorted gated test does from the outside, removes every
   divergence over 600 ticks. The fix belongs in `winners_of` (a sorted
   result) or in `start` (sort before starting), and the ignored test
   `the_installed_game_hashes_the_same_over_six_hundred_ticks` is the
   acceptance check.

Candidates the detector has not caught yet, kept here because the same
pattern is in the code and a longer or wider scenario can reach them:

2. `LoadOrder::winners` and `LoadOrder::overridden` walk the same map.
   `CellIndex::build` reads `winners`, so any consumer that keeps the first
   of two equal candidates in iteration order is order dependent.
3. The world keeps one shared xorshift RNG (`World::random_u64`). Any new
   caller shifts every later draw, so a replay recorded before a native
   gained a random call will not match after. A stream per system or per
   actor, derived from the seed, is the round 5 shape.
4. `NativeRegistry::calls` is a `HashMap` of counters. It is hashed in key
   order here, and the live run summaries sort it before printing. Any new
   reader that walks it unsorted prints a different order per run.
5. The Godot host's threaded builders join in master order, which the
   detector does not cover, the host is outside `Game`.

What the detector cannot see: floating point differences between machines
or compiler versions (the hash compares bit patterns, so a different build
of the same code can hash differently and still behave the same), and
anything the world does not store, such as a native that reads the wall
clock and does not write the result into state.
