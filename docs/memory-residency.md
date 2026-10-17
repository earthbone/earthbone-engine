# Installed render memory, PRF-39

The installed capture is `installed_memory_attribution` in godot-host. It runs
the production walkthrough on Xvfb :99, samples the Bannered Mare and the
Whiterun gate three times, and records a final clear. The declared cache
threshold is 64 MiB for each image cache. Repeated inn snapshots may grow by
at most 1 MiB after the first complete cycle. These thresholds precede the
first capture. Repeated inn mesh, texture, material and shader counts must
not grow after that settling cycle. Explicit clear must release all three
payload caches. Actor upload copies must be released, and repeated actor
paths must share a GPU texture RID at the gate.

`tools/memory_capture.py` is a Godot launch adapter for `SOE_GODOT_BIN`. It
requires `SOE_RSS_OUTPUT` and DISPLAY :99. It launches the run's existing
Godot wrapper, follows only that child's process tree, and samples VmRSS and
VmHWM from `/proc/<pid>/status` every 20 ms. The TSV retains each observation
and the JSON retains the observed peak VmRSS, the kernel's peak VmHWM and the
child exit status. Samples can miss brief RSS peaks, so both peaks are kept.
The existing timeout, audio isolation and installed-test lock stay in force.

The category capture includes retained CPU mesh attributes and indices,
pointer-deduplicated CPU image payloads by format, prepared and RGBA cache
bytes, completed LOD mesh bytes, collision source geometry, resource counts,
shader source bytes, loaded Papyrus PEX source bytes and Rust allocation
ownership counters. Godot provides aggregate video, texture and buffer
bytes. Upload payload bytes are estimates of requested GPU storage. Driver
alignment, shader binaries, material internals, Jolt's private heap, parsed
script heaps and Godot's internal CPU copies have no public per-resource
byte counter here. Their byte fields are null. PEX source sizes are input
sizes, not the allocation size of the parsed VM. A missing source remains
unknown rather than contributing a false zero to a measured total.

Mesh copies retained for batched reference moves, visibility updates,
expressions and terrain seam joins are functional owners. CPU image
attribution deduplicates Arc payloads within each report. Actor reports are
separate owners and must not simply be added to source reports if they share
an allocation. Process RSS, Rust allocation bytes and Godot video bytes also
overlap, particularly with the software Vulkan renderer. They cannot be
added to obtain a physical-memory total.

Local references read: Mudcrab main `268f9884f7acaaa0302425331064f7268aac9d3c`,
PR 224 `d7885ce3bcf6300863488d2f4e662df866603af6`,
`crates/engine/src/profiling.rs` CPU asset accounting and its test,
`crates/engine/src/metrics.rs` resource-count diagnostics, and
`docs/roadmap/02-profiling.md`. Issues 183 and 184 motivate fixed-scene
attribution. Issue 185 proposes content-hash invalidation rather than a
landed eviction implementation. The supplied comparison in
`soe-run/log/comparison/memory/memory.md` distinguishes those proposals from
main and PR 224. No executable-derived research was used.

The CPU accounting approach is adapted with the attribution in `memory.rs`
and the MIT license in `THIRD_PARTY_NOTICES.md`. Bevy's mesh allocator slab
counter and render-world asset extraction have no corresponding Godot
public API. Local Godot `class_image.rst` supplies `get_data_size`, and
`class_imagetexture.rst` states that image readback returns a copy. The
accounting captures format and bytes at upload and avoids that readback.
Godot resources and server RIDs replace Bevy handles. Collision
remains on bundled Jolt, with no Rapier code or dependency. The cache uses
the existing host audio cache's LRU and oversized-bypass contract rather
than introducing another asset system. Mod paths, load-order identity and
authored mip filtering retain their meaning.

Image caches retain at most 64 MiB each and 4,096 slots including misses.
Oversized images bypass retention while remaining valid for their consumer.
Single-flight prepared slots still coalesce concurrent requests. Pixel Vec
capacity is charged, not only length. BC upload payloads reserve the validated
mip-chain size once, avoiding excess capacity from geometric Vec growth. Terrain baking borrows shared RGBA
images rather than cloning every layer. Unsigned BC1, BC2, BC3, BC4, BC5 and
BC7 pass through to Godot. CPU terrain composition and unsupported signed
formats retain their required RGBA decode. Authored mip chains remain sampled
by the inspected material shaders, so this change does not discard them.

Completed LOD products retain at most 64 MiB of weighted payloads and 512
entries, including absent blocks. Each request builds its requested type,
with that type included in the load-order and block identity. Clear releases
completed products and both image caches. Mesh Vec capacity is charged and
shared textures are conservatively charged per surface in this cache budget.
The 512-entry ceiling is a chosen metadata bound rather than a measured
working-set optimum. Tests cover LRU, oversized results, zero-byte entries
and Arc owners surviving eviction.

Actor mesh loading resolves its material textures once, removing a duplicate
VFS read and image preparation pass. Player body, first-person arms and NPC
materials share uploaded images by the existing texture key within
one resolved source. A new source resets that cache. Material parameters
remain per actor. Actor surfaces release upload-only CPU texture and tint
buffers after submission. Geometry remains available for morph and skin
updates. Unused shared images are pruned by their Godot reference count.
The installed capture compares non-owning texture RIDs across actors and
checks zero retained actor image bytes. It creates no reference nodes and
adds no dependency or saved state.

## Installed baseline, 2026-10-13

