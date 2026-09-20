# Skyrim open engine: handoff

Updated 2026-09-20 for the combined execution foundations. Branch subsequent
work from the consolidated master, preserving the remaining scopes below.

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

The combined baseline includes these source milestones:

| Branch | Source commit | What it adds |
|---|---|---|
| combat-execution | 3488cb2 | Actor-value components and corrected race/NPC/template sources |
| crafting-execution | 4d552ac | COBJ transactions, conditions and inventory events |
| vanilla-save-runtime | da0b0fd | Typed Papyrus execution decoding and round trips |
| vanilla-save-bindings | ba21070 | Explicit-mapping object/array import and variable identity |
| dialogue-execution | f64821d | Dialogue choices, response lifecycle and named fragments |
| magic-effects | a4dd385 | Direct non-hostile applications, effect scripts and expiry |
| host-gameplay | 0873bd2 | Host activation, doors, keys and material-selected footsteps |

Integration resolves shared condition evaluation, declaring-script variables,
actor-value modifier domains and SOEW version 9 persistence. One reload test
retains an active conversation and buff without replaying either. Source
worktrees remain in place. Check their status before taking ownership.

## What to pick up next

Continue Skyrim gameplay parity using [execution-plan.md](docs/execution-plan.md)
and [status.md](docs/status.md). Weapon combat, complete spell execution,
tempering/alchemy/enchanting, dialogue presentation and remaining conditions,
progression and player interfaces remain unfinished. Vanilla-save restoration
still needs world change forms, runtime identity mapping, stacks and latent
continuations. Parser and import tests do not establish gameplay resume.

Host activation and load-door travel are connected. Footsteps reach the mixer
from grounded movement. Remaining host work includes physical speaker checks,
swing-door animation/collision, walking exterior streaming and measured cold
resource preparation. Portal city transitions and core AI redesign remain
deferred until Skyrim itself is playable. The CommonLib shim and editor also
remain unfinished.

## Verification and safe continuation

The combined installed-data game run passes 65 tests, including actor values,
crafting, dialogue, magic, locks and footstep selection. Startup starts 423
quests through 1,490 native calls with zero unresolved calls. Save corpus and
object/array import checks pass all nine selected tests using the 60 external
saves. These checks do not execute saved stacks or restore every world field.
Godot 4.7.2 passes the door round trip, E-key crossing and footstep mixer test.

Historical validation remains in crate docs, labeled by milestone. Never
substitute a skipped gated test for a real-data run. Headless Godot mixer
output is separate from physical speaker and rendered-image verification.

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
