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

## Round 3: fill the engine gaps (started 2026-09-14)

Same fork rules as rounds 1 and 2. Each item is one worktree branch, merged to master when its done
check holds. Order is by what unblocks the most.

| Item | Branch | Done check |
|---|---|---|
| Live validation | `live-validation` | Every `SKYRIM_DATA` gated test in every crate has run against the installed game, failures fixed or filed, and each `docs/<crate>.md` carries the numbers (records, archives, meshes, scripts, cells parsed, time taken). |
| Actor base stats | `game-npc` | `NPC_` `ACBS` and `DNAM` feed actor values, level and flags, `CNTO` and `SPLO` seed inventory and spells, `RNAM` gives `GetRace`. Test on a synthetic NPC_ and on the Skyrim.esm Prisoner. |
| VM save and load | `vm-save` | Script instances, variables, threads and the world's run-time state serialize to a file and load back, a quest mid-flow resumes and finishes. Own format first, ESS import is round 4. |
| Godot headless load | `godot-headless` (merged, real cell merged as `godot-real-cell`, host study in `../research/host-engine-options.md` says stay on Godot and gate on the Whiterun exterior benchmark, open-world support such as floating origin and cell-ring streaming is built in the host crate) | A Godot 4 binary in `../tools/godot/`, the extension built as a cdylib and loaded by `godot --headless` on a test project, one cell streamed and its mesh count printed. |
| Behavior interpreter | `behavior` | The havok crate's graph model runs: state machines, blend nodes, clip generators, transitions on events, for one vanilla actor skeleton and its idle plus walk clips, pose output checked against the clip sampler. The single hard blocker of the map. |
| Packages and AI | `packages` | `PACK` records read, sandbox, travel and sleep packages drive an actor through the navmesh graph on the clock, `EvaluatePackage` is real. |
| Parser fuzzing | `fuzz` | `cargo fuzz` targets for esm, bsa, nif, hkx, dds and pex, one hour each without a panic, findings fixed. |
| Performance harness | `perf` | `soe-bench` times every subsystem over the installed game, the trace crate records spans per phase and per native call and exports Chrome trace files, `docs/performance.md` carries the numbers, the thread scaling and the tracing overhead on this machine. Landed 2026-09-17. |
| Natives by call count | `natives-2` | The next hundred natives from `docs/papyrus.md` with state behind them, each with a scenario, and the conformance table updated. |

Round 3 status (2026-09-17): live validation, actor base stats, VM save and load, Godot headless
load (extended to a real cell) and parser fuzzing are merged. Behavior interpreter, packages and AI,
and natives-2 are not started.

## Round 4: tooling

Rust by default. Go only where a component is easier to maintain in Go than in Rust, behind the
protobuf seam, never inside the runtime crates.

| Item | Done check |
|---|---|
| Validation runner | A binary that loads a profile, runs a directory of scenario files, and prints a conformance report with the capture each scenario cites. |
| Save reader | `crates/save`: ESS header, change forms and globals table read, quest stages and globals from a vanilla save become scenario expectations. |
| CK wiki review | `engine-refs/ck-wiki/` filled, every native in `docs/game.md` checked against its page, differences fixed or listed. |
| Editor backend | `docs/editor.md` becomes code: record editing over the esm crate with the Godot editor as the front end. |
| Packaging | A profile plus load order becomes a runnable install without a mod manager in the loop. |
| Scripting beyond Papyrus | Decided (2026-09-17, user): WebAssembly through wasmtime as the mod extension ABI, bound to the same native registry and form table Papyrus uses, with the component model for versioned interfaces. Rust natives stay the engine-level path. Lua only on author demand. GDScript for editor tooling and UI only. Done check: a Rust mod compiled to wasm subscribes to OnActivate and calls SetStage on the synthetic quest. |

Decisions (2026-09-14, user): the runtime stays Rust. Go is used for a tooling component only when
it makes that component easier to maintain than Rust would, behind a protobuf seam, never inside
the runtime crates. Licensing stands: GPL-3.0-or-later core, MIT shim.

## Review of 2026-09-18 and round 5: hardening

The user passed on an outside review. What it got right is folded in below, what the plan
already covered is noted so nobody re-litigates it.

