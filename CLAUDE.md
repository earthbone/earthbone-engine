# Skyrim open engine (soe)

An open-source reimplementation of the Skyrim SE runtime that keeps the mod
ecosystem working. One design rule holds everywhere (research map, section 3):
the engine is a record database with an interpreter on top. Every system reads
forms keyed by resolved FormID from the load order, never engine-private
structs, so existing plugins, xEdit output and Synthesis patchers keep their
meaning. Core crates are GPL-3.0-or-later, the SKSE shim crates are MIT.

## Where the intent lives

- `ENGINE-PLAN.md`: rounds, per-crate done criteria, fork rules.
- `../research/skyrim-open-engine-map.md`: read sections 1 (verdict), 3 (form
  table and API boundary), 4 (VM and natives), 10 (first 90 days), 12 (risks).
- `../research/engine-map/verified/`: the verified briefs the map cites.
- `../BUILD-PLAN.md`: the mod manager rounds, with the engine goal at the end.
- `../research/project-intent.md`: the user's own words on goal, stack and
  compatibility, mined from earlier sessions. The engine is partly a
  redesign: preserve what mods depend on, replace workaround tooling with
  native systems where a better one exists, keep SKSE-shaped compatibility.

## Crates and docs

| Crate | What it is | Doc |
|---|---|---|
| `esm` | Load-order aware plugin reader, winning-override form table | `docs/esm.md` |
| `bsa` | BSA and BA2 archives plus a loose-file overlay by load order | `docs/bsa.md` |
| `nif` | Skyrim SE meshes into a renderer-neutral struct | `docs/nif.md` |
| `papyrus` | PEX loader, assembler, VM with scheduler and latent calls | `docs/papyrus.md` |
| `havok` | hkx packfiles, skeletons, spline clips, behavior graph model | `docs/havok.md` |
| `dds` | DDS container, BC1 to BC7 decode, encode behind a feature | `docs/dds.md` |
| `physics` | bhk shapes, a headless static world and capsule character. Godot's bundled Jolt drives the rendered path, the engine links no physics library | `docs/physics.md` |
| `navmesh` | Vanilla NAVM graph plus Recast generation | `docs/navmesh.md` |
| `profile` | plugins.txt, INI layering, MO2 profile layout | `docs/profile.md` |
| `commonlib-shim` | CommonLib header scope, MIT Rust skeleton | `docs/shim.md` |
| `godot-host` | GDExtension: cell streaming, mesh and terrain builders | `docs/godot-host.md` |
| `game` | Runtime form table, Papyrus object model, natives, clock, debug commands | `docs/game.md` |
| `save` | Vanilla `.ess` reader, change forms, quest stages, Papyrus block | `docs/save.md` |
| `audio` | FUZ, LIP and xWMA containers, Vorbis and Opus decode | `docs/audio.md` |
| `moddata` | SPID, KID and Base Object Swapper ini files, OAR folders | `docs/moddata.md` |

Other docs: `docs/status.md` (per subsystem detail), `docs/design-system.md` (what of the Creation Engine's design the
engine preserves), `docs/editor.md` (Creation Kit replacement on the Godot
editor), `docs/style.md` (Rust coding baseline), `docs/validation.md`
(conformance harness against vanilla behaviour).

## Creation Engine knowledge, under `../engine-refs/`

Open the corpus before writing a parser or a native, not after. Sixty
repositories and fetched page sets live there, and three times on 2026-09-18 a
fork went looking outside for something already cloned. **Load the `soe-corpus`
skill**, which says which source answers which question and carries the licence
rules for porting from each.

The four that answer most questions: `TES5Edit/Core/wbDefinitionsTES5.pas` for
record and subrecord layouts, `uesp-modfileformat/*.wikitext` for the same in
prose, `ck-wiki/*.wikitext` for Papyrus native semantics, and `Mutagen/docs`
for winning-override rules.

Licence care: the core crates are GPL-3.0-or-later so reading or porting from a
GPL tool is fine there, but no GPL derived code may reach the MIT shim crates.

## Working rules

- One crate per fork, in a worktree: `git worktree add ../soe-<crate> -b <crate> master`.
  Touch only `crates/<crate>/` and `docs/<crate>.md`, commit on the branch,
  report branch, commit, test count and what is untested.
- Edit tool for source edits. The auto-mode classifier blocks large scripted
  edits.
- Gate before commit: `cargo check --workspace`, clippy clean on the touched
  crate, `cargo test -p <crate>`. Clippy the workspace with `--all-features`
  too, since code behind an off-by-default feature escapes the normal run and
  rots: that is how the `dds` encode path drifted until 2026-09-18.
- Real-file tests are gated on
  `SKYRIM_DATA=/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data`
  and skip cleanly when unset. Synthetic fixtures are written by the tests.
