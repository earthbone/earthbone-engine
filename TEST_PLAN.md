# Test plan audit (2026-09-18)

Phase 1 of the test hardening effort from `ENGINE-PLAN.md` round 5. This is an
audit, no test was written for it. It maps what each crate takes from
untrusted data, where the unsafe, concurrent and foreign code is, what the
suite covers today, and which techniques pay for themselves here. The table in
section 3 is the deliverable, the rest is the evidence behind its rows.

Numbers in this file were measured on the `test-plan` worktree at master
`67e94dd` (cell index merged, batching, behavior, packages and natives-2 still
on branches).

## 1. Inventory

### 1.1 Untrusted inputs, by crate

Every byte a mod author ships is untrusted. The engine reads it, an SKSE
plugin never runs, so the parsers are the attack surface and the VM is the
second one.

| Crate | Untrusted input | Entry points | Bytes reach |
|---|---|---|---|
| `esm` | plugins (`.esm`, `.esp`, `.esl`), load order | `Plugin::parse`, `LoadOrder::add`, `CellIndex::build`, every `records::*` view | zlib inflate of compressed records, group recursion, FormID resolution across the master list |
| `bsa` | BSA v104 and v105 archives, BA2 GNRL and DX10, loose files by path | `index_archive_in`, `Archive::open`, `read_record`, `Vfs::read`, `add_loose_root` | zlib and lz4 inflate with a declared size, path hashing, folder and file name tables, the loose overlay walks the file system |
| `nif` | meshes 20.2.0.7 at BS version 83 and 100 | `parse`, `mesh::build` | block type table, string table, per block payloads, index and vertex counts that size allocations |
| `havok` | hkx packfiles, animations, behavior graphs | `Packfile::parse`, spline decoding, graph model, the interpreter under construction | section layout, object fixups, spline compressed tracks with bit packed quantisation |
| `dds` | textures | `Dds::parse`, `decode`, `encode` behind a feature | header arithmetic (width, height, mips, array size, pitch), BC1 to BC7 block decode |
| `papyrus` | PEX compiled scripts, the SOEV save stream | `pex::parse`, `Vm::run`, `Vm::save_state`, `Vm::load_state` | string table indices, opcode operands, jump targets, the whole VM state on load |
| `game` | VMAD from plugins, SOEW and SOEG save streams, scenario files | `Game::bind`, `Game::load_state`, `harness::run` | script property values typed from plugin bytes, object graph on load |
| `physics` | `bhk*` collision blocks inside NIF | `bhk::decode`, `shape::collect`, `SimpleWorld` | shape counts, MOPP wrapped shape, compressed mesh chunks |
| `navmesh` | NAVM records, NVNM triangles and edges, terrain for generation | `Graph::build`, queries, `generate` through `rerecast` | triangle indices, edge links to other meshes by FormID, portal links |
| `profile` | `plugins.txt`, `Skyrim.ini` and overrides, MO2 `modlist.txt` and profile folders, `ccc` | `plugins_txt::parse`, `ini::layer`, `mo2::instance` | text decoding through `encoding_rs`, CRLF and BOM, asterisk enable flags |
| `godot-host` | everything above through `DataSource`, Godot's own calls into the extension | `DataSource::open`, `stream_cell_id`, `CellStreamer` node methods | all parsers on worker threads, the node tree |
| `commonlib-shim` | none yet, an MIT skeleton of the SKSE surface | | |
| `trace` | none, spans from our own code | `span!`, `write_chrome` | |
| `bench` | the installed game through the crates above | `soe-bench <phase>` | |

Save streams are ours, but a save is a file on disk that a user edits, copies
between versions or truncates on a crash. `load_state` therefore counts as
untrusted input on the same footing as a plugin, and it has no fuzz target
today.

### 1.2 Unsafe, concurrency, foreign code

Unsafe. The workspace forbids `unsafe_code`. One occurrence exists:
`crates/godot-host/src/lib.rs:31`, the `unsafe impl ExtensionLibrary` gdext
requires. Nothing else in the tree dereferences a pointer. This shapes the
plan: memory unsafety tooling (Miri, ASan, MSan) has nothing to bite on until
a real FFI lands.