The baseline is detached integration `735176dff` with attribution exports
only, before the retention changes. Its release extension SHA-256 is
`56cb8732c3626209b45d540daa0da3ccf6bdfe38378e5077a1b66230d733a9f5`.
The preserved extension and raw results live under `target/memory/before`.
All captures use Godot 4.4.1 on Xvfb :99 at 640 by 360, with software Vulkan
selected by the run's cargo wrapper. Installed runs execute one at a time.

| Run | Peak VmRSS GiB | Result |
|---|---:|---|
| Inn and gate attribution | 16.339 | Pass |
| PRF-35 magic | 11.533 | Pass |
| HST-23 rendered characters, confirmed rerun | 15.100 | Pass |
| HST-23 threaded snapshot | 6.135 | Pass |
| HST-23 headless frame budget | 11.691 | Fail |
| RND-19 LOD capture | 14.120 | Pass |

The original HST-23 rendered capture measured 14.928 GiB. Its interrupted
headless capture has no exit result, so the confirmed rerun supplies all
three exits. The unchanged headless timing gate fails at 18.0 ms p99 and
48.4 ms worst, against 16.7 ms and 33 ms. No timing limit is changed.

The following counts and bytes are the streamer's first fixed-scene
snapshots. CPU totals count retained payload owners. Mesh upload bytes
estimate submitted arrays. Texture upload bytes include the mip chain.
Materials and shaders have counts and shader source bytes, while their
private compiled allocations remain unknown.

| Category | Bannered Mare count | Bytes | Whiterun gate count | Bytes |
|---|---:|---:|---:|---:|
| Uploaded meshes | 319 | 10,276,064 | 13,850 | 185,361,052 |
| CPU mesh vertices | 319 | 8,496,560 | 30,396 | 221,389,824 |
| CPU mesh indices | 319 | 1,779,504 | 30,396 | 64,157,232 |
| GPU texture DXT1 payload | 79 | 60,191,464 | 102 | 152,440,184 |
| GPU texture DXT5 payload | 169 | 216,117,072 | 535 | 509,985,104 |
| GPU texture RGBA8 payload | 0 | 0 | 377 | 156,260,020 |
| CPU compressed BC1 faces | 99 | 60,191,464 | 187 | 172,698,344 |
| CPU compressed BC3 faces | 178 | 221,099,424 | 867 | 646,310,608 |
| CPU RGBA8 images | 2 | 2,097,152 | 501 | 151,274,112 |
| Materials | 113 | Unknown | 565 | Unknown |
| Shader source | 21 | 53,009 | 47 | 120,747 |
| Streamer collision shapes | 917 | 1,005,568 | 0 | 0 |
| Loaded Papyrus PEX sources | 14,302 | 17,650,570 | 14,302 | 17,650,570 |

The gate's collision geometry belongs to PlayerNode rather than the
streamer. The updated capture reports that owner separately. Godot's
material and shader heaps, Jolt internals, movement-world heap and parsed
Papyrus allocations are not derived from the source payload sizes.

Actor image owners retain another 270,798,872 bytes at the inn and
1,735,799,144 bytes at the gate. These are sums over actor owners rather
than a global allocation census. Godot's aggregate texture monitor reports
544,842,752 bytes at the inn and 2,629,301,376 bytes at the gate. The
streamer cache retains 970,283,064 image bytes and 199,621,080 LOD mesh
bytes after returning to the inn, including after its baseline clear.

## Capture commands

Build the release extension with the run's cargo wrapper first. Set
`SKYRIM_DATA` to the installed Data directory and `SOE_GODOT_BIN` to the
absolute path of `tools/memory_capture.py`. Set `SOE_RSS_OUTPUT` to a fresh
directory for each campaign. The wrapper selects Xvfb :99 and retains the
installed-test and release-build locks.

Run these targets separately through that cargo wrapper:

```text
cargo test -p soe-godot-host --test installed_memory_attribution -- --nocapture
cargo test -p soe-game --test it prf_35_installed_windowed_magic_budget -- --nocapture
cargo test -p soe-godot-host --test hst_23_characters -- --nocapture
cargo test -p soe-godot-host --test rnd_19_player_lod_capture -- --nocapture
```

## Validation and pending installed captures

Source checkpoint `155597a82eb9723bee87ea688a6cf03cced6e15e` is committed
on detached HEAD in the scratch worktree. The installed baseline is retained
separately. The changed release build completed in 5 minutes 55 seconds.
Its extension SHA-256 is
`2a87618fb2011bfcc7f602b35ace8d0f5cd92ebf781174deeefa6d27269579d0`.

The full `tools/lint.sh` run uses four compiler jobs and unsets installed
data and the optional Godot launcher for Tier 1. Its log is
`target/memory-gate.log`. Format, every clippy mode including all targets,
the logging facade, comment lint, prose, TOML, Markdown and fuzz formatting
pass. The 52 performance runner contracts pass. Cargo deny and cargo audit
are absent and reported as skips. Comment lint emits advisory findings.

The gate exits 101 in game integration tests, with 5,234 passes, 12 failures
and 11 existing ignored cases across the completed binaries. Their combined
reported test runtime is 153.65 seconds. Game integration itself reports
4,241 passes, 12 failures and four existing ignored cases in 10.43 seconds.
The failures are the unchanged AI-22 follow and package navigation fixtures
listed in the gate log. Neither game nor navmesh has a worktree change, and
game has no dependency on godot-host. No check is weakened or removed.

A separate godot-host library run passes all 440 cases in 0.54 seconds,
including the seven new cache and image-accounting cases. Its log is
`target/memory-host-lib.log`. The independent byte-cache run passes all
three eviction, oversized-slot and negative-cache cases. Gated cases that
return early without installed inputs are not installed acceptance evidence.

