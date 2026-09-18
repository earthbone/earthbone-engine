# AGENTS.md

Handover for an agent with no prior context on this repository. Read this
first, in full. It is short on purpose. Everything it points at is real and
worth opening before you write code.

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

Godot is the host, used unpatched, through a GDExtension. The host drives
`RenderingServer` and `PhysicsServer3D` by resource id rather than building
node trees, because per-node overhead is the thing that would sink an open
world at this scale. There is exactly one deliberate exception, the audio
module, because Godot has no server-level API for positional sound and
`AudioStreamPlayer3D` is the only route to attenuation. If you find yourself
adding a second exception, say so loudly in your report rather than quietly.

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
| `havok` | hkx packfiles, skeletons, spline clips, behavior graph model |
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

```
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

Tier 1 is currently 750 tests and takes about twelve seconds against a five
minute budget. It must stay under that budget.

Tests that need the installed game are gated on an environment variable and
skip cleanly when it is unset:

```
SKYRIM_DATA="/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data"
```

Sixty real Special Edition saves sit under `../engine-refs/saves/`, reached
through `SOE_TEST_SAVE`. They are the user's own data. Read them, never commit
one, never copy one into this repository.

## How work is organised

One plan item, one git worktree, one branch, one report.

```
git worktree add ../soe-<item> -b <item> master
```

Touch only the crates the item names, plus their docs. If the item genuinely
needs a line in another crate, keep it minimal and say so in the report. Commit
incrementally, one commit per milestone, because a long-running agent that
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

`master` is green at 750 tier 1 tests. Merged and working: the plugin reader
and form table, archives, meshes including skinned characters, the Papyrus VM
with 296 natives, behavior graphs, AI packages, navmesh, cell streaming,
physics on Jolt, a character controller, door transitions without a loading
screen, audio output, and a vanilla save reader. The DLC are in scope and sit
in every gated load order.

Twenty-three of the plan's forty-seven items are open. They group as testing
and quality (ten items, including `determinism` record and replay, `proptest`,
`failpoints`, `perf-gates`, `mod-corpus`), rendering and performance (six,
including `gpu-textures`, `multimesh`, `occluders`, `lod`), tooling and editor
(four, including a record inspector and a Papyrus compiler), and gameplay
(three: `packages-2`, `ui`, `console`).

**The most important thing a newcomer should know about that list is what is
not on it.** The plan has no item for combat, dialogue, magic, crafting or
levelling. It is thorough about the engine substrate and silent about the game
layer that sits on top. Much of that layer is data the substrate already reads,
combat runs on behavior graphs and game settings, dialogue is DIAL and INFO
records driving the VM, crafting is COBJ records, so the work is wiring rather
than invention. It is still the bulk of the distance to something playable and
it has not been scoped.

Two gaps are known and unplanned. `crates/godot-host` has no door handling, so
nothing drives `arm_doorway` from a key press, which is the first job once the
`input` branch merges. And the fork workflow's abbreviated gate omits
`--all-features`, which is what let those thirteen clippy errors through.

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
