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
skip cleanly when unset. Skyrim SE is being installed on 2026-09-06 into the Steam library under
`/run/host/media/pengolodh/Data` (find it with `find /run/host/media/pengolodh/Data -maxdepth 7
-iname Skyrim.esm`), so a fork whose crate is ready before it finishes reports the gated test as
written but not run, and one that finds the files runs it and reports the numbers. No Godot binary is installed either, so the host crate must build with
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
  (<https://github.com/adamhynek/activeragdoll>, fetch it into `engine-refs/activeragdoll`) and
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

## Round 2 (after round 1 merges): the runtime seams

Each item is a fork in its own worktree, same rules. Items depend on the round-1 crates by name,
so they start once master carries them.

- **game** (`crates/game`, branch `game`). The runtime form table: load a load order through
  `soe-esm` into resolved forms, define the object model Papyrus sees (Form, ObjectReference,
  Actor, Cell, Quest, with the base fields every native reads), register the first hundred
  natives by vanilla call count into the `soe-papyrus` native registry (Debug, Utility, Game,
  Form, ObjectReference activation and enable state, Quest stages and objectives, Actor
  inventory and values), and a game clock that drives the VM scheduler. Done: a synthetic
  plugin with a quest script that advances stages on activation runs end to end.
- **dds** (`crates/dds`, branch `dds`). DDS container and DXGI decoding (BC1, BC3, BC4, BC5,
  BC7, uncompressed RGBA) to RGBA8 with mip levels, plus the encode path the mod manager's
  TransformedTexture already needs, so both share one crate. Done: a synthetic BC1 block
  decodes to the expected pixels, every unsupported format is a typed error.
- **physics** (`crates/physics`, branch `physics`). Jolt through its C bindings: bhk shapes from
  the `soe-nif` collision blocks (box, sphere, capsule, convex, compressed mesh) converted to
  Jolt shapes for statics, a world stepped from the game clock, and a character controller
  capsule. Done: a static mesh from a synthetic NIF stops a falling capsule.
- **navmesh** (`crates/navmesh`, branch `navmesh`). Two halves: read the vanilla NAVM records
  through `soe-esm` into a walkable graph so vanilla AI packages have their data, and generate
  a Recast navmesh from `soe-nif` collision plus LAND heights so new content and mods with
  broken navmeshes get one without the Creation Kit. Done: a synthetic cell produces a
  navmesh whose path between two points avoids a placed box.
- **shim** (`crates/commonlib-shim`, branch `shim`, MIT). The CommonLib shim scope: a header
  diff against the alandtse CommonLibSSE-NG `include/RE` listing every type SPID, Base Object
  Swapper and Papyrus Extender touch, grouped by whether the new engine has the concept, and
  the skeleton of the Rust-side API those headers would bind to. Done: `docs/shim.md` with the
  grouped list and counts, and the skeleton compiles.
- **editor** (`docs/editor.md`, branch `editor`, documentation only). The Creation Kit
  replacement design: the Godot editor as host, a forms dock backed by `soe-esm` with
  winning-override display, standard-format import (glTF and PNG to NIF and DDS through the
  crates above), navmesh regeneration on save, and what of the Creation Kit's own workflow is
  worth keeping. Draws on `docs/design-system.md` and the CKPE and Skyrim-Behavior-Editor
  references for what modders have had to bolt on.
- **profile** (`crates/profile`, branch `profile`). Load order and INI conventions from the
  installers-ini research lead: `plugins.txt` and `loadorder.txt` reading with ESM and ESL
  rules, `Skyrim.ini` and `SkyrimPrefs.ini` with the precedence documented there, MO2 profile
  layout so a manager loadout is playable directly. Done: an MO2-shaped profile directory
  resolves to the expected load order.

Round 2 status (2026-09-14): dds, physics, navmesh, profile, shim and editor merged on 2026-09-06.
`game` merged on 2026-09-14 (`crates/game`, `docs/game.md`, `docs/validation.md`): VMAD and QUST
readers, the form table with lazy per-form state, 138 natives plus 178 quiet stubs, the clock, the
scenario harness. The VM gained per-object base classes for it (`Vm::create_object_of`). Done check
met: the synthetic quest advances on activation end to end, a door teleports, a container opens,
an `OnUpdate` timer fires on the clock. Every item in rounds 1 and 2 now has a first implementation.

## Round 3 (not started): live validation and the seams the map left open

- Run every `SKYRIM_DATA` gated test against the installed game and record the numbers in the docs.
- Capture Papyrus logs from vanilla for three scenarios (a door, a container, a quest stage) and
  check them in under `crates/game/tests/captures/` with their scenario files, see
  `docs/validation.md`.
- A save-file reader (`crates/save`) so quest stages, globals and reference state from a vanilla
  save become scenario expectations without hand transcription.
- Fetch the Creation Kit wiki's Papyrus function pages through a browser session (the site is
  behind a Cloudflare challenge for plain fetches) into `engine-refs/ck-wiki/`, then review each
  native in `docs/game.md` against its page.
- Godot binary installed, the extension loaded, the Whiterun benchmark run for real.
- Decide the Go plus Rust split recorded on 2026-09-05 (Go tooling, Rust runtime, protobuf seam)
  against the all-Rust workspace that exists, and write the outcome here.