The changed installed attribution command was submitted twice through the
run cargo wrapper, but neither attempt reached cargo. Both owned waiters were
stopped before final reporting. A wrapper trace using installed data and
`cargo test --help` passes the 12 GiB memory admission floor with 22,707,668
KiB available, then blocks at `flock 8` on `installed-0.lock`. That diagnostic
exits 124 at its 15-second limit. It launches no Godot scene. The trace is
`target/memory-installed-wrapper.log`.

After VmRSS for PRF-35, HST-23 and RND-19 remains unmeasured. The new capture
has not yet established actor texture sharing, zero actor image retention,
repeated-cell stability or the under-8-GiB target on installed data.
`target/memory/after/binary.json` identifies the built extension. Release and
installed-test locks remain in force. The row remains `wip` pending those
captures and a passing integration gate.

## Preview cache sharing regression, PRF-39

The scratch preview at `abfb2665` retains RND-41's bounded distant demand.
A fresh single-test process on `363092378` peaks at 1.648 GiB for the installed
Whiterun preparation. The same isolated test on the merged preview peaks at
1.668 GiB and prepares the same eight admitted references, five drawable
surfaces and three markers. The previously reported 22.9 GiB peak does not
reproduce in that isolated process. Process high-water marks from a full suite
can include earlier tests and child processes, so these comparisons use exact
filters and one test thread.

The inn admission does reproduce the regression. `363092378` prepares
297,001,596 payload bytes and admits the inn. The preview prepares
525,750,308 bytes and rejects it under the unchanged 384 MiB limit.
Both preparations cover 697 references. The memory-port merge `d595ee31d`
replaced lifetime image sharing with a 64 MiB owning cache. Eviction removed
its lookup slot while prepared surfaces still owned the image, and later
references loaded additional copies. The synthetic live-owner eviction check
fails before the fix and passes afterward.

The prepared-image cache now retains weak image and in-flight slot handles
alongside its bounded owning cache. A live consumer shares the same image
after eviction, including oversized images. Once every owner releases an
image, a later request loads it again. Explicit clear also resets weak handles
and starts a cold load. Old loads cannot repopulate a cleared weak registry.
The owning byte and negative-slot limits remain 64 MiB and 4,096 entries.
Expired weak metadata is pruned at that slot threshold. Live weak handles
retain no image payload. The RGBA and completed LOD cache limits are unchanged.

Local OpenMW `components/resource/objectcache.hpp`, `GenericObjectCache::update`,
keeps externally referenced resources available for reuse. Its time-based
strong cache does not fit this byte budget, so weak ownership supplies reuse
without extending cache residency. No dependency, saved state or reference
node is added. Texture identity still follows the resolved source and VFS path.

`persistent_full_lod_installed_whiterun_preparation_peak_is_bounded` runs the
preparation in a fresh Linux child process and checks VmHWM against two GiB.
The limit was fixed before its first run. The child retains the existing
selected-model and marker assertions. Cache tests cover slot and byte
eviction, oversized sharing, expired owners, explicit clear and concurrent
aliases. Installed after-fix measurements and the commit gate are recorded
with the completed preview validation.

The after-fix isolated Whiterun worker peaks at 1.650 GiB, with five drawable
surfaces and the same eight admitted references. The inn payload returns to
297,001,596 bytes, exactly the visibility-branch payload, and admission passes
without changing its 384 MiB limit. The after-fix inn command peaks at
1.709 GiB, compared with 1.899 GiB before the fix and 1.751 GiB on the
visibility branch. All commands use one test thread. Eleven texture-cache
checks pass, including the previously failing live-owner eviction regression.
These are CPU preparation measurements. They do not certify the Godot scene's
RSS or its existing six-GiB target. The release scene requires its own capture.

## Full LOD arena attribution, 2026-10-15

The full LOD visibility check keeps its six GiB process RSS assertion. The
software Vulkan run includes memory a hardware driver would place on the GPU.
The diagnostic reports sample process RSS, Godot monitors, Rust live bytes
and glibc arena totals at the inn, city exit, gate, landmark and city return.
They do not call collection or GPU image readback while reporting.

The preserved default-allocator capture is `target/full-lod/run-peBBpa`.
The first new capture with the extension's two-arena cap is
`target/full-lod/run-drSqUC`. Godot exits zero and every rendering and release
check passes in that capture. The test exits 101 at the unchanged RSS assertion,
with a sampled peak of 13.322 GiB. The user supplied the earlier 17.9 GiB peak.
The earlier default capture below samples 17.757 GiB.

The following values are MiB, except object and node counts. Rust requested
bytes and Godot video bytes overlap process RSS. Arena free bytes describe
allocator ownership, rather than a separate amount to add to RSS.

| Phase | Default RSS | Two-arena RSS | Rust live | Godot textures | Godot buffers | Objects | Nodes | Orphans | Arena free |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| after_inn | 5301.3 | 4365.4 | 2623.7 | 282.0 | 17.7 | 2576 | 98 | 0 | 266.8 |
| after_exit | 12509.1 | 10694.8 | 4429.0 | 1001.4 | 169.1 | 19260 | 169 | 0 | 1810.4 |
| gate_settled | 16470.6 | 11704.8 | 3850.3 | 818.1 | 117.2 | 17798 | 103 | 0 | 2714.1 |
| landmark_done | 16507.5 | 11707.1 | 3850.3 | 818.1 | 117.2 | 17803 | 104 | 0 | 2534.5 |
| after_release | 17782.4 | 13289.0 | 4303.2 | 1001.4 | 168.7 | 19265 | 169 | 0 | 3578.8 |
| end | 17782.4 | 13289.0 | 4303.0 | 1001.4 | 168.7 | 19268 | 169 | 0 | 3579.9 |

