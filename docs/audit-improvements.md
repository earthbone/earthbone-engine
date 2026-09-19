# Audit improvements

Branch `audit-improvements`, initially based on `75cfb55`. Audit commit
`3f336e3` follows the approved reference-fix commit `0c84bbe`. The reference
commit was fast-forwarded into `master` before reconciling the audit branch.
This is one review item covering the recommendations from the code audit.

## Scope and order

1. Host concurrency and correctness, `godot-host`.
   Extract an ordered, bounded scoped worker helper. Use it for masters,
   archives and mesh work. Return a typed worker error without shifting
   results onto another reference. Canonicalize texture cache keys through
   the VFS and test concurrent successful and failed loads.
2. Navigation queries, `navmesh`.
   Add a derived spatial index for mesh bounds, retain deterministic ties,
   avoid stale A* queue expansions, and check generated graphs against an
   independent shortest path oracle. Compare indexed point queries with
   exhaustive queries, including index invalidation after adding a mesh.
3. Readability, `papyrus` and `godot-host`.
   Split VM execution, dispatch, properties and scheduling into internal
   modules. Extract texture and exterior work from the data source. Share
   the Godot scenario lookup. Preserve public entry points and behavior.
4. Test coverage, `papyrus`, `navmesh`, `fuzz` and their format dependencies.
   Add generated PEX and VM continuation tests. Extend byte fuzz targets to
   audio, profile text, moddata text and VM state using existing parser APIs.
   Correct the existing save target's compression-sensitive oracle.
5. Measurement and build hygiene, `bench`, `nif` and `physics`.
   Add configurable performance gates and percentile reporting where the
   harness has real samples. Give the two dump examples distinct names.
   Run focused tests, the repository gate, and available real-data checks.

## Measurement rules

- Correctness must agree across one and multiple workers, including order,
  errors and archive precedence. Worker tests cover zero work, fewer items
  than workers, a configured limit of two, and a panic in a multi-item chunk.
- Spatial queries must agree with exhaustive queries, including equal-distance
  ties. A separated synthetic world must prune at least 90 percent of mesh
  candidates. This threshold is fixed before running the new test.
- Performance claims require before and after runs on the same inputs and
  build profile. Existing historical timings are context, not a new baseline.
- Lazy archive loading and a persistent pool are conditional experiments.
  Archive names must remain indexed eagerly to preserve override precedence.
  Any deferred payload work must be measured at first use as well as startup.
- No global allocator or runtime is added without an allocation or contention
  measurement. Allocation-count gates require a dedicated isolated harness,
  since other test threads and FFI allocations would distort a global count.

## References

- `docs/style.md`, the affected crate docs, `TEST_PLAN.md`, and
  `docs/performance.md` establish current contracts and previous measurements.
- Local `engine-refs/recastnavigation/Detour/Source/DetourNavMeshBuilder.cpp`
  and `DetourNavMeshQuery.cpp` describe bounding volume pruning and A* queue
  updates. The engine keeps its triangle graph and centroid semantics rather
  than adopting Detour's tile and polygon representation.
- `bsa::Vfs` and `docs/bsa.md` define last archive wins, loose file precedence,
  and canonical path normalization. Archive construction may run in parallel,
  but insertion into the VFS must follow the original archive list.

## Results

Implemented in this isolated worktree:

- Bounded, ordered host workers and parallel archive opening. Worker panic
  handling cannot shift later geometry onto another reference. Trace spans
  remain available. Archive precedence, worker limits and panic behavior have
  regression tests.
- Canonical texture cache keys, sorted missing-texture reports and concurrent
  success and failure tests. Exterior distance checks no longer overflow on
  extreme grid coordinates.
- Lazy mesh-bounds indexing, conservative mesh components and stale A* queue
  rejection. Three 192-case properties cover point queries, large coordinates
  and directed blocked paths. A synthetic pruning threshold is enforced.
- VM and data-source module splits, one shared Godot scenario lookup, and
  distinct NIF and physics example names. VM state validates frame-instance
  indices and sorts object, variable and latent-waiter maps before writing.
- A 128-case generated PEX and multi-object continuation test, four additional
  fuzz targets, valid seeds and a seed checker that fails on rejected seeds.
  The existing save fuzz target compares headers and decompressed bodies.
- Explicit timing limits, p99 and maximum reporting, and sample counts.
  Samples release their previous result before measuring the next. Cold and
  warm cell streams are measured separately.

