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
| `physics` | Jolt through C bindings, bhk shapes, character capsule | `docs/physics.md` |
| `navmesh` | Vanilla NAVM graph plus Recast generation | `docs/navmesh.md` |
| `profile` | plugins.txt, INI layering, MO2 profile layout | `docs/profile.md` |
| `commonlib-shim` | CommonLib header scope, MIT Rust skeleton | `docs/shim.md` |
| `godot-host` | GDExtension: cell streaming, mesh and terrain builders | `docs/godot-host.md` |
| `game` | Runtime form table, Papyrus object model, natives, clock | `docs/game.md` |

Other docs: `docs/design-system.md` (what of the Creation Engine's design the
engine preserves), `docs/editor.md` (Creation Kit replacement on the Godot
editor), `docs/style.md` (Rust coding baseline), `docs/validation.md`
(conformance harness against vanilla behaviour).

## Creation Engine knowledge, under `../engine-refs/`

Open the corpus before writing a parser or a native, not after.

- `TES5Edit/Core/wbDefinitionsTES5.pas`: record and subrecord layouts, the
  authority for every field.
- `uesp-modfileformat/*.wikitext`: the same layouts in prose, per record type.
- `ck-wiki/*.wikitext`: Papyrus native semantics per function, 402 pages
  fetched through the Wayback Machine (the live site is behind Cloudflare),
  289 of them reconstructed from HTML (`HTML_FALLBACK.txt`), 60 missing
  (`FAILED.txt`), resume with `ck-wiki/crawl_ck_wiki.py`.
- `uesp-skyrim-mod/*.wikitext`: engine quirks and modding conventions.
- `wrye-bash/Mopy/bash/brec` and `wrye-bash/Mopy/bash/game/skyrimse`: a
  second record-definition source, plus `wrye-bash.wiki` for bash tags.
- `Mutagen/docs`: winning-override semantics, the Big Cheat Sheet.
- `CommonLibSSE-NG/include/RE` (and `CommonLibSSE/include/RE`): the runtime
  object model SKSE plugins see.
- `openmw/components/esm4`, `Champollion`, `skymp`: reader and VM reference.
- `libloot/docs`: load order rules and masterlist syntax.
- `TES5Edit.github.io`: the xEdit documentation site source.

## Working rules

- One crate per fork, in a worktree: `git worktree add ../soe-<crate> -b <crate> master`.
  Touch only `crates/<crate>/` and `docs/<crate>.md`, commit on the branch,
  report branch, commit, test count and what is untested.
- Edit tool for source edits. The auto-mode classifier blocks large scripted
  edits.
- Gate before commit: `cargo check --workspace`, clippy clean on the touched
  crate, `cargo test -p <crate>`.
- Real-file tests are gated on
  `SKYRIM_DATA=/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data`
  and skip cleanly when unset. Synthetic fixtures are written by the tests.
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
- Code forks at most three at a time. Progress is read from branch commits and
  the master gate, never from an agent's own report.

## Status (2026-09-18, master 2984fb0)

Rounds 1, 2 and 3 are merged. Round 5 (hardening and tooling) is running,
round 4 (tooling) and round 6 (UI) are written with done checks in
`ENGINE-PLAN.md`. Workspace gate green: 426 tests in 59 suites, none ignored,
clippy and fmt clean, gated tests run against the installed game.

What the engine does today, per subsystem:

- **Records.** `esm` reads the load order with winning-override semantics, a
  `CellIndex` maps cells and the worldspace grid to their placed objects
  (build 0.15 s over 872k placed objects, lookup 9 ns), and `dirty.rs`
  reports identical-to-master overrides, deleted references and deleted
  navmeshes per plugin. Dirty plugins need no cleaning: a deleted reference
  resolves and starts disabled, a deleted navmesh drops with edge repair.
- **Scripts.** 347 natives with state plus 189 stubs, the VM walks base
  classes on dispatch, and the first real `start` on the installed game runs
  423 quests through 1,388 native calls with **zero** unresolved calls, which
  the gated test now asserts. The five masters declare 686 natives in all and
  the gated report lists what the registry still lacks, with call-site counts.
  No script named for a DLC declares a native of its own: every DLC native is
  an addition to a base class, so DLC coverage is measured by call sites, not
  by script name. `SOEV`, `SOEW` (version 3) and `SOEG` save and load.
- **Actors.** `NPC_` base stats, `PACK` packages with sandbox, travel and
  sleep on the navmesh graph (4,635 of 5,961 vanilla packages resolve), and a
  havok behavior interpreter that walks the vanilla `mt_behavior.hkx` with
  poses within 3.8e-6 of the clip sampler.
- **Rendering.** The Godot host batches surfaces per material as
  `RenderingServer` instances, uploads block-compressed DDS untouched from a
  session cache, and streams interior and exterior cells with terrain. The
  Whiterun 3 by 3 block draws at p99 0.519 ms, a cell streams in 4 ms. Godot
  stays, unpatched, tested on 4.4.1 and 4.7.2 with the api-4-4 floor.
- **Determinism.** A per-tick state hash over world, VM and clock with a part
  per subsystem, `SOE_HASH_DUMP`, and gated 600-tick runs that hash equal
  twice. `LoadOrder::winners` walks a sorted list so nothing depends on hash
  map order.

Read `ENGINE-PLAN.md` for what is next and why. The near items are
`TEST_PLAN.md` phase 2 (waiting on the user's go), mod data formats (SPID,
KID, BOS, OAR), and the second passes on the behavior interpreter and
packages. Branch `audio` is in flight.

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