At the gate, Rust live ownership decreases from the city snapshot while RSS
rises. The prepared inn occupies 297,001,596 bytes after the city exit and is
gone at the gate. The source image cache and completed LOD cache remain within
their 64 MiB limits. The final release means releasing Tamriel distant models
and returning to Whiterun, so city geometry and its collision remain live.
The node count returns to 169, with zero orphans.

The first-parent comparison with `363092378` changes neither the counting
allocator nor its header size. `57623d5e6` moves runtime cell attachment across
frames. `d595ee31d` introduces bounded owning caches, whose live-image sharing
regression was corrected at `5d8724475`. The matching live payloads and resource
counts across the earlier 13 and 18 GiB captures do not support attributing
five GiB of new retained data to a particular merge.

The source worker pool is unchanged in that comparison. Walker changes at
`5d4ab8a63` add required location preparation on the prefetch worker. Moving
large preparation from the main thread can change arena reuse, so this is
a candidate for changed retention. The captures and inspected diffs do not
prove it caused five GiB of growth.

Local sources read include Mudcrab `crates/engine/src/config.rs`,
`crates/engine/src/streaming.rs` and `crates/engine/src/profiling.rs`, plus
`/usr/include/malloc.h`. Mudcrab's inspected main uses a per-frame upload
budget and streamed-root retirement. Those are not a total asset byte cap
and do not replace this host's RID ownership. Existing Mudcrab CPU payload
attribution and its third-party notice remain in place. The glibc controls
use the local malloc API, with no added dependency or copied upstream code.

The allocator header stores payload provenance alongside the original system
in the same word. Source workers inherit the submitting provenance. Free and
reallocation retain it across threads. These counters group allocation sites,
not independent physical-memory pools. Source, cell preparation, graph, clips,
scripts, movement, gameplay and other counters sum to the requested Rust heap.

### Release and first-person changes

The Linux glibc host caps future arena creation at two during core extension
initialization. An explicit MALLOC_ARENA_MAX setting takes priority. Existing
arenas are not removed by the cap. Completed location replacement drops its
CPU cell and collider preparation owners before calling malloc_trim. The
call releases unused pages while preserving live allocations. Other libc
and operating-system hosts skip the glibc controls. The new
`location_heap_release` load trace has a declared 33 ms budget.

The child fixtures show that threaded allocations create more than two arenas
without the cap and no more than two with it. A separate fixture keeps small
anchors alive between large freed blocks and requires at least 128 MiB to
return to the OS. These limits were declared before the first run. All four
heap-control fixtures pass. No sampling thread, dependency, saved field or
reference node is added.

First-person loading previously built the third-person skeleton, graph, clips
and appearance, then replaced them. It now shares the existing race/project
metadata resolver with the third-person loader and constructs first-person
assets directly. The synthetic fixture has valid first-person assets and
absent third-person skeleton and graph files. It fails at the unused skeleton
read before the change and passes afterward. Local CommonLibSSE-NG
`include/RE/P/PlayerCharacter.h` names the separate firstPerson3D. The existing
HKX character/project reader supplies the metadata without a new parser.

The CPU-only probe holds the source, scripts, player rigs and NPC rigs in a
fresh process. Source requested bytes are about 1,277 MiB. Script loading adds
179 MiB, the player body adds 192 MiB, first-person arms add 55 MiB, and each
empty male/female NPC rig adds 148 MiB. Preparing the 114-cell Whiterun
location adds 831 MiB. Its 1,053 static bodies contain 371,946 primitives, and
building the broad phase adds 28.4 MiB. Dropping that location returns Rust
live ownership to its earlier level. The probe is not a rendered RSS result.

After arm preparation, the old probe retains 2,041 MiB of arena space. Direct
first-person construction retains 1,848 MiB, a reduction of 193 MiB. The final
rig's live size stays nearly the same. Its raw measurements remain in
`target/lod-cpu-probe.log` and `target/lod-cpu-city-probe-after.log`.

The earlier pass reports three default controls at 17.712, 17.757 and
18.141 GiB, and three MALLOC_ARENA_MAX=2 controls at 12.771, 12.872 and
12.896 GiB. The complete captures are `run-HAJvpj`, `run-peBBpa`,
`run-c4f3Yv`, `run-TD2wVX`, `run-PYM7Ku` and `run-IytgBl` under
`target/full-lod`. All six contain FULL_LOD_RESULT PASS. Their environment
classification comes from that pass's preserved heap-control notes. The
new two-arena capture independently confirms the lower range.

### Combined fix capture

The combined release build adds direct first-person preparation, explicit
location preparation drops, unused-page reclamation and allocation provenance.
Its capture is `target/full-lod/run-X6HFfX`. At the gate, the provenance counters
report the following requested live bytes. Values are MiB and sum to the Rust
heap, rather than to process RSS.

| Allocation site | After inn | After exit | Gate |
|---|---:|---:|---:|
| Shared source | 1281.4 | 1281.5 | 1281.8 |
| Gameplay | 698.3 | 1072.0 | 1159.9 |
| Scripts | 174.4 | 172.1 | 172.1 |
| Cell preparation | 193.3 | 470.0 | 72.1 |
| Actor clips | 164.9 | 430.0 | 433.1 |
| Actor graph | 51.9 | 502.2 | 141.1 |
| Movement | 1.7 | 32.1 | 56.0 |
| Other | 40.7 | 469.1 | 539.2 |

