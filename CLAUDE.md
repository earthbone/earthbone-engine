# Skyrim open engine (soe)

An open-source reimplementation of the Skyrim SE runtime that keeps the mod
ecosystem working. One design rule holds everywhere (research map, section 3):
the engine is a record database with an interpreter on top. Every system reads
forms keyed by resolved FormID from the load order, never engine-private
structs, so existing plugins, xEdit output and Synthesis patchers keep their
meaning. Core crates are GPL-3.0-or-later, the SKSE shim crates are MIT.

## Where the intent lives

- `docs/intent.md`: the full governing intent, restated on 2026-09-19 from
  the user's recorded goals and later decisions. Read it before planning work.
  It distinguishes commitments and design choices from shipped features.
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
| `hkx` | hkx packfiles, skeletons, spline clips, behavior graph model | `docs/hkx.md` |
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
(conformance harness against vanilla behaviour), and `docs/reference-audit.md`
(exact source contracts, fixes, reference inventory and pending gameplay work).

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

- For GPT agents, explain each action before taking it, including edits,
  checks and commits (user instruction, 2026-09-19).
- For interactive walkthroughs, retain the launch tool's process session ID
  and poll it for closure and exit status before replacing the extension.
  Normal shutdown prints `WALKTHROUGH stopped`, but the process result is
  authoritative, including on crashes. Leave user test windows open without
  a timeout. Do not use broad process-name searches or ask about closure
  when the tracked session already reports it.
- One writer per checkout. Preserve unrelated changes. The agent reviews each
  completed step and runs the gate. Before any commit or merge, show a diff
  summary naming the files, changes and reasons, then wait for explicit user
  approval of that summary, as the latest global instructions require.
  Leave changes unstaged while awaiting approval. Commit approved steps and
  keep changes isolated from other writers.
- Prefix every shell command with `rtk`. Use Serena and language servers for
  symbols, direct reads and RTK searches for named files and literal text.
- One crate per fork, in a worktree: `git worktree add ../soe-<crate> -b <crate> master`.
  Touch only `crates/<crate>/` and `docs/<crate>.md`, commit on the branch,
  report branch, commit, test count and what is untested.
- Edit tool for source edits. The auto-mode classifier blocks large scripted
  edits.
- Gate before commit: `rtk proxy ./tools/lint.sh` from the worktree root.
  It runs formatting, three clippy passes (production, all features/all
  targets, no default features/all targets), prose and config checks, fuzz
  formatting and tier 1 tests. Crate-only checks and the abbreviated fork
  skill commands are not substitutes. Report optional checks the script skips.
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

## Status (reconciled 2026-09-19, master 75cfb55)

Rounds 1, 2 and 3 have merged foundations. The round 6 branches are merged,
including `playback` at `5b1a9ed` and `input` at `75cfb55`. The round 7
simulation transition split, `no-load-screen`, merged at `35a5dd7`.
Merged does not mean the interactive walkthrough is verified.

`docs/status.md` is the current subsystem summary. `HANDOFF.md` names the
stopping point and proposed next work. This update reviewed source and Git
history, not a new engine run. The previous handover's 750-test tier 1 result
predates the input merge. Do not report it as a fresh gate on `75cfb55`.

Follow-up on branch `host-player-run`: PlayerNode's input/camera script now
passes in Godot 4.4.1 and 4.7.2, with real Bannered Mare collision/movement
checked on 4.7.2. The PC control map uses DirectInput scan codes and includes
movement bindings. Earlier tests had selected a console map by suffix.
Late rebindings, chord handling and action edges are corrected. Activation
only records a ray hit and does not drive the game/doorway lifecycle. Audio
buffers reach host playback code, but footsteps and dialogue are not driven
by the game loop and speaker output is unverified. These are the seams for
the proposed full Bannered Mare walkthrough. A movement-only launcher now
exists at `godot/walkthrough.gd`. Portal doorways remain a prototype
proposal. Mod-data parsing/application and behavior-2 have merged, while
OAR animation consumption and packages-2 remain incomplete.

## Reference-correction checkpoint, 2026-09-19

Based on master `75cfb55`, with the reference fixes in this change. The full
gate reports 835 test passes and every lint tool ran. The runtime registers
345 stateful natives and 191 stubs. Waits have distinct gameplay, menu and
game-hour clocks, persisted in SOEV version 2 with version 1 reading retained.
Quest getters agree on the highest stage. Dispel calls preserve learned and
equipped spells pending active effects. SPID deterministic fractional rules
are reported as unsupported, with their percentage and marker preserved.

The host keeps alpha and culling in material identity while sharing uploads,
and sends transformed native collision shapes to Jolt. The installed-game
startup check and Bannered Mare contact tests pass. Detailed evidence and
limits are in `docs/reference-audit.md` and `docs/physics.md`.

`docs/status.md` carries subsystem detail. `ENGINE-PLAN.md` and the reference
map now scope missing gameplay work and original-runtime probes. Those future
systems are pending, not delivered by the reference fixes.

Adversarial testing has reached phase 3. The archive decompression-size
mismatch and the PEX assembler's acceptance of duplicate function names
were fixed. Evidence, limits and repros remain in `TEST_LOG.md`.

A number falling can be a fix. Reading `XLKR` on actor references closed a
real gap, the `ACHR` reader had ignored linked references, and the first
start's native calls fell from 1,560 to 1,388 because scripts that used to
get `None` from `GetLinkedRef` now get the reference and take a shorter
branch. Same quests, same log lines. Check the cause before reading a drop
as a regression.

Known gaps: no player UI or MCM layer, no complete combat, magic or dialogue,
no LOD consumption, no terrain texture layers or water. The CommonLib shim
is a Rust skeleton without generated C++ headers or a compiled plugin.
The editor remains a design. The reference audit scopes pending gameplay
work, with combat, crafting and progression still requiring separate items.
No Proton launch has been reported.
Audio exists with the integration limits above. Open risks are in map section
12, read together with the later decisions in `ENGINE-PLAN.md`.
