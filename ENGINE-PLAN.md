# Skyrim open engine: build plan

The research behind this plan is `../research/skyrim-open-engine-map.md` (read sections 1, 3, 10
and 12 before any work) and the verified briefs under `../research/engine-map/verified/`. The
reference checkouts are under `../engine-refs/`. The design rule from section 3 of the map holds
everywhere: the engine is a record database with an interpreter on top. Every system consumes forms
keyed by resolved FormID, never engine-private structs, so xEdit output, Synthesis patchers and
existing plugins stay meaningful.

Licence: the core crates are GPL-3.0-or-later, the OpenMW model, so LMO's BSA parser, OpenMW's
esm4 and nif components and esplugin are usable as ports and reading material. The later SKSE shim
crates will be MIT so plugin authors can link them. This was decided on 2026-09-06 to unblock
work and is open to change while nothing has been published.

Crate prefix `soe-`. Every parser crate exposes plain data structs with no rendering or physics
dependency, and every crate has malformed-input tests (no parser in the briefs was ever fuzzed).
No Skyrim data files exist on the development machine: tests use synthetic files written by the
test suite from the spec, and real-file tests are gated on `SKYRIM_DATA=<path to Data/>` so they
skip cleanly when unset. No Godot binary is installed either, so the host crate must build with
cargo alone.

Rules for every fork: Edit tool for source edits, `cargo check --workspace`, clippy clean on the
touched crate, `cargo test -p <crate>`, house style (no em dashes, no semicolons in prose, no AI
trailers). Each fork works in its own git worktree of this repo (`git worktree add ../soe-<crate>
-b <crate> master`), touches only `crates/<crate>/` plus `docs/<crate>.md`, commits on its
branch, and reports branch, commit, test count, and what is untested.

## Round 1 (2026-09-06, forks): weeks 1 to 13 of the map's first 90 days, in parallel

- **esm** (`crates/esm`, branch `esm`). Load-order aware TES4 plugin reader: TES4 header,
  masters, groups (all group types), records, subrecords, zlib-compressed records, the ESL flag and
  its local FormID range, FormID resolution against the master list, and a winning-override form
  table across a load order (Mutagen's Big Cheat Sheet is the semantics reference). Typed views
  for CELL, REFR, WRLD, LAND, NAVM, STAT and the TES4 header, every other record as a raw
  subrecord map that still round-trips. References: UESP Mod File Format (hand-verified, the site
  blocks fetching), `engine-refs/TES5Edit/wbDefinitionsTES5.pas` for field layouts,
  `engine-refs/openmw/components/esm4`, `engine-refs/esplugin`. Done: a test writer that builds
  synthetic plugins, a two-master plus override load order resolving to the right winner, a
  gated test that loads the five base masters and prints record counts per type for later diffing
  against xDump.
- **bsa** (`crates/bsa`, branch `bsa`). Port LMO's parser (`../linux-mod-organizer/src/bsa`,
  GPL-3.0, keep the attribution) into a standalone crate: BSA v103, v104, v105 with zlib and LZ4,
  BA2 GNRL and DX10, the TES4 name hash with its loop quirk, Windows-1252 names, plus a virtual
  file system layer that overlays loose files over archives by load order and answers a path
  lookup case-insensitively. Done: LMO's tests carried over and passing, an overlay test with a
  loose file shadowing an archived one, a malformed-header test.
- **nif** (`crates/nif`, branch `nif`). Reader for Skyrim SE NIF (version 20.2.0.7, user version
  12, BS version 100 and 130): the block graph, NiNode transforms, BSTriShape and NiTriShape
  geometry, BSLightingShaderProperty and BSEffectShaderProperty with flags and texture sets,
  skinning (BSDismemberSkinInstance, NiSkinPartition), collision blocks as opaque bhk data for
  later, and a neutral output struct (positions, normals, tangents, uv, indices, material, bone
  weights). References: `engine-refs/nifxml/nif.xml` as the format authority,
  `engine-refs/openmw/components/nif`, `engine-refs/nifly`. Done: a synthetic NIF written from
  nif.xml by the tests loads to the expected mesh, a gated real-file test, a truncated-file test.
- **papyrus** (`crates/papyrus`, branch `papyrus`). PEX loader per the UESP Compiled Script File
  Format (Champollion in `engine-refs/Champollion` is the reference decoder), then a VM: object
  model with parent scripts, properties, variables, states, events, the full opcode set, a
  scheduler with threads, latent calls and `Utility.Wait`, and a native registry trait so a host
  registers natives by `Object.Function` name with a collision rule (first registered wins,
  duplicates reported). Reference `engine-refs/skymp` papyrus-vm (MIT). Done: a tiny assembler in
  the test crate that emits PEX, tests for every opcode family, a latent-call test where a
  thread suspends and resumes, and a mock native set that runs a script calling
  `Debug.Notification` and a property getter.
- **havok** (`crates/havok`, branch `havok`). Reader for Skyrim SE hkx packfiles (Havok 2010.2,
  the 64-bit SE layout) covering hkaSkeleton, hkaAnimationBinding, hkaSplineCompressedAnimation
  with its decoder, and the hkb behavior classes as a typed data model (hkbBehaviorGraph,
  hkbStateMachine, states, transitions, events, variables, hkbClipGenerator, blender
  generators, modifiers) drawn from the activeragdoll `havok_behavior.h` class layouts
  (https://github.com/adamhynek/activeragdoll, fetch it into `engine-refs/activeragdoll`) and
  `engine-refs/serde-hkx`, `engine-refs/HavokLib`, `engine-refs/Skyrim-Behavior-Editor-`.
  Then `docs/havok.md`: the behavior-graph interpreter design with the event and variable model
  and how a replay harness would drive it. Done: a synthetic packfile round trip for the
  skeleton and a clip, spline decode against a hand-computed curve, a behavior graph parsed
  into the model from a fixture serde-hkx or the editor repo ships, and the design document.
- **godot-host** (`crates/godot-host`, branch `godot-host`). A Godot 4 GDExtension in Rust
  (godot-rust gdext, pin the version through context7 and `engine-refs/gdext`) that builds
  without a Godot binary: a `CellStreamer` node that keeps a grid of cells loaded around a
  position with budgets, a mesh builder from the `nif` crate's neutral struct into ArrayMesh
  data (code against a local trait mirroring that struct until the nif crate lands), a terrain
  builder from LAND-shaped height grids sized for Terrain3D, and a synthetic Whiterun-sized
  benchmark (generated meshes and heights) with a headless timing test. Plus a `godot/`
  project directory with the `.gdextension` file so the extension loads once Godot is
  installed. Done: cargo builds and tests pass with no Godot present, the benchmark test prints
  cells streamed per second, `docs/godot-host.md` records the streaming budget design.

Deferred to round 2: Jolt binding from bhk shapes, DDS loading, the top hundred natives, the
CommonLib shim scope, Recast navmesh generation, the Creation Kit replacement.