At the audit commit, the VM parent module was 862 lines instead of 1,988. The
data-source parent was 590 lines, with textures, exterior work and workers in
focused modules.
Public entry points are preserved, apart from the new typed worker-error variant
and renamed command-line examples.

### Audit commit verification

The full repository gate exited successfully with 844 reported test successes
and zero failures. Installed-data tests skip in that tier 1 run.

| Gate step | Result |
|---|---|
| Workspace formatting | Passed |
| Clippy, library and binaries | Passed |
| Clippy, all features and all targets | Passed |
| Clippy, no default features and all targets | Passed |
| comment-lint | Passed, with access to its external cache database |
| Prose lint | Passed |
| taplo | Skipped, not installed |
| markdownlint-cli2 | Skipped, not installed |
| Fuzz workspace formatting | Passed |
| Workspace tests | 844 passed, zero failed |

Additional checks:

- Real Bannered Mare streaming passed with 690 references, 318 parsed meshes,
  124,924 triangles, 763 surfaces, 88 decoded textures and no missing textures
  or failed meshes. Warm streams at one, two and eight workers matched reference
  ownership, geometry and texture payloads.
- Tamriel's real graph passed: 13,011 meshes, 2,271,709 triangles and all 50
  sampled cross-mesh paths. Reachability covered 2,101,384 triangles.
- The Papyrus suite passed 48 tests with the installed data, including parsing
  14,026 scripts with 54,620 functions and zero failures.
- All fuzz binaries built with the locked dependencies. Seed generation and
  validation passed. Each new target completed 10,000 mutation smoke inputs.
  These stable builds lack coverage instrumentation and sanitizers. Nightly
  Rust and cargo-fuzz are not installed, so this is not a full fuzz campaign.
- The save target passed all 60 external saves as fixed inputs. No save was
  copied into the repository or modified.
- A misspelled performance metric failed the real benchmark command, as
  required. Unit tests cover invalid limits and exceeded limits.

Province path queries improved from 27.934 s to 6.135 s with 40 of 100 paths
found in both versions. Three separate-process startup pairs all improved on
the parallel path, with medians of 9.168 s before and 3.776 s after. Background
work and large timing variation prevent treating these as stable gates. See
`docs/performance.md` for samples, cache conditions and limitations.

### Dependency and integration notes

The property tests reuse proptest 1.11 already locked in the workspace. The new
fuzz crate dependencies are local engine crates. Their additional fuzz-workspace
transitives are hound and serde_json with its dependencies, already present in
the main workspace. Existing fuzz dependency versions are unchanged. No runtime
or persistent thread-pool dependency was added.

Lazy archive payloads, persistent workers and allocation counting are deferred
pending measurements. No Godot visual or interactive runtime test was performed.
The Godot-specific helper compiled under the all-features gate.

At the audit checkpoint, the independent `reference-fixes` worktree overlapped
the VM, VM state and Godot files and was inspected read-only. After the user
confirmed its handoff and approved both diff summaries, the reference fixes
were committed first and the audit changes second. Both original commits are
preserved.

## Reconciliation

The reference branch's three-clock wait behavior now lives in the audit's
`vm/scheduler.rs` and `vm/dispatch.rs` modules. Its clock fields, version 2 state
encoding, version 1 compatibility and hashing coexist with the audit's stable
map serialization and frame-index validation. Both host-documentation sections
are retained. Godot material caching, primitive shape handling and game native
corrections remain intact. No additional gameplay semantics were introduced.

The combined Papyrus/game test run passed 253 tests, including all wait-domain,
legacy-state and generated continuation regressions. The three all-feature
Godot primitive-geometry tests also passed. Fuzz targets rebuilt with locked
dependencies and their regenerated seeds passed validation. VM-state and
mod-configuration targets each passed 10,000 additional mutation smoke inputs,
with the same stable-build limitations recorded above.

The combined full gate passed with 855 reported test successes. All gate tools
ran, including Taplo and markdownlint from the reference workstream's temporary
installation. No lint step was skipped. Installed-game script binding and the
real-cell test also passed on the combined code. Bannered Mare retained 690
references, 318 parsed meshes, 124,924 triangles and 88 decoded textures, with
no failed meshes or missing textures. Geometry and texture data matched at
one, two and eight workers. The combined branch has not been rerun in an
interactive Godot session.