The combined capture's phase values are MiB, except counts. The unchanged
whole-process RSS assertion fails at 12.736 GiB. Godot exits zero and
FULL_LOD_RESULT PASS confirms the scene assertions. The test process exits
101 after 400.86 seconds. The sampled process peak is above the last in-scene
snapshot, so its texture and object counts are not captured at that exact
instant.

| Phase | RSS | Rust live | Arena free | Arena live | Mapped | Godot textures | Godot buffers | Objects | Nodes | Orphans |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| after_inn | 4091.2 | 2606.7 | 88.2 | 3479.3 | 313.1 | 282.0 | 17.7 | 2575 | 98 | 0 |
| after_exit | 10199.0 | 4429.0 | 1819.4 | 6639.9 | 441.1 | 1001.4 | 169.1 | 19259 | 169 | 0 |
| gate_settled | 11177.3 | 3855.4 | 2841.8 | 5963.5 | 610.1 | 818.1 | 117.2 | 17797 | 103 | 0 |
| after_gate | 11177.3 | 3855.5 | 2841.3 | 5963.9 | 610.1 | 818.1 | 117.2 | 17797 | 103 | 0 |
| landmark_done | 11181.0 | 3855.5 | 2612.1 | 6193.1 | 610.1 | 818.1 | 117.2 | 17802 | 104 | 0 |
| after_release | 12711.9 | 4303.2 | 3479.1 | 6742.3 | 569.1 | 1001.4 | 168.7 | 19264 | 169 | 0 |
| end | 12711.9 | 4303.0 | 3479.2 | 6742.2 | 569.1 | 1001.4 | 168.7 | 19267 | 169 | 0 |

The reported 17.9 GiB peak falls to 12.736 GiB. Against the new cap-only
capture, the combined change reduces the peak from 13.322 to 12.736 GiB.
That is a single rendered comparison, not a three-run control for each
individual change. The six GiB goal remains unmet.

The source stays shared across transitions. Plugin record bodies account for
about 913 MiB of its payload. `NpcActors.rigs` retains shared actor assets by
skeleton and behavior path until the source changes. Clearing NPC residents
queues their nodes for release but preserves this rig cache. The clip counter
includes player assets as well as those shared NPC rigs, so it cannot be read
as the cache's isolated size. Per-actor graph allocations fall substantially
at the gate. CPU cell preparation also falls, and the prepared inn is gone.

Gameplay is an allocation-site category covering Game startup, VM execution
and runtime form construction. Its persistent growth is measured, but these
counters do not isolate individual World maps or VM objects. Likewise,
subtracting requested Rust bytes from libc live totals does not identify a
native cache. Allocation headers, allocator rounding and native allocations
remain in that difference. Godot texture and buffer monitors explain only
part of it. The exact remaining native owners are unresolved.

The combined capture records `location_heap_release` calls of 47.5, 158.3,
182.6 and 156.3 ms, exceeding the declared 33 ms budget. Reclamation
therefore has a measured crossing cost. Free arena bytes can remain reported
after malloc_trim has discarded physical pages, so that counter alone does
not measure resident free pages.

The final checks pass 535 host library tests and 42 trace library tests.
Host and trace clippy checks with all targets and warnings denied pass.
Package formatting and the release extension build pass. Installed rendering
passes its scene checks and fails its RSS assertion. With SKYRIM_DATA set,
all four `data_source::full_lod_tests` checks pass in 427.36 seconds, including
the Godot drawing and release check. The installed
`actor::tests::installed_first_person_excludes_all_head_parts` check passes
in 31.84 seconds. Both commands exit zero. The workspace gate is reserved
for the lead. That earlier pass left its changes unstaged pending review,
with its two new backlog rows still wip.

## Continued full LOD investigation, 2026-10-17

The continuation uses the existing detached checkout at `a621c939b`. It does
not check out older revisions or create another worktree. The earlier
uncommitted fixes and captures above are retained as the starting point.
The current task authorizes commits on this detached integration checkout.
The lead owns the workspace gate, so validation here covers the touched
host and trace crates and the named installed checks.

### Fixed mmap threshold control

The environment-only control `target/full-lod/run-tzsPFX` uses the previous
release extension, SHA-256
`6a041d1924d1b0dfe61e9c2f397cbefb34b5b6db2d1926b40fbb474b8797b802`,
with `MALLOC_MMAP_THRESHOLD_=131072`. Godot exits zero and every scene check
passes. The unchanged RSS assertion fails at 12.272 GiB. This is one control,
not a new three-run campaign. The earlier default and two-arena controls
remain the six-run evidence for arena retention.

| Phase | RSS MiB | Rust requested MiB | Free arena MiB | Live arena and mapped MiB |
|---|---:|---:|---:|---:|
| after_inn | 4088.3 | 2622.6 | 108.6 | 3809.3 |
| after_exit | 8619.9 | 4428.9 | 404.2 | 7076.0 |
| gate_settled | 10225.0 | 3860.2 | 1637.1 | 6588.6 |
| landmark_done | 10228.1 | 3860.3 | 1444.9 | 6780.7 |
| after_release | 11247.0 | 4303.1 | 2112.4 | 7357.5 |
| end | 11247.2 | 4302.9 | 2112.7 | 7357.3 |

The local `mallopt(3)` manual describes glibc's adaptive mmap threshold.
Freeing a large directly mapped buffer can raise the threshold, placing
later temporary buffers in arenas. A fixed 128 KiB threshold keeps large
buffers eligible for independent release. Existing free lists can still
satisfy those allocations, so this is not a promise that every large buffer
gets a mapping. It adds kernel mapping and zeroing work. Arena retention
remains substantial in the control and the whole-process peak still exceeds
the budget. No cache, texture resolution, mesh demand or RSS measurement
is changed by the allocator control.