Already covered, no change: the research map lists the Scaleform UI gap (section 12, mitigation is
a native UI layer that reads SkyUI's config records, not its SWF), FaceGen, FUZ and XWM audio,
navmesh and the closed LOD tools (consume DynDOLOD, BTO and BTR output, never generate LOD in year
one). SKSE DLLs never load natively, the `commonlib-shim` crate is the plugin API and the popular
SKSE Papyrus extensions are natives in `crates/game` like any other. `research/host-engine-options.md`
already weighs OpenMW's paging, Godot's single-precision band and the double-precision build, the
decision is single precision with a centred origin and cell-grid origin shifting only when a mod
worldspace needs it. Headless simulation without Godot exists (`crates/game` harness, `soe-bench`),
and `fuzz/` covers the six parsers. The `World` RNG is a seeded xorshift and the clock is virtual.

Changes made on 2026-09-18 from the review: the release profile is `lto = "fat"` and
`codegen-units = 1` (no `target-cpu=native` in shipped builds), the batching fork drives
`RenderingServer` instances and `MultiMesh` buffers from Rust rather than one node per reference,
frame times are reported as p50, p99, p99.9 and worst hitch, never as an average alone.

| Item | Branch | Done check |
|---|---|---|
| Test plan audit | `test-plan` | `TEST_PLAN.md`: per crate its untrusted inputs, unsafe blocks, concurrency and FFI boundary, a table of component, failure modes, technique, tool, expected bug class, cost and priority ranked by risk times likelihood, and an explicit list of techniques not worth it and why. No tests written in this item. |
| Determinism | `determinism` | One seed drives RNG, clock and job order. A state hash per tick, input record and replay, a test that replays a recorded quest run and matches every hash. Cross-thread work joins in a fixed order. |
| Tiered suite | `test-tiers` | Tier 1 under five minutes on every commit (the current gate), tier 2 nightly (fuzzing, Miri, sanitizers, `cargo-mutants`), tier 3 weekly (soak, full corpus over the installed game and the top mods). Every failure prints its seed and a one-line repro. `TEST_LOG.md` records what each addition found and cannot catch. |
| Generative tests | `proptest` | Property and metamorphic tests for the parsers (parse, write, parse round trips, load-order independence where the format promises it), differential tests against xEdit dumps, Champollion and OpenMW where a reference exists. Structure-aware fuzz targets through `arbitrary` for the six parsers, the save formats and PEX bytecode. |
| Fault injection | `failpoints` | `fail` crate points on IO, truncated and corrupt files, missing masters, kill during save, memory caps, 1 fps and 1000 fps, script overload. Each scenario in the validation harness. |
| Performance gates | `perf-gates` | Instruction-count gates in CI through `iai-callgrind` on the parsers and the VM, frame-time percentile gates on the benchmark scenes, a 24 hour soak for leaks, a thousand-actor stress, startup time. Allocator swap to mimalloc measured, per-frame arenas where the trace shows steady-state allocation. |
| Console and crash reporter | `console` | In-game console over the harness commands, a crash report that carries the seed and the replay, the clean-room policy written in `docs/clean-room.md`. |
| Supply chain | `deny` | `cargo-deny`, `cargo-audit`, `cargo-vet` in the gate, `cargo-geiger` report in docs. |

Order: the test plan audit first, then determinism as soon as the packages and natives-2 branches
merge (it touches `world.rs` and the clock), the rest as forks in any order.

Added 2026-09-18, after the rendering brief (`../research/rendering-performance.md`) and the
behavior and packages merges:

| Item | Branch | Done check |
|---|---|---|
| Compressed texture upload | `gpu-textures` | DDS payloads go to Godot as block-compressed bytes with their mip chain (`FORMAT_DXT1`, `DXT5`, `RGTC`, `BPTC`), the dds crate decode stays as fallback, a session-long texture cache on the data source, stream time recorded before and after. First item of the brief, it is 97 percent of a stream. |
| MultiMesh buckets | `multimesh` | Repeated statics and grass per cell in `MultiMesh` buckets filled on worker threads, p99 frame time recorded. |
| Runtime LOD and BTO | `lod` | LOD index arrays on batched surfaces, a BTO reader for Bethesda's precomputed terrain and object LOD, shown through visibility ranges. |
| Occluders | `occluders` | One `ArrayOccluder3D` per cell from the largest opaque statics, kept only if p99 improves. |
| Behavior interpreter 2 | `behavior-2` | Behavior references through `hkbSymbolIdMap`, modifiers, conditions, priority arbitration, transition effects, bone masks, extracted motion. The standing idle plays under the vanilla graph. |
| Procedure trees | `packages-2` | PACK procedure trees (sequence, random, simultaneous), the 18 remaining procedures, package fragments, interrupt and combat flags. |
| hkx rename | `hkx-rename` | The `havok` crate becomes `hkx`, the name says the format it reads, not a library it links. |
| Papyrus log import | `papyrus-log` | A vanilla `Papyrus.0.log` parses into scenario expectations, a differential run against the VM reports every divergence. |
| Script coverage | `script-coverage` | A harness run records which vanilla script functions executed, reported as a percentage per quest and per script. |
| Papyrus compiler | `psc` | `.psc` source compiles to PEX with the assembler, scenarios can be written in Papyrus, output matches the Creation Kit compiler on the vanilla sources that ship. |
| Record inspector | `inspector` | An xEdit style record tree over the esm crate inside the Godot editor, the first slice of the round 4 editor backend. |
| Mod corpus bot | `mod-corpus` | Nightly load of the top mods fetched by the mod manager, reporting parser failures, unresolved natives and missing assets. |
| Frame capture harness | `capture-harness` | Headless captures per cell compared with stored images under an SSIM tolerance, a rendering regression fails a test. |

Not patching Godot: GPU occlusion, mesh shaders, virtual geometry and virtual texturing all need
a fork, none is needed at Skyrim's asset counts, and a patch brings back the per-release rebase
cost the single-precision decision avoided. Track proposal 13406 (draw-list access in
`CompositorEffect`) only.

### Second review, 2026-09-18, decisions

Design rule (user): as little proprietary magic as possible while staying compatible with the
game. Where the vanilla format is the only way a mod can express something (ESM records, NIF,
HKX, PEX, DDS, FUZ, xWMA), the engine reads it. Where a mod author has a choice, the engine also
takes the open format (Ogg Vorbis and Opus next to xWMA, glTF next to NIF later, WASM next to
PEX) so new mods need no proprietary tool.

- Godot version: the repo pinned 4.4.1 on 2026-09-17 because that was the binary the extension
  was first proven on, and the gdext `api-4-4` floor was chosen for built-in Jolt. Stable is
  4.7.2 (2026-08-18). The extension keeps the 4.4 API floor and is tested on 4.7.2, the
  rendering brief gets a section on what 4.5 to 4.7 changed in the renderer, and the binary in
  `../tools/godot/` moves to 4.7.2 once the headless test passes on it.
- Determinism detector now, fixes in round 5: a per-tick state hash and a test that runs the same
  seed twice headless and compares every hash, additive to the world file. Known sources to
  check first: `HashMap` iteration (32 uses in `world.rs`, 23 in the VM, 3 in `ai.rs`), which
  is randomly seeded per process, so any world logic iterating one differs between runs
  (fixed hasher, `BTreeMap` or `IndexMap` where order matters), and the single shared world RNG,
  which makes replays brittle (derive a stream per system or per entity from the seed).
- `world.rs` split: one 56 KB file that three branches needed serialises the forks. Splitting it
  by concern (references, actors, quests, timers and RNG, save) is a round 5 item ahead of any
  feature, done right after the current batch merges so no branch is in flight.
- Fork cap: code forks at most three at a time, document forks unlimited. Progress is verified
  by commits on the branch and the gate on master, never by an agent's own report.
- Exterior benchmark threshold, fixed before the numbers arrive: the Whiterun exterior 3 by 3
  block at 1600 by 900 on this machine passes at p99 frame time at or under 16.7 ms and worst
  hitch under 33 ms with every static drawn, and a cell streams in under 250 ms on worker
  threads. Fail on either and the next step is MultiMesh buckets and compressed textures, not a
  host change. A host change is only on the table if the block cannot reach 33 ms p99 after
  those.
- The rendering brief's order is provisional until the batching fork's numbers land. Rank by the
  measured span, not the literature.
- UI gets its own round (round 6): a native Godot UI reading the same form table, an MCM
  compatible config API for the `SKI_ConfigBase` script interface so MCM menus keep working,
  and a SkyUI-shaped inventory and magic menu. Scaleform SWF is not executed.
- Mod data formats are a separate compat surface from natives: SPID `_DISTR.ini`, KID
  `_KID.ini`, Base Object Swapper `_SWAP.ini`, OAR and DAR animation folders, the shim does not
  cover them. Round 5 item `mod-data-formats`: a crate that reads them into load-time patches on
  the form table and the animation set, the native replacement for those plugins.
- Audio: an `audio` crate decodes FUZ and xWMA for vanilla data and also plays Ogg Vorbis and
  Opus files under the same sound descriptors, so new mods can skip the proprietary encoder.

| Item | Branch | Done check |
|---|---|---|
| Determinism detector | `determinism-hash` | Per-tick state hash over the world, VM and clock, two headless runs of one seed compare equal on every tick or the test names the first diverging tick and subsystem. |
| World split | `world-split` | `world.rs` becomes modules by concern with no behaviour change, every test green, no public API change outside the crate. |
| Godot 4.7.2 | `godot-4-7` | The headless test and the Bannered Mare run pass on 4.7.2, the binary under `../tools/godot/` updated, differences noted in docs/godot-host.md. |
| Mod data formats | `mod-data-formats` | SPID, KID and BOS ini files and OAR folders parse and apply on the synthetic fixtures and on one real mod of each kind. |
| Audio | `audio` | FUZ split, LIP parse, xWMA decode, Ogg Vorbis and Opus playback under SNDR and SOUN descriptors, a voice line plays with its lip data. |
| UI round | `ui` | Round 6, scoped when round 5 closes. |

Scope note (2026-09-18, user): the DLC are in scope. Dawnguard, HearthFires and Dragonborn are
part of the load order every gated run uses, their scripts count in the native ordering, and a
native they declare (`Actor.SendVampirismStateChanged` is the first) is engine work like any base
game native, not a leftover to list. Creation Club plugins load as ordinary plugins through the
profile, their natives are added on the same call-count order when they appear.

| Item | Branch | Done check |
|---|---|---|
| DLC natives | `natives-dlc` | Every native declared by a DLC script (`Actor.SendVampirismStateChanged`, the `DLC1` and `DLC2` extensions to `Actor`, `Game`, `ObjectReference`, `Race`, `Werewolf` and `Vampire` events) is registered with a scenario, the first real `start` has zero unresolved calls, and a Dawnguard flow (`DLC1VampireTurn` through the state change) runs on the harness. Starts after `world-split` merges. |

Dirty plugins (2026-09-18): xEdit's Quick Auto Clean exists because the vanilla engine crashes on
a deleted reference or navmesh that something else still points at, and because identical-to-master
overrides silently undo a plugin in between. Bethesda's own Update, Dawnguard, HearthFires and
Dragonborn masters carry all three. The engine takes the cleaned semantics natively so nobody has
to clean anything: a deleted placed object stays resolvable and behaves as disabled at start, a
deleted navmesh is dropped with its edge links repaired to the neighbours, an identical-to-master
override is ordinary data (the load order already applies it as such). The gated runs load the
uncleaned masters as shipped. The validation runner reports counts per plugin for authors.

| Item | Branch | Done check |
|---|---|---|
| Deleted record semantics | `dirty-records` | A deleted `REFR` or `ACHR` winner resolves and is disabled at start (the cell index keeps it, flagged), a deleted `NAVM` drops with edge repair, the counts of identical-to-master overrides, deleted references and deleted navmeshes are reported per plugin, tested on the four uncleaned vanilla masters and on a synthetic dirty plugin. |

## Tooling survey, 2026-09-18, and the items it corrects

`../research/open-source-tooling.md` surveys what the open mod tool ecosystem already solves, per
subsystem, after the user's rule that nothing gets reimplemented when tooling exists. Its leading
finding is about us, not about the ecosystem: `../engine-refs/` already held about 50 repositories,
and several of them answer questions this engine had since solved by hand. Read that document and
the corpus before opening any plan item below.

Four items change. The done checks stand, the route to them does not.

| Item | Was | Now |
|---|---|---|
| `behavior-2` | Derive the unimplemented `hkb` classes from the Project Anarchy headers | `serde-hkx` is already cloned, MIT or Apache-2.0, last commit 2026-08-11, and carries 680 generated Havok class definitions of which 204 are `hkb`, each with the C++ name, signature hash, size per pointer width, and per field the ctype, both byte offsets, type size and serialization flags. Every class branch `behavior` left out is in there, `hkbBehaviorReferenceGenerator` and the modifiers included. The layouts become a port. The evaluation semantics stay ours, so the map's behavior risk narrows to semantics rather than disappearing. |
| `psc` | Write a Papyrus source compiler | `Caprica` is already cloned and is a complete open Papyrus compiler under MIT. Use or port it, do not write a second one. |
| `lod` | Build a reader for the BTO and BTR LOD formats | BTO and BTR are NIF files under different extensions, so consumption is two extensions on the existing `nif` reader plus the LOD specific blocks. Generation stays closed with no open alternative. |
| `nif` block definitions | Hand written per block | `nifxml` is already cloned and declares the block layouts, so the readers could be generated from it instead of maintained by hand. |

Also corrected: SkyUI carries no licence file at all and its last commit is 2015, so it is readable
and not copyable, which is firmer than the map's "licence unconfirmed". Licence fragmentation in
the risk register softens, three flagged areas now have permissive options.

Gaps the survey confirms as genuinely open, with nothing to reuse: FaceGen runtime TRI, EGM and EGT
blending, LIP decoded into phoneme timing (every tool copies it as an opaque blob), LOD generation,
a GFx capable Flash runtime, the `hkb` evaluation semantics, and ENB's hook protocol.
