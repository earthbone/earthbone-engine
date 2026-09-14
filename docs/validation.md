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
