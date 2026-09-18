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

## Status (2026-09-18)

- Rounds 1 and 2 merged. Round 3 merged so far: live validation (every gated
  test ran on the installed game, numbers in each `docs/<crate>.md`), actor
  base stats from `NPC_`, VM and world save and load (formats SOEV, SOEW,
  SOEG), Godot 4.4.1 loading the extension headless (`tools/godot/`,
  `godot/headless_test.gd`), parser fuzzing (`fuzz/`, `docs/fuzz.md`, five
  crashes fixed), vanilla scripts loaded from the BSA (28,143 of 28,152
  attachments bound), and a real cell streamed through esm, bsa and nif
  (`WhiterunBanneredMare`, 318 statics, 124,924 triangles), materials and
  textures on it (46 sets, 88 textures), full-property setters in the VM (97
  stale vanilla properties left), and the performance harness (`soe-bench`,
  `crates/trace`, `docs/performance.md`: Bannered Mare streams in 0.44 s of
  which 0.36 s is the reference scan, Godot draws it at 56 ms per frame from
  763 unbatched nodes, VM 12.7M instructions per second). Open in round 3:
  the behavior interpreter (start from `../research/behavior-graph-references.md`,
  the Project Anarchy headers are the semantics source), packages and AI, the
  next hundred natives (start with the leveled-list natives and the
  `Alias.RegisterForUpdate` chain walk that the first `start` left
  unresolved, see docs/game.md), a cell-to-reference index and surface
  batching (the two hot spots the trace found).
- 2026-09-18: the VM had evaluated PEX call and property name operands as
  variables, so every vanilla call dispatched as `None` until branch
  `vm-callname` fixed it. The first real `start` reaches 63 natives in 1,095
  calls. Any earlier claim that vanilla scripts "ran" meant they loaded and
  bound, not that their calls dispatched.
- `Vm::get_property` runs compiled getters and read-only properties are
  counted apart in `BindReport` (branch `vm-props`, merged 2026-09-18).
- Merged later on 2026-09-18: `cell-index` (esm `CellIndex`, reference scan
  361 ms to 0.5 ms, the material path is now 97 percent of a stream),
  `behavior` (havok interpreter, vanilla `mt_behavior.hkx` walks, pose within
  3.8e-6 of the sampler, behavior references and modifiers still open),
  `packages` (PACK view, sandbox, travel and sleep on the navmesh, 4,635 of
  5,961 vanilla packages resolve), `natives-2` (296 natives, alias dispatch
  walks base classes, the first real `start` has one unresolved call left),
  `test-plan` (`TEST_PLAN.md`, phase 2 waits for the user). Save format
  `SOEW` is version 2 with both branches' fields. Godot 4.7.2 loads the
  extension headless with the same Bannered Mare counts as 4.4.1, the binary
  sits next to the old one under `../tools/godot/`. Rendering decisions in
  `../research/rendering-performance.md`: compressed texture upload first,
  no Godot patches. `batching` merged the same day: surfaces batched per
  material as `RenderingServer` instances (763 nodes to 48), compressed DDS
  pass-through with a session texture cache (592 MB to 175 MB), exterior
  streaming with terrain, the Whiterun 3 by 3 block at p99 0.519 ms and
  worst 1.698 ms, the earlier 56 ms figure was a misread of
  `Performance.TIME_PROCESS`. With the index on the same tree a cell streams
  in 4 ms and the block in 13 ms, so the host question is settled for now.
  `determinism-hash` merged: per-tick state hash with subsystem parts,
  `SOE_HASH_DUMP`, two-run tests, one known source (`Game::start` orders
  quests by `HashMap` iteration in `LoadOrder::winners_of`, the gated test
  is ignored with that reason). The hash does not yet cover the natives-2
  fields (perks, spell casts, leveled lists, settings). `world-split` merged
  next: `world.rs` is a directory of `impl World` modules by concern (lookup,
  objects, refs, actors, quests, forms, events, timers, rng, driver, save,
  hash), the hash covers every field with a test per part, and
  `LoadOrder::winners` walks a sorted list so `Game::start` is
  deterministic and the 600-tick gated hash run passes. Workspace 419 tests,
  none ignored. Running: `natives-dlc` (DLC natives, zero unresolved calls)
  and `dirty-records` (deleted records handled natively, per-plugin dirty
  report), see ENGINE-PLAN.md rounds 5 and 6.
- Untested live: a real Proton launch, natives against the CK wiki pages.
- Language decision (2026-09-14): runtime crates stay Rust. Go only for a
  tooling component where it eases maintenance, behind a protobuf seam.
  Licensing confirmed: GPL-3.0-or-later core, MIT shim.
- Open risks (map section 12): no open behavior-graph interpreter exists,
  about a thousand natives with unwritten semantics, EULA and reverse
  engineering exposure, GPL sources in the reading list, a single-maintainer
  CommonLib fork, closed LOD tools, no GFx UI runtime, unverified sources,
  Godot open-world scale unproven, small team.