Concurrency. No `rayon`, no channels, no atomics. Three `std::thread::scope`
fan-outs with a join at the end: master parsing and static building in
`godot-host/src/data_source.rs` (lines 232 and 436), the load order parse in
`bench/src/main.rs:215`, and a trace self test. Shared state is one global
`Mutex<Vec<Event>>` in `trace` and an `RwLock<HashMap>` string interner in
`commonlib-shim/src/util.rs`. The VM and the world are single threaded by
contract, `Rc<RefCell<World>>` in `game/src/lib.rs:114` and `Rc` values in
`papyrus/src/value.rs`. The one concurrency risk today is ordering: worker
output is collected back in master order (`data_source.rs:213`) and that
invariant is asserted nowhere.

Foreign code. godot-rust gdext 0.5.5 behind the `godot` feature, `api-4-4`.
With the feature off the crate is pure Rust and that is what the tests and
the benchmark link. Jolt is not linked (`docs/physics.md`, no CMake on this
machine), `SimpleWorld` is plain Rust behind the `PhysicsWorld` trait. The
Godot boundary is therefore the only FFI and it is exercised only by a
running Godot binary (`godot/headless_test.gd`, `godot/bench.gd`).

Compression and decoding dependencies that process untrusted bytes:
`flate2` (esm, bsa), `lz4_flex` (bsa), `encoding_rs` (profile), `image_dds`
(dds, behind `encode`), `rerecast` (navmesh generation). These are the
places where a declared size drives an allocation.

### 1.3 What the suite covers today

318 `#[test]` functions across 49 suites, gate `tools/lint.sh` (fmt, clippy
pedantic with the allow list, comment-lint, prose-lint, `cargo test
--workspace`). Per crate: esm 38, dds 36, bsa 32, papyrus 29, godot-host 28,
game 27, physics 25, profile 23, havok 22, nif 22, navmesh 21, commonlib-shim
10, trace 3, bench 2.

Kinds present:

- Synthetic fixtures written by the tests (every crate), one regression test
  per fuzz crash (`docs/fuzz.md`, five, embedded bytes).
- `SKYRIM_DATA` gated corpus runs: every plugin, archive, NIF, HKX, DDS and
  PEX in the installed game is parsed by a `real_files.rs` or `real_data.rs`
  test per crate, numbers in each `docs/<crate>.md`. This is already a corpus
  run over vanilla, it lacks a mod corpus and it lacks a decode pass on every
  texture (the dds live run samples).
- Round trips: havok `round_trip.rs` (writer to parser), dds `bc7_roundtrip`
  (encode to decode), esm `Plugin::to_bytes` used inside `hundred_plugins.rs`
  and `cell_index.rs`, VM and world save and load (`quest_flow.rs` resumes a
  quest mid flow).
- Load order semantics: `hundred_plugins.rs`, a synthetic Wabbajack shaped
  order, winner is the last plugin to touch the form.
- Fuzzing: six `cargo-fuzz` targets, ten minutes each on 2026-09-17, plain
  bytes, no `arbitrary`, no save target, no VM execution target.
- Validation harness: `crates/game/src/harness.rs`, scenario files with
  `expect` lines and `traces-match` against a captured `Papyrus.0.log`. One
  self test, no real capture committed yet (needs the user at the game).
- Performance: `soe-bench` phases, trace spans, `docs/performance.md`.
  Numbers are medians and minimums, no percentiles, no gate.
- Replay design for the behavior interpreter (`docs/havok.md` 5.4), not built.

Kinds absent: property based, stateful model based, differential against an
external tool, fault injection, mutation score, coverage report, determinism
hash, instruction count gate, any tiering (everything runs on every commit,
gated tests skip when `SKYRIM_DATA` is unset).

One stale note: `docs/papyrus.md` "Not done" still opens with the `None`
call name defect that branch `vm-callname` fixed on 2026-09-18. Phase 2 should
drop that bullet when it touches the file.

## 2. Failure modes the engine actually has

Ordered by how they would show up for a user.

1. A crafted or merely unusual mod file panics or exhausts memory in a parser.
   Proven class: five of five fuzz finds were declared counts driving an
   allocation or a shift.
2. The load order resolves a form to the wrong winner or the wrong cell, so a
   patch silently loses. Everything downstream reads through this table.
3. The VM diverges from vanilla on a script (wrong dispatch, wrong state
   precedence, wrong string or float semantics), so a quest sticks. Proven
   class: every vanilla call dispatched as `None` for three days without a
   test noticing, because no test executed vanilla bytecode against natives.
