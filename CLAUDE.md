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
- `ck-wiki/*.wikitext`: Papyrus native semantics per function (being filled by
  a crawl, check the directory is non-empty before relying on it).
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

## Status (2026-09-14)

- Round 1 (esm, bsa, nif, papyrus, havok, godot-host) merged. Round 2 (dds,
  physics, navmesh, profile, shim, editor, game) complete as first
  implementations once the `game` branch merges.
- Untested live: every `SKYRIM_DATA` gated test beyond esm and papyrus, a
  Godot binary loading the extension, a real Proton launch, fuzzing over any
  parser, VM save and load.
- A 2026-09-05 session agreed a Go for tooling plus Rust for runtime split with
  a protobuf seam. The plan and all crates are Rust only and do not reflect
  that decision yet.
- Open risks (map section 12): no open behavior-graph interpreter exists,
  about a thousand natives with unwritten semantics, EULA and reverse
  engineering exposure, GPL sources in the reading list, a single-maintainer
  CommonLib fork, closed LOD tools, no GFx UI runtime, unverified sources,
  Godot open-world scale unproven, small team.