The host applies that threshold during core extension initialization and
honors `MALLOC_MMAP_THRESHOLD_` and `GLIBC_TUNABLES` overrides. The arena cap
also honors its matching glibc tunable. A child-process fixture compares
160 MiB of temporary buffers separated by live anchors at a raised threshold
and at the fixed threshold. The declared check requires at least 128 MiB
to return without `malloc_trim` in the mapped case. The raised-threshold
control must retain its pages, and both cases preserve their anchors.

### Cache ownership and additional attribution

`NpcActors.rigs` previously retained strong rig owners until the data source
changed, even after the last resident or pending consumer disappeared. It
now prunes entries without consumers during NPC reconciliation. A busy
decoder's mutex is skipped until a later frame. Active nodes and completed
loading results keep their Arc and continue to share the rig. The synthetic
fixture checks active sharing, pending ownership, a busy decoder and final
payload release. This is a render cache change, with no saved state or
new per-reference node.

Local OpenMW `components/resource/objectcache.hpp`, `GenericObjectCache::update`,
provides the external-owner cache policy. Its expiry delay does not fit the
full LOD memory target, so this host removes unconsumed rigs at reconciliation.
Mudcrab's streamed-root retirement and upload budgets were also read. Its
Bevy asset ownership does not replace this host's Arc and RID ownership.
The existing Mudcrab accounting attribution and MIT notice remain in
`memory.rs` and `THIRD_PARTY_NOTICES.md`. No new upstream code or dependency
is copied for the libc controls or rig pruning.

The phase report now includes glibc usable bytes for Rust allocations, with
the host header and allocator rounding. Reallocations and cross-thread frees
stay charged to their original owner. Other libc targets omit these fields.
Rig-cache entry and unconsumed-entry counts are reported alongside NPC
textures. A per-frame process-RSS peak stores Godot texture, buffer, object
and node monitors. Blocking operations can peak between frames, so the
independent 25 ms process sampler remains the budget measurement.

The local mallinfo(3) page warns about omitted secondary arenas. A controlled
worker fixture on this machine's glibc 2.43 compares `mallinfo2` with
`malloc_info` XML while 128 MiB remains allocated on a worker. The two arena
system sizes are 135,168 and 134,234,112 bytes. Their sum, 134,369,280 bytes,
exactly matches `mallinfo2.arena`. Thus the captured allocator totals on this
libc include the secondary arena. The raw comparison is
`target/arena-coverage.xml`. This does not establish coverage on older libcs.

The merge audit checked the first-parent changes for memory-port, stream-port,
play2, walker, console and debug views. Worker pool sizing is unchanged.
Console wireframe generation affects the Compatibility renderer according
to local Godot documentation, while this test uses Vulkan. Debug geometry
returns before allocating when its views are off. The confirmed memory-port
image-sharing regression remains attributed to `d595ee31d` and fixed by
`5d8724475`. The existing captures do not establish a separate five-GiB
retained payload introduced by a particular merge. Walker preparation on a
different thread remains a possible arena-reuse contributor, without causal
measurement on the older binary.

The subsequent call-path review narrows that statement. The script sets
`auto_step=false`, so it bypasses required background transition preparation.
The walker merge still changes the path this script does use. Commit
`5d4ab8a63`, merged by first-parent `4f30b2dfa`, changes nearby-door selection
from interiors to all load destinations and adds exterior speculation.
The earlier traces show a rejected Whiterun job with 839,651,176 payload
bytes and 371,946 collision primitives. Its 114 cells and reference count
pass the cheap admission checks. The loader then builds the complete location
before checking the 384 MiB payload and 100,000 primitive limits. At the
Tamriel gate this prepares city resources that cannot be retained and are
not part of the visible distant-model demand. This is a demonstrated source
of unnecessary allocation in the merge, not a measured explanation of the
entire five-GiB branch difference.
The rig cache and first-person loader already arrived in `8daccdb9d`, an
ancestor of visibility checkpoint `363092378`. Their retained ownership and
redundant preparation therefore predate the preview merges being compared.
They are baseline memory fixes, not evidence against a later merge.

### Incremental speculative admission

The speculative loader now checks cumulative mesh and pointer-deduplicated
image payload after each cell, and primitives after each static body. An
exceeded limit returns a typed error and drops the partial preparation before
loading later products. A single completed product can cross a limit before
its cost is known. This is not a strict bound on every transient allocation
inside a mesh or texture decoder. Required transitions do not apply these
speculation limits and retain their ordinary cell and terrain selection.

Payload accounting moves from the prefetch module to the location products,
using the same fields and image identity. The final admission checks remain
in place. The streaming callback gains an error-returning sibling so a budget
rejection can stop the cell loop, while existing callers keep their interface.
The synthetic three-cell fixture uses distinct texture files. A zero-byte
speculative budget must load only the first texture before rejecting.
The required path must load all three cells and textures. The installed
Whiterun check additionally requires rejection before reaching the measured
complete city payload, then verifies that required preparation still completes.
The native capture confirms rejection at 418,855,364 bytes before collision
construction starts, compared with the former 839,651,176 bytes and 371,946
primitives. The final completed cell can cross the existing 384 MiB limit.

Local OpenMW `apps/openmw/mwworld/cellpreloader.cpp` confirms that preloaded
products are owned separately from the live scene. Its cell-count and expiry
policy does not supply this host's byte admission, so the existing host
payload limits are applied during preparation rather than after it.

Actor material preparation still creates local CPU image caches before the
shared GPU upload cache is consulted. Pending NPC jobs can therefore hold
duplicate CPU images. This is an inspected allocation path, not a measured
share of the remaining installed peak.

