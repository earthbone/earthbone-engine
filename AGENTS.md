# AGENTS.md

Handover for an agent with no prior context on this repository. Read this
first, in full. It is short on purpose. Everything it points at is real and
worth opening before you write code.

Read `docs/intent.md` for the full governing intent and `docs/status.md` for
the current integration status on 2026-09-20. The intent combines
the user's original goals with later decisions, including the compatibility
boundary, modernization and editor scope. It is not a feature-completion list.

## What this is

`skyrim-open-engine` is an open-source reimplementation of the Skyrim Special
Edition runtime in Rust, whose purpose is to keep the existing mod ecosystem
working. It is not a new game and not a Skyrim-like. A plugin built for the
Creation Engine, an xEdit output, a Synthesis patcher result: all of them must
keep their meaning here.

One design rule holds everywhere and explains most of the code:

> The engine is a record database with an interpreter on top. Every system
> reads forms keyed by resolved FormID from the load order, never from
> engine-private structs.

When a change would be easier if some system kept its own parallel copy of game
state, that is the rule telling you no. Read the form.

Godot is the host, used unpatched, through a GDExtension. Bulk rendering and
host physics use server resource IDs rather than a node per placed reference.
Positional audio uses `AudioStreamPlayer3D`, since Godot has no equivalent
server-level path. The host also has control nodes such as PlayerNode. Those
do not change the bulk-world rule. Report any new per-reference node path and
its measured cost.

Licensing matters and is not uniform. The core crates are GPL-3.0-or-later, so
reading and porting from a GPL tool is fine there. The `commonlib-shim` crates
are MIT and no GPL-derived code may ever reach them.

## The rule that gets broken most

**Look locally first.** `../engine-refs/` holds sixty cloned repositories and
fetched documentation corpora: xEdit, Mutagen, CommonLib, OpenMW, nifly,
FallrimTools, serde-hkx, the UESP and Creation Kit wiki page dumps, and a
Morrowind reimplementation in Godot called `godotwind` that has already solved
several problems this project faces.

On 2026-09-18 alone, five separate agents started deriving from raw bytes
something a cloned repository declared outright, and two went searching online
for repositories already sitting on disk. Every one of those cost hours and
produced a worse answer than reading would have.

So: before writing any parser, format reader, layout, or algorithm, grep
`../engine-refs/` for the format name, the class name, the file extension, or
the tool that writes it. If something answers the question, read it, follow it,
and credit it in the crate's doc. If you still write your own, write down in
the doc what you read and why it did not fit.

The four sources that answer most questions:

- `TES5Edit/Core/wbDefinitionsTES5.pas` for record and subrecord layouts.
- `uesp-modfileformat/*.wikitext` for the same layouts in prose.
- `ck-wiki/*.wikitext` for Papyrus native semantics.
- `Mutagen/docs` for winning-override rules.

## Layout

The workspace root is `..` relative to this file and is not itself a git
repository. This engine is at `skyrim-open-engine/`, on `master`. Reference
clones are at `../engine-refs/`, research notes at `../research/`, and a Rust
mod manager sharing the house style at `../linux-mod-organizer/`.

| Crate | What it is |
|---|---|
| `esm` | Load-order aware plugin reader, winning-override form table |
| `bsa` | BSA and BA2 archives plus a loose-file overlay by load order |
| `nif` | Skyrim SE meshes into a renderer-neutral struct |
| `papyrus` | PEX loader, assembler, VM with scheduler and latent calls |
| `hkx` | hkx packfiles, skeletons, spline clips, behavior graph model |
| `dds` | DDS container, BC1 to BC7 decode, encode behind a feature |
| `physics` | bhk shapes, headless static world, capsule character |
| `navmesh` | Vanilla NAVM graph plus Recast generation |
| `profile` | plugins.txt, INI layering, MO2 profile layout |
| `commonlib-shim` | CommonLib header scope, MIT Rust skeleton |
| `godot-host` | GDExtension: cell streaming, mesh and terrain builders |
| `game` | Runtime form table, Papyrus object model, natives, clock |
| `save` | Vanilla `.ess` reader, change forms, quest stages |
| `audio` | FUZ, LIP and xWMA containers, Vorbis and Opus decode |
| `moddata` | SPID, KID, Base Object Swapper ini files, OAR folders |

