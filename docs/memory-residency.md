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