### Uploaded face-detail ownership

The preview adds `ActorSurface.face_detail` for the authored FaceGen texture
slot 3. First-parent merge `7579a1930` brings this field from the characters
play fixes, including source commit `afab12a12`. The upload loop released
`surface.textures` and `face_tint` but kept
`face_detail` until the actor unloaded. Its memory report also omitted that
field. Earlier zero-image reports therefore did not cover this owner.
The common upload-image release method now drops all three groups, and the
actor report includes face detail. Geometry, skin maps and expressions remain
available for pose updates. Godot materials retain their uploaded textures.

The synthetic fixture shares one image among the regular bundle, face tint
and face detail. Releasing upload inputs must expire its Weak handle while
preserving the geometry and its sampled skin transform. The initial native
capture uses the preserved extension from before this face-detail correction.
Its binary and source snapshots remain beside the native allocation samples.

### Native allocation capture

The diagnostic run `target/native-lod/run-040939` preserves release extension
SHA-256 `6d38b503f7ce1c5f2d29c435f5f7c0deb9261eab916a568f6f52ef7c2fb34d95`.
It includes incremental speculative admission, rig pruning and the fixed
mmap threshold. It precedes the face-detail fix. The launch adapter only
profiles the visibility script, leaving the four ordinary full LOD checks
and installed first-person check unchanged. Those five checks pass.

A temporary native interposer samples live malloc allocations in 1 MiB
units, records large allocations exactly, and captures direct mmap callers.
SIGUSR2 requests a snapshot at the next allocation without a sampling thread.
Each snapshot retains allocation stacks, smaps and malloc_info XML. A
standalone fixture checks allocation, resize, free and mapping release.
The diagnostic files stay under target and are excluded from the commit.
This is an instrumented attribution run, with timing and memory overhead.
The ordinary visibility test still supplies the independent RSS verdict.

Rust usable-byte counters and the native sampler identify different
allocation paths. At the gate, Rust accounts for 3,929.5 MiB, sampled
lavapipe malloc ownership for 1,977 MiB, and sampled Godot ownership for
397.5 MiB. Their sum is 6,304 MiB, compared with 6,471.7 MiB of libc live
arena and mapped allocations. Sampling error, unobserved small native
allocations and allocator rounding prevent an exact identity.

Direct driver mappings are separate from those libc allocations. Whole VMAs
contained within a recorded lavapipe mapping account for another 2,222 MiB
of resident pages at the gate. This matching omits partially overlapping
VMAs and must not be presented as all driver mapping residency. Arena free
space is 1,783.8 MiB, with some pages potentially already discarded. Godot
reports 818.1 MiB of texture storage and 117.2 MiB of buffers. These are
overlapping resource estimates, not amounts to add to driver ownership.

The loaded driver is Mesa lavapipe 26.2.3, build ID
`f93300d4975a836b20e56bdcfe0587733e1de3d3`. Its private symbols and debug
information are absent. The samples identify the driver allocation owner,
but do not identify its individual internal shader or resource pools.
No driver pool is called a leak solely because it survives a cell change.

The following MiB values pair each native sample with the nearest requested
phase. They are not synchronized physical-memory partitions. Requested Rust
bytes and usable Rust bytes overlap, as do Godot monitors and driver memory.
The native heap columns are sampling estimates. Driver maps count matched
resident whole VMAs. No snapshot reports swap usage.

| Phase | RSS | Rust requested | Rust usable | Lavapipe heap | Godot heap | Driver maps | Arena free |
|---|---:|---:|---:|---:|---:|---:|---:|
| after_inn | 4101.3 | 2618.7 | 2816.6 | 683.0 | 123.9 | 441.0 | 105.7 |
| after_exit | 8650.5 | 4429.0 | 4662.5 | 1824.0 | 396.9 | 1536.7 | 374.7 |
| gate_settled | 10127.5 | 3695.0 | 3929.5 | 1977.0 | 397.5 | 2222.0 | 1783.8 |
| landmark_done | 10131.5 | 3695.0 | 3929.5 | 2108.0 | 410.5 | 2222.0 | 1627.7 |
| after_release | 11216.6 | 4299.3 | 4535.1 | 2122.0 | 434.8 | 2222.4 | 2058.4 |

Near the return peak, `native-09.tsv` estimates 3,954 MiB of lavapipe heap,
447.8 MiB of Godot heap and 2,222.1 MiB of matched resident driver mappings.
That snapshot is taken near a rising sample of 12,414.4 MiB. The unchanged
25 ms test sampler measures 12.156811 GiB and fails its six GiB assertion.
Godot exits zero and all scene checks pass. The whole six-test command exits
101 with five passes and the one budget failure after 905.85 seconds.
The rising native sample contains only 112.3 MiB of free arena space.
Rust reports a 5,846.3 MiB interval peak during the return, compared with
4,299.3 MiB live after release. These are close in time, not synchronized
measurements. Further trimming alone cannot account for the remaining
gap. Live preparation products and native driver allocations must fall.

The script records its nearest frame peak at 12,416.5 MiB during the return
interval, labelled landmark_done. Its monitors report 953.1 MiB of textures,
158.5 MiB of buffers, 18,208 objects, 134 nodes and zero orphan nodes.
Blocking work can peak between frames, so this frame sample does not replace
the external sampler or the budget. After release the ordinary city owns
39 actor nodes, compared with nine at the gate. The speculative inn payload
falls from 283.2 MiB after exit to zero at the gate. The rejected city job
holds no ready payload. The completed LOD cache holds only 0.21 MiB.
All four cached NPC rigs still have consumers at the gate. Actor graph
requested bytes fall from 502.2 MiB after exit to 119.9 MiB there, and actor
clips fall from 430.0 MiB to 293.7 MiB. Source ownership remains 1,281.8 MiB
and gameplay ownership grows to 1,160.1 MiB at the gate. Those are allocation
sites rather than an inventory of individual World maps.