Each crate has a doc under `docs/` with the same name. Read the doc for a crate
before changing it. `docs/status.md` says what works today, per subsystem.
`ENGINE-PLAN.md` carries the rounds, the per-item done checks, and the reasoning
behind decisions already taken. `CLAUDE.md` is the same orientation as this
file written for a different agent, so the two overlap and neither is stale on
purpose. If you change a working rule, change both.

## The gate

Run `./tools/lint.sh` from the repository root before every commit. It is the
only definition of done that counts. In order, it runs:

```text
cargo fmt --all --check
cargo clippy --workspace --lib --bins -- -D warnings
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo clippy --workspace --no-default-features --all-targets -- -D warnings
comment-lint crates docs
tools/prose-lint.sh
taplo fmt --check
markdownlint-cli2
fuzz workspace formatting
cargo test --workspace
```

The three clippy runs are three runs for a reason. Code behind an
off-by-default feature escapes the normal pass and rots, which is how the `dds`
encode path drifted until someone noticed. Tests are checked only by the
`--all-targets` runs, and on 2026-09-18 thirteen clippy errors reached `master`
because an agent ran a gate without them.

The reference-correction gate reports 835 test passes, including gated tests
that return early without their data variables. Tier 1 must stay under the
five minute budget.
Optional lint tools may be skipped when absent. Report those skips as well
as exit status. The combined branch requires its own gate run.

Tests that need the installed game are gated on an environment variable and
skip cleanly when it is unset:

```text
SKYRIM_DATA="/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data"
```

Sixty real Special Edition saves sit under `../engine-refs/saves/`, reached
through `SOE_TEST_SAVE`. They are the user's own data. Read them, never commit
one, never copy one into this repository.

## How work is organised

One plan item, one git worktree, one branch, one report.

For GPT agents, explain each action before taking it, including edits,
checks and commits (user instruction, 2026-09-19).

For an interactive walkthrough, retain the launch tool's process session ID
and poll that session to detect closure and collect its exit status. Confirm
exit before replacing the loaded extension. A normal shutdown also prints
`WALKTHROUGH stopped`, but the process result is authoritative, including on
crashes. Leave user test windows open without a timeout. Do not rely on broad
process-name searches or ask the user about closure when the tracked session
already reports it.

One writer per checkout. Preserve unrelated changes. The agent reviews each
completed step and runs the gate. Before any commit or merge, show a short
diff summary naming the files, changes and reasons, then wait for explicit
user approval of that summary. The latest global instructions require this
approval even when the agent is the reviewer. Leave changes unstaged while
awaiting approval. Commit approved steps and keep other writers isolated.
Prefix every shell command with `rtk`. Use Serena and language servers for
symbols, direct reads and RTK searches for named files and literal text.

```text
git worktree add ../soe-<item> -b <item> master
```

Touch only the crates the item names, plus their docs. If the item genuinely
needs a line in another crate, keep it minimal and say so in the report. Commit
reviewed milestones incrementally, because a long-running agent that
loses its session loses everything uncommitted. Run the gate before the final
commit. Do not merge your own branch and do not remove your worktree, the lead
does both.

Report: branch, commit hashes, files touched, test counts, the gate result per
step, what you read in `engine-refs` and what you took from it, and what is
untested or out of scope. Be specific about the last one. A report that hides a
gap costs more than the gap.

There is no git identity configured in this repository. Set
`GIT_AUTHOR_NAME`, `GIT_AUTHOR_EMAIL`, `GIT_COMMITTER_NAME` and
`GIT_COMMITTER_EMAIL` on every commit. Never add an AI-attribution trailer to a
commit, a pull request, or any signed prose.

## House style, enforced by the gate

`tools/prose-lint.sh` checks this and will fail your commit.

- No em dashes and no en dashes anywhere. Use a comma, a period, or restructure.
  Spell ranges out, "3 to 5".
- No semicolons as prose separators. Real code syntax is fine.
- No AI vocabulary: delve, tapestry, leverage, robust, seamless, comprehensive,
  "it is important to note", "in conclusion".
- No redundant emphasis: "by design", "deliberately", "not an accident" add
  nothing when the sentence already states the fact.