4. A save does not load back into the same state, or loads back into a
   state that then diverges. No test writes a save at an arbitrary tick.
5. Two runs of the same input differ (hash map order, thread join order,
   float accumulation), which makes every other bug unreproducible.
6. Hostile bytecode hangs the scheduler or grows the stack without bound.
7. A truncated archive, a missing master or a disk error in the middle of a
   save corrupts state instead of failing cleanly.
8. Frame time or stream time regresses without anyone noticing, since numbers
   are recorded by hand in Markdown.

## 3. The plan

Risk is the damage when it happens (1 to 5), likelihood is how often the
current code and suite would let it through (1 to 5). Cost: S under a day, M
one to three days, L a week. Priority follows the product, ties broken by
cost.

| Component | Failure modes | Technique | Tool | Expected bug class | Cost | Risk x likelihood | Priority |
|---|---|---|---|---|---|---|---|
| `papyrus` VM executing bytecode | hostile or unusual PEX hangs, recurses, indexes out of range, diverges from vanilla | Structure aware fuzzing of the VM, not just the loader: `arbitrary` derived instruction streams assembled with `asm::ScriptBuilder`, run under an instruction and stack budget against the mock natives | `cargo-fuzz` plus `arbitrary`, new target `fuzz-vm-run` | operand validation gaps, unbounded recursion, scheduler livelock, integer edge cases in `value.rs` | M | 5 x 5 = 25 | P1 |
| `esm` load order and winner rules | wrong winner, wrong cell after an override, ESL slot arithmetic, master index out of range | Property based over random load orders (masters, light plugins, overrides, deletions, moved references) checking last-writer-wins, `CellIndex` agreement with a brute force scan, resolve round trip through `Plugin::to_bytes` | `proptest` in `crates/esm/tests`, seed printed on failure | index arithmetic, override precedence, deletion handling | M | 5 x 4 = 20 | P1 |
| `papyrus` and `game` save streams | a save from a crash, an older build or a hand edit panics on load, or loads into a state that diverges | Two halves. Round trip property: save at a random tick of `quest_flow`, load into a fresh game, save again, the bytes must match. Fuzz target on `load_state` for SOEV, SOEW and SOEG with `arbitrary` driven field mutation of a valid save | `proptest`, `cargo-fuzz` target `fuzz-save` | missing fields (the NPC level and race were missed once already), length prefixed reads without a cap, dangling object ids after load | M | 5 x 4 = 20 | P1 |
| `game` and `papyrus` determinism | two runs differ, replay impossible | State hash per tick (world forms touched, VM threads, timers, RNG), the same scenario run twice must produce the same hash sequence, and the hash goes into every failure message. Pairs with the `determinism` round 5 item that adds record and replay | in tree, `std::hash` over a canonical ordering, no crate | `HashMap` iteration order leaking into behaviour, join order from worker threads, float accumulation in the clock | M (hash) then L (replay) | 5 x 4 = 20 | P1, after `packages` and `natives-2` merge |
| `esm` `Plugin::parse` | crafted record or group layout panics, inflates a bomb, recurses deep | Structure aware fuzz with a grammar of groups, records and subrecords through `arbitrary`, metamorphic parse to `to_bytes` to parse equality, a decompression ratio cap test | `cargo-fuzz` plus `arbitrary`, `proptest` | declared size allocations (one fixed), group nesting depth, subrecord length overflows, `XXXX` extended length handling | M | 4 x 4 = 16 | P1 |
| `bsa` archive index and read | declared sizes drive inflate, path normalisation lets two names collide, loose overlay picks the wrong file | Property based on synthetic archives (`sse_bsa` builder exists): every written file reads back, hashed lookup equals linear lookup, overlay precedence matches load order. Fault injection on truncated and short read files | `proptest`, `fail` crate points in `read_record` and `Vfs::read` | inflate size trust, case and slash normalisation, truncated records returning partial bytes as success | S | 4 x 3 = 12 | P1 |
| `dds` decode | header arithmetic overflow, block decode out of range, wrong pixels | Keep the fuzz target nightly (it found four of five bugs). Differential decode against `image_dds` on every texture in the installed game, not a sample, pixel exact for BC1 to BC5 and within one unit for BC7 | `cargo-fuzz`, gated test extending `real_data.rs` | wrong block mode tables, partial block edge handling, mip size arithmetic | S | 3 x 4 = 12 | P1 |
| `nif` parse and mesh build | index out of range in mesh arrays, NaN vertices, wrong triangle count | Invariant checks on the neutral mesh over the whole installed corpus (every index below vertex count, no NaN, no degenerate scale), structure aware fuzz seeded from the block grammar | gated test, `cargo-fuzz` plus `arbitrary` | index validation after parse, unmodelled block types sizing the stream wrongly | S | 4 x 3 = 12 | P2 |
| `papyrus` loader against a reference | wrong string table or debug info decoding | Differential: decompile every vanilla PEX with Champollion, compare function names, parameter lists and opcode counts with the loader's view | Champollion from `engine-refs`, gated test, one JSON dump per script cached under the scratch dir | off by one in the string table, debug info skipped wrongly | S | 3 x 3 = 9 | P2 |
| `esm` records against a reference | a typed view reads a field at the wrong offset | Differential: xEdit dump of Skyrim.esm for the record types the views cover (QUST, NPC_, PACK, CELL, REFR, LVLI) compared field by field. Needs one xEdit dump made by the user on Windows or under Proton, then it runs unattended | xEdit dump file under `SOE_XEDIT_DUMP`, gated test | wrong subrecord versions, missed optional fields, flag bit positions | M plus one manual session | 4 x 2 = 8 | P2 |
| `game` quest state | stage order, objective state, alias fill diverge from vanilla | Stateful model based: a small model of a quest (stages, objectives, aliases) driven by random operations, the engine must agree with the model. Then a scenario per native as the CK wiki review lands | `proptest` state machine, the harness | stage guard logic, objective display flags, reset and stop semantics | M | 4 x 2 = 8 | P2 |
| `havok` interpreter | pose diverges from the clip sampler, state machine transitions fire wrongly | The replay harness from `docs/havok.md` 5.4 with synthetic captures first, then an SKSE side capture when the user records one | in tree, the `behavior` fork lands the interpreter | blend weight normalisation, transition timing, event ordering | M, blocked on `behavior` | 4 x 2 = 8 | P2 |
| `navmesh` graph and queries | asymmetric edges, path through a missing portal, generation drifts from vanilla walkable area | Properties: edge symmetry, every path step adjacent, path reversal gives a path, portal links resolve. Coverage comparison between generated and vanilla NAVM per cell as a percentage | `proptest`, gated test | link resolution across cells, generation parameters | S | 3 x 2 = 6 | P2 |
| `physics` `SimpleWorld` and hulls | hull with a point outside it, ray misses a hit, capsule tunnels at large steps | Properties: every input point inside the hull within epsilon, translate world and ray together gives the same hit, capsule sweep at 1 fps step against a thin wall must stop | `proptest` | hull face orientation, sweep substepping | S | 3 x 2 = 6 | P2 |
| `profile` text parsers | BOM, CRLF, mixed encoding, comments, asterisks mis-parsed | Property based on generated `plugins.txt` and INI text with random line endings and encodings, layering precedence as a property | `proptest` | encoding fallbacks, whitespace trimming | S | 2 x 2 = 4 | P3 |
| `godot-host` pure layer | streaming ring loads the wrong cells, mesh arrays wrong lengths, worker results out of master order | Properties on `streaming.rs` (the loaded set equals the ring, hysteresis never thrashes), an assertion that `build_all` output order equals input order, the exterior benchmark as a percentile gate | `proptest`, `soe-bench`, `godot/bench.gd` | off by one at ring edges, join order | S | 3 x 2 = 6 | P2 |
| `godot-host` Godot boundary | extension fails to load, a node method panics inside Godot | Headless smoke run nightly (`headless_test.gd` streams one cell and prints counts), it is the only test that links gdext | Godot 4.4.1 binary under `tools/godot/` | ABI drift after a gdext or Godot upgrade | S | 4 x 1 = 4 | P2 |
| `esm` cursor and `dds` header arithmetic | bounds and overflow in the two small pieces every parser goes through | Bounded model checking on `cursor.rs` (`take`, `u32`, `zstring`) and `dds::header` (`mip_count`, `layer_count`, `slice_bytes`) only | Kani | overflow in size arithmetic before the cap | S | 3 x 2 = 6 | P2 |
| `papyrus` opcode semantics, `esm` winner logic | a refactor changes a semantic and every test still passes | Mutation testing scoped to `vm.rs` dispatch and `value.rs` arithmetic, and `load_order.rs` plus `cell_index.rs` | `cargo-mutants` with `--file`, nightly | weak assertions, untested arms | S to run, M to act on | 3 x 3 = 9 | P2 |
| parsers and VM | a performance regression lands unnoticed | Instruction count gates on `Plugin::parse` of a synthetic plugin, `pex::parse`, `Vm::run` of the hot loop, `Dds::decode` of one BC7 texture. Stable across machines, so it runs in CI | `iai-callgrind` | accidental quadratic passes, allocation in the VM slice | S | 3 x 3 = 9 | P2 |
| stream and frame time | hitches | Percentile gates (p50, p99, p99.9, worst) on the Bannered Mare stream and the Whiterun exterior once the `batching` fork lands them, weekly, on this machine only | `soe-bench`, `godot/bench.gd` | material cache rebuilds, unbatched draw calls | S once the numbers exist | 3 x 3 = 9 | P2 |
| IO paths in `bsa`, `esm`, `game` save | truncated file, missing master, disk error mid save | Fault injection: failpoints on file open, read and write, save written to a temporary file and renamed, a kill between the two must leave the old save intact | `fail` crate, `tempfile` | partial reads accepted, save clobbered on error | S | 4 x 2 = 8 | P2 |
| VM under load | a script overload starves the rest | Chaos scenario: a thousand scripts registering for update every tick, a runaway `While` loop, a latent call that never returns, the scheduler budget must hold and the trace must name the offender | in tree scenario tests | budget accounting, thread starvation | S | 3 x 2 = 6 | P3 |
| whole engine, mods | a popular mod's files break a parser | Corpus run over a mod directory the user points `SOE_MOD_CORPUS` at (the top Nexus mods, never committed), every parser, weekly | gated test, same shape as the vanilla live runs | formats vanilla does not use (BC7 arrays, custom NIF versions, ESL flags) | S, the corpus is the user's to install | 4 x 3 = 12 | P1 for the harness, the corpus is manual |
| coverage | untested branches unknown | `cargo-llvm-cov` report nightly, published as a number in `TEST_LOG.md`, no gate | `cargo-llvm-cov` | none directly, it aims the others | S | 2 x 3 = 6 | P3 |
| dependencies | a vulnerable or unlicensed crate | `cargo-deny` (licences, advisories, duplicates), `cargo-audit` | `cargo-deny`, `cargo-audit` | GPL and MIT split violated by a dependency, advisories in `flate2` or `lz4_flex` | S | 2 x 2 = 4 | P3 |