After the return, sampled lavapipe heap falls to 2,122 MiB. The final teardown
sample has no remaining sampled driver heap and only 0.1 MiB of matched
driver mappings, but its arenas still contain about 7,540 MiB of free space.
The corresponding RSS remains about 7,579 MiB just before exit. This shows
that these owners release their allocations and that allocator retention
survives them. It does not establish the driver's individual pool lifetimes
or explain the old branch difference with a single merge.

### Final continuation checks

The release rebuild after the face-detail fix exits zero in 5 minutes
44 seconds. Its extension SHA-256 is
`e757c4188b2e5df1bb04ac920e97171f3859cabcd7d0b22bf3c9f92f0ae95360`.
The current host library passes 541 tests in 0.56 seconds without
SKYRIM_DATA, so installed cases return through their existing skip paths.
The trace library passes 42 tests in 0.28 seconds. Host all-target clippy
passes with default and no-default features, and trace all-target clippy
passes, using warnings denied through the required run wrapper.
Host package formatting and git diff whitespace checks pass.
The installed Whiterun bounded-speculation check passes separately in
87.89 seconds. The full workspace gate remains the lead's responsibility.
The final uninstrumented six-test command passes the installed first-person
check and all four data_source::full_lod_tests checks with this extension.
Its visibility case exits Godot successfully and reports FULL_LOD_RESULT
PASS, including zero retained Tamriel full models after the return. The
unchanged RSS assertion fails at 12.151299 GiB. The command exits 101 after
872.87 seconds, with five passes and one failure. No test is removed,
ignored or weakened. The visibility test's Rust file is unchanged.

The final ordinary capture is `target/full-lod/run-O3dO1w`, with Godot PID
4951. The following values are MiB except counts. Libc live combines arena
and directly mapped malloc allocations, excluding the driver's direct maps.
Requested and usable Rust bytes overlap with that libc total. Godot resource
estimates overlap with driver ownership. These columns must not be summed.

| Phase | RSS | Rust requested | Rust usable | Libc live | Arena free | Textures | Buffers | Objects | Nodes | Orphans |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| after_inn | 4046.1 | 2606.7 | 2804.6 | 3793.3 | 59.5 | 282.0 | 17.7 | 2575 | 98 | 0 |
| after_exit | 8631.0 | 4416.5 | 4650.0 | 7049.2 | 435.3 | 1001.4 | 168.8 | 19258 | 169 | 0 |
| gate_settled | 9987.9 | 3699.8 | 3934.3 | 6412.8 | 1658.7 | 818.1 | 117.2 | 17797 | 103 | 0 |
| after_gate | 9988.0 | 3699.8 | 3934.3 | 6414.3 | 1657.2 | 818.1 | 117.2 | 17797 | 103 | 0 |
| landmark_done | 9991.6 | 3699.8 | 3934.3 | 6642.9 | 1428.6 | 818.1 | 117.2 | 17802 | 104 | 0 |
| return frame peak | 12412.8 | - | - | - | - | 953.1 | 158.5 | 18208 | 134 | 0 |
| after_release | 11121.7 | 4286.7 | 4522.8 | 7297.1 | 2010.0 | 1001.4 | 168.7 | 19264 | 169 | 0 |
| end | 11122.0 | 4286.5 | 4522.6 | 7297.0 | 2010.0 | 1001.4 | 168.7 | 19267 | 169 | 0 |

The independent sampler's 12.151299 GiB peak remains authoritative. The
script's nearest frame peak is lower because blocking work occurs between
frames. Rust's interval peak during the return is 5,846.4 MiB. The final
phase report also exposes Linux RssShmem, at 443.3 MiB after the inn,
1,537.2 MiB after exit, 2,231.5 MiB at the gate and 2,240.7 MiB after release.
This explains why RssAnon and RssFile alone did not cover process RSS.

All actor phase reports now include face detail and retain zero CPU image
bytes. The final gate has four cached rigs and zero without consumers.
The speculative inn is gone and the rejected city leaves no ready payload.
The completed LOD cache holds 223,440 bytes after release. Distant CPU and
GPU ownership is empty after returning to Whiterun. The city remains live,
so its ordinary meshes, textures and 39 actor nodes are still required.

Implementation commit `9349362e855d11148adb1b02a0a4b8ec0843d94e` preserves
the earlier allocator and first-person corrections and adds the mmap
threshold, rig pruning, incremental speculative admission, face-detail
release and expanded attribution. Against the user's supplied 17.9 GiB
baseline, the final observed peak is 12.151 GiB. This comparison combines
the changes and allocator variation. It does not assign each byte to one
fix or prove one merge caused the full five-GiB difference.

The result is partial. The six GiB budget is still exceeded. Remaining live
preparation and driver allocations require further reduction. The native
capture names lavapipe but cannot name its private internal pools without
symbols. Gameplay allocation sites remain grouped rather than split into
individual World maps. Actor CPU image preparation can still duplicate
pending inputs before shared GPU upload, with no measured isolated cost.
The final trim traces take 17.6, 62.1, 148.4 and 92.4 ms at location installs,
so three exceed their declared 33 ms budget. Allocator controls were measured
on this GNU/Linux glibc host, and other libc implementations remain untested.
No additional dependency, per-reference node or saved field is introduced.