- `comment-lint` enforces the mechanical rules on code comments. Hold the same
  bar in Markdown and commit messages.

`docs/style.md` is the Rust baseline.

## How this project decides things

Measure before claiming. Several beliefs on this project turned out to be
wrong under measurement, and the measurements are why the design is what it is:

- A reported 56 ms frame time was a misread of `Performance.TIME_PROCESS`. The
  real p99 is 0.519 ms on a Whiterun 3 by 3 block, and that number is the
  reason Godot was kept.
- A door crossing was assumed to cost microseconds once the destination was
  prefetched. Prefetch moved the load off the crossing but left the unload on
  it, and the unload was the expensive half at 464 microseconds. Deferring it
  is what actually produced the 8 microsecond crossing.
- Skin partition bone lists were assumed to be the identity. Over 400 actor
  meshes, 564 of 846 partitions reorder.

Write the threshold into the test before its first run, not after seeing the
number. Prefer a survey over a single file: a test asserting that one voice
line is audible failed on a creature-dialogue placeholder that is silent in
vanilla, and the fix was to sample sixty and assert that most carry signal.

Do not reimplement what already has tooling. When you must write your own, name
in the doc what you read and why it did not fit. Dependency scope is bounded:
prefer a small maintained crate, keep a heavy one such as ffmpeg behind a cargo
feature or an out-of-process binary, run `cargo tree` on anything new and
report what it drags in.

Where Godot already ships a subsystem, use Godot's rather than linking a second
one. That is why physics runs on Godot's bundled Jolt and audio goes through
`AudioServer` instead of `cpal` or `rodio`.

## Where the project stands

The 2026-09-20 integration combines actor-value components and corrected
race/template sources, crafting transactions and inventory events, dialogue
selection and fragments, direct non-hostile magic, host doors and footsteps,
and vanilla-save execution parsing and object/array import groundwork.
The native registry has 361 stateful entries and 180 stubs. SOEW version 9
combines actor-value components, dialogue and active effects. SOEV version 3
retains declaring-script variable identity.

PlayerNode now drives Game/VM activation, locked-door key checks and the
load-door lifecycle with rendering/collision replacement. Grounded movement
selects footstep sounds from records and reaches the Godot mixer. Physical
speaker output, swinging-door collision and exterior walking streaming remain
unverified or unfinished. Portal city transitions and revised AI are deferred.

Weapon combat, complete magic/crafting/dialogue, player UI and full vanilla-save
restoration remain open. ESS import requires explicit object mapping and does
not resume saved stacks or restore all world change forms. The CommonLib shim
is a skeleton and the editor is a design. Read docs/status.md and
docs/execution-plan.md for current evidence and the remaining scopes.

Do not reuse the old 23-of-47 open-item count as a current completion metric.
Some listed work landed under other branches, including compressed texture
upload in batching, while other merged branches retain integration gaps.
`docs/status.md` gives the evidence and limits per subsystem.
`ENGINE-PLAN.md` preserves the original done checks and later decisions.

The abbreviated fork skill gate remains stale. Use `tools/lint.sh`, including
all feature-range and target checks, instead of that abbreviated list.

The reference fixes in `0c84bbe`, reconciled with the audit improvements at
`6d990e7`, add distinct wait clock domains, safe dispel stubs and explicit
unsupported deterministic SPID rules. That checkpoint reported 345 stateful
natives and 191 stubs, before the integrated execution milestones. `docs/reference-audit.md` now supplies source paths
and acceptance checks for pending gameplay work.

## Traps

- `cargo clippy` results are cached. A piped `grep` will show you a stale clean
  run. Check the exit code of `cargo` itself, not of the pipeline.
- `crates/game/tests/transitions.rs` is pulled into `no_load_screen.rs` with
  `#[path]`, so it compiles twice and anything unused in the second build reads
  as dead code.
- Reading a form builds it. A residency check written as "does any reference of
  this cell exist" answers yes after any unrelated lookup, which silently
  suppressed a cell event until a test caught it.
- `HashMap` iteration order leaks into behaviour. A quest-start nondeterminism
  traced to `LoadOrder::winners_of` and was fixed with a sorted cached list.
  Sort anything that feeds a hash, a save, or an event order.