## 4. Not worth it here, and why

- **loom and shuttle.** No lock free code, no channels, three scoped
  fan-outs that join before any result is read, one global mutex around a
  vector. loom models interleavings of atomics, and there are none. Revisit
  when a job system or a lock free trace buffer lands.
- **Miri.** The only unsafe block is the gdext entry, which is FFI Miri cannot
  execute. On safe Rust Miri finds what the borrow checker already forbids.
  Worth it the day Jolt or a memory map lands.
- **ASan, MSan, TSan.** Same reason. Sanitizers on a workspace with
  `unsafe_code = "forbid"` cost a nightly toolchain and a slow build for
  nothing. Add TSan and ASan with the Jolt binding.
- **Kani beyond the two small modules.** Parsers loop over records, and
  bounded model checking over those loops explodes. The cursor and the DDS
  size arithmetic are straight line code, everything else is fuzzing's job.
- **Visual regression with SSIM.** The renderer is Godot's, not ours. What the
  engine controls is node count, triangle count, material set and transforms,
  and those are asserted as numbers already. An SSIM check needs a GPU and a
  window and catches Godot's changes, not ours. Keep the captures under
  `docs/images` as documentation, not as a test.
- **Cross platform state hash comparison.** One target platform today.
  The determinism work produces the hash, comparing across platforms waits
  for a second one.
