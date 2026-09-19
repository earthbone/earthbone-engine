# Skyrim open engine: handoff

Updated 2026-09-19 against `master` at `75cfb55`. This replaces the September
14 handoff. Its instructions to start the game crate and install Godot were
superseded by subsequent merges.

## Read first

Read `AGENTS.md`, `CLAUDE.md`, [governing intent](docs/intent.md) and
[current status](docs/status.md). The intent preserves the user's goal and
later decisions. The status separates implemented systems from integration
and validation still needed. `ENGINE-PLAN.md` retains the historical rounds.
Read the relevant crate doc before changing its code.

The engine is a Rust reimplementation of Skyrim SE using stock Godot as host.
Every subsystem consumes forms keyed by resolved FormID. Preserve the meaning
of existing content and mods while modernizing runtime and authoring tools.
The DLC are in scope. The GPL core and MIT shim remain separate.

## Repository and merged work

The parent workspace is not a Git repository. The engine's main checkout is
`skyrim-open-engine/` on `master`, with no remote configured at this review.
Reference code and documentation are in `../engine-refs/`, research in
`../research/`, and host/tool binaries in `../tools/`.

Rounds 1 and 2 have first implementations, and the round 3 foundation
branches have merged. Later merges include the save reader, audio and viseme
work, behavior second pass, mod-data parsing/application, determinism hashes,
Jolt host path, character controller, skinning and bind-pose correction.

The most recent feature merges are:

| Branch | Merge | What it adds |
|---|---|---|
| playback | `5b1a9ed` | Host audio support and real-data conversion tests |
| no-load-screen | `35a5dd7` | Simulation prefetch/attach split and deferred unload |
| input | `75cfb55` | Control maps, input driver, cameras and Godot PlayerNode |

Those branches are no longer in flight. At the start of this review there
were no unmerged local branches or extra registered worktrees. The main tree
had untracked `.serena/memories/`, which this task leaves alone. The
documentation update uses its own `docs-status-intent` branch and sibling
worktree. Check Git again before taking ownership of any tree.

## What to pick up next

The nearest proposed milestone is a verified Bannered Mare walkthrough.
The `host-player-run` follow-up now verifies input and camera in Godot 4.4.1
and 4.7.2, and real-cell movement on 4.7.2. It fixes PC control-map selection,
scan-code translation and rebindings, and adds `godot/walkthrough.gd` as a
movement-only launcher. Its activate path only records whether a ray hit. It does not identify
and activate a game reference or connect the doorway lifecycle. SoundBank
receives audio buffers, but nothing in the game loop drives footsteps or
dialogue, and speaker output is unverified.

Run and connect these existing pieces before claiming an interactive demo.
Verify the camera and movement in a real cell, activation and the door event
sequence, deferred unload on a later frame, and audible footsteps. Measure
cold material loading before promising a crossing without a fallback fade.
See the final sections of `docs/godot-host.md` and `docs/game.md`.

The broader gameplay layer still needs a concrete plan: combat, magic,
dialogue, menus and inventory interaction, crafting and progression, quest
conditions and alias filling. The existing future quest-loop paragraph is
not a complete breakdown. The CommonLib shim is a skeleton, and the Creation
Kit replacement is a design. Do not count either as delivered compatibility.

`portal-cells` remains a prototype proposal. The simulation transition split
it depends on has merged, but visible doors still need host integration.
Reuse the local godotwind findings before beginning portal work.

## Verification and safe continuation

This handoff is based on source and history inspection, not a new engine run.
The previous handover recorded 750 passing tier 1 tests before the input
merge. A new full gate is needed before asserting that the current commit is
green. Historical live-game and save-corpus results remain in the crate docs.
Never substitute a skipped gated test for a real-data run.

Run `rtk proxy ./tools/lint.sh` from the engine worktree before a commit.
It includes formatting, three clippy passes, prose checks, TOML and Markdown
checks, fuzz workspace formatting and tier 1 tests. Optional tools that are
missing are reported as skipped by the script. Check its output as well as
its exit status. Live-game and save-corpus tests are separate.

The recorded data path is
`/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data`.
Use `SKYRIM_DATA` for game tests and the save-test variables documented by the
selected tests. The 60 user saves under `../engine-refs/saves/` must remain
outside the repository. Recheck tool and data availability in the current
environment before running anything that depends on them.

One writer per checkout. Create a worktree for a bounded implementation task.
Use RTK for shell commands and Serena for symbol navigation. Consult the local
reference corpus first. The agent reviews each completed step and runs the
gate. Before any commit or merge, show the files changed, what changed and
why, then wait for explicit user approval of that summary, as the latest
global instructions require. Leave changes unstaged while awaiting approval
and commit approved steps. Keep the full gate and the prose rules in
`AGENTS.md` and `CLAUDE.md` aligned. No AI attribution trailers.