- Sixty real Special Edition saves sit under `../engine-refs/saves/`, supplied by
  the user on 2026-09-18. `Clean Saves/` holds nineteen main quest stages,
  `Main Quest + Dawnguard(Vampire) + Dragonborn/` holds forty-one numbered 01 to
  41 covering the main quest, the Dawnguard vampire path (23 and 24 bracket the
  turn, 33 completes it) and Dragonborn. Ordered and named, so a quest state is
  known before anything is parsed and consecutive saves can be diffed. They are
  user data: read them, never commit one, never copy one into this repository.
  Tests reach them through `SOE_TEST_SAVE`.
- No git identity is configured: set `GIT_AUTHOR_NAME`, `GIT_AUTHOR_EMAIL`,
  `GIT_COMMITTER_NAME`, `GIT_COMMITTER_EMAIL` for every commit. No AI trailers.
- Prose in comments, docs and commits: no em dashes, no semicolons as prose
  separators, no AI vocabulary. `docs/style.md` is the Rust baseline.
- Natives are ordered by vanilla call count (`docs/papyrus.md`), semantics come
  from the CK wiki and play behaviour, never from the binary.
- As little proprietary magic as possible while staying compatible: read every
  vanilla format a mod can only express that way, and take the open format next
  to it wherever an author has a choice (Ogg and Opus next to xWMA, WASM next to
  PEX). No Godot patches, the extension works on stock Godot at the 4.4 API
  floor and is tested on current stable.
- The DLC are in scope (user, 2026-09-18): Dawnguard, HearthFires and
  Dragonborn sit in every gated load order, their scripts count in the
  native ordering and their natives are engine work, never a leftover.
- **Look locally first. This is the rule that gets broken most.** Before writing
  any parser, format reader, layout or algorithm, search `../engine-refs/` and
  load the `soe-corpus` skill. Sixty repositories are cloned there. On
  2026-09-18 alone, five forks began deriving from bytes what was already
  written down a directory away: the `hkb` class layouts (`serde-hkx`), the
  save file's Papyrus section (`FallrimTools`), the voice container's codec
  sniff (`Skyrim-NX-Toolkit`), the LIP header (`FaceFXWrapper`, which declares
  it as a struct), and character skinning (`nifly`, which ships working Skyrim
  characters). Two more went looking online for repositories already cloned.
  Every one of those cost hours and produced a worse answer than reading would
  have. Grep the corpus before the first line of a parser, not after the first
  failure.
- Do not reimplement what already has tooling (user, 2026-09-18). When you do
  write your own, name in the doc what you read and why it did not fit.
  Dependency scope is bounded too: prefer a small maintained crate, keep a
  heavy one (ffmpeg and its kind) behind a cargo feature or a trait with the
  binary invoked out of process, run `cargo tree` on anything new and report
  what it drags in. A credited vendored routine can beat a dependency that
  pulls in eighty crates.
- Code forks at most three at a time. Progress is read from branch commits and
  the master gate, never from an agent's own report.

## Status (2026-09-18, master 15dd049)

Rounds 1, 2 and 3 are merged, and so is most of round 6: physics on Jolt, the
character controller, door transitions and skinning. `input` and `playback`
are the two round 6 items still in flight. Round 7 (streamed transitions,
portal doorways) is written with done checks in `ENGINE-PLAN.md`. Workspace
gate green: 711 tests in 85 suites, none ignored, clippy clean at both ends of
the feature range, fmt and both linters clean, gated tests run against the
installed game.

What the engine does today, per subsystem, is in `docs/status.md`.

Read `ENGINE-PLAN.md` for what is next and why. Branches `input`, `playback`
and `no-load-screen` are in flight. `portal-cells` is held until
`no-load-screen` lands, because it builds on the split that item establishes
between streaming a cell and attaching it. The remaining near items are mod
data formats (SPID, KID, BOS, OAR) and the second passes on the behavior
interpreter and packages.

Adversarial testing has reached phase 3. Two findings worth carrying: an
archive record states its unpacked size and neither decompressor checked it,
so a record declaring 4,119 bytes whose stream held 23 returned 23 bytes and
success, and every reader downstream would have worked on a truncated buffer
believing it whole. And Papyrus has no overloading, so a compiled script
carrying two functions of one name in one state is not representable. The
reader was right and the assembler was wrong, which 14,026 shipped scripts
confirm with zero offenders.

A number falling can be a fix. Reading `XLKR` on actor references closed a
real gap, the `ACHR` reader had ignored linked references, and the first
start's native calls fell from 1,560 to 1,388 because scripts that used to
get `None` from `GetLinkedRef` now get the reference and take a shorter
branch. Same quests, same log lines. Check the cause before reading a drop
as a regression.

Known gaps, none of them silent: no UI (Scaleform is not executed, a native
layer with an MCM compatible config API is round 6), no combat, magic or
dialogue, no LOD consumption, no terrain texture layers or water, no audio,
no Proton launch attempted. Open risks are map section 12.