- **A 24 hour soak.** There is no game loop that runs for 24 hours yet, only
  the VM scheduler and the streaming ring. A ten minute run of the scheduler
  under `dhat` with instance and thread counts checked at the end finds the
  same leaks. Grow it to a soak when packages and AI give the loop something
  to do.
- **A thousand actor stress.** Blocked on the `packages` fork. Until actors
  do something, a thousand of them measure `HashMap` insertion.
- **Mutation testing across the whole workspace.** Record layout code is
  thousands of field reads, and a mutant that swaps two of them survives
  unless a real file test reads that field, which is slow and mostly an
  equivalent mutant hunt. Scope it to the logic dense modules named above.
- **Differential against real Skyrim VM traces as an automated tier.** The
  harness supports it, but a capture needs the user at the game once per
  scenario. It stays a manual tier 3 step, and each capture, once committed,
  runs in tier 1 forever.
- **Snapshot tests with `insta` on parsed records.** A snapshot of Skyrim.esm
  output is a copy of game data in the repository. Snapshots of synthetic
  fixtures add little over the assertions the tests already make. Skip.
- **1000 fps chaos as a separate harness.** The clock is virtual and delta
  based. A property test that ticks the same scenario with deltas of 0.001,
  0.016 and 1.0 seconds and checks the same end state covers it in tier 1.

## 5. Tiers

Tier 1, every commit, under five minutes. The current `tools/lint.sh` gate
plus the new synthetic property tests with a fixed case budget (`proptest`
at 256 cases, the seed printed on failure, `PROPTEST_CASES` raises it
locally), the save round trip property, the failpoint scenarios, the
determinism hash comparison, and every committed vanilla capture in the
harness. No `SKYRIM_DATA`, no network. `lint.sh` prints its wall time so the
five minute claim is measured at every run rather than assumed.

Tier 2, nightly. `SKYRIM_DATA` live runs (the existing gated tests plus the
full texture decode differential and the NIF invariants over the corpus),
`cargo-fuzz` ten minutes per target including the two new ones, `proptest`
at 10,000 cases, `cargo-mutants` on the scoped modules, `iai-callgrind`
gates, Kani on the two modules, the headless Godot smoke run,
`cargo-llvm-cov`, `cargo-deny` and `cargo-audit`. Output appended to
`TEST_LOG.md` by the runner.

Tier 3, weekly, this machine. Fuzz one hour per target, the mod corpus run
when `SOE_MOD_CORPUS` is set, the stream and frame percentile gates through
`soe-bench` and `godot/bench.gd`, the ten minute scheduler soak under `dhat`,
and the differential runs that need a user made dump (xEdit, Papyrus
captures) whenever a new one exists.

Failure output rule for every tier: the seed, the tick or byte offset, and a
one line command that reproduces it, no test passes without printing those
on failure.

## 6. The first five commits of phase 2

One technique on one component each, tier 1 unless said otherwise, each
followed by a `TEST_LOG.md` entry (what was added, what it found, what it
cannot catch).

1. **`esm`, property based load order.** `proptest` generators for a master,
   light plugins and patches that override, move and delete references,
   checking last-writer-wins, `CellIndex` against a brute force scan, and the
   `to_bytes` to `parse` round trip. Adds the seed printing helper every
   later property test reuses.
2. **`papyrus`, structure aware VM fuzzing.** `fuzz-vm-run`: `arbitrary`
   derived instruction streams through `ScriptBuilder`, run under an
   instruction and stack budget against the mock natives, plus the `fuzz-pex`
   target switched to the same grammar. Nightly tier, with the first ten
   minute run logged.
3. **`game` and `papyrus`, save round trip at a random tick.** Save during
   `quest_flow` at a `proptest` chosen tick, load into a fresh `Game`, save
   again, bytes equal, then the loaded game finishes the quest. With the
   `fuzz-save` target on `load_state` for the three streams, nightly.
4. **`bsa` and `game` save, fault injection.** `fail` points on archive read,
   loose read and save write, truncated archive and short read scenarios,
   the save written through a temporary file and a rename so a failure
   between the two keeps the previous save readable.
5. **`papyrus` and `esm`, mutation score.** `cargo-mutants` on `vm.rs`,
   `value.rs`, `load_order.rs` and `cell_index.rs`, surviving mutants listed
   in `TEST_LOG.md` as the gaps commits six onward close. Nightly tier.

The determinism hash follows as commit six once the `packages` and
`natives-2` branches are on master, since both edit `world.rs` and the hash
must cover the fields they add.
