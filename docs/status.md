# Status, per subsystem

What the engine does today. `CLAUDE.md` carries the short version and the
always-relevant working rules, this file carries the detail. Update both when
a batch of branches merges.

Reference-correction checkpoint, 2026-09-19, based on master `75cfb55`.
The full gate reports 835 test passes. Earlier measurements below retain their
original scope. See [the reference audit](reference-audit.md) for source
contracts, regression tests and work still pending.

- **Records.** `esm` reads the load order with winning-override semantics, a
  `CellIndex` maps cells and the worldspace grid to their placed objects
  (build 0.15 s over 872k placed objects, lookup 9 ns), and `dirty.rs`
  reports identical-to-master overrides, deleted references and deleted
  navmeshes per plugin. Dirty plugins need no cleaning: a deleted reference
  resolves and starts disabled, a deleted navmesh drops with edge repair.
- **Scripts.** 345 natives with state plus 191 stubs, the VM walks base
  classes on dispatch, and the first real `start` on the installed game runs
  423 quests through 1,388 native calls with **zero** unresolved calls, which
  the gated test now asserts. The five masters declare 686 natives in all and
  the gated report lists what the registry still lacks, with call-site counts.
  No script named for a DLC declares a native of its own: every DLC native is
  an addition to a base class, so DLC coverage is measured by call sites, not
  by script name. `SOEV` version 2 (also reading version 1), `SOEW` version 4 and `SOEG`
  save and load. Waits use distinct clock domains. Dispel calls are stubs
  that preserve known and equipped spells until active effects exist.
- **Actors.** `NPC_` base stats, `PACK` packages with sandbox, travel and
  sleep on the navmesh graph (4,635 of 5,961 vanilla packages resolve), and a
  havok behavior interpreter that walks the vanilla `mt_behavior.hkx` with
  poses within 3.8e-6 of the clip sampler.
- **Rendering.** The Godot host batches surfaces per material as
  `RenderingServer` instances, uploads block-compressed DDS untouched from a
  session cache, and streams interior and exterior cells with terrain. The
  Whiterun 3 by 3 block draws at p99 0.519 ms, a cell streams in 4 ms. Godot
  stays, unpatched, tested on 4.4.1 and 4.7.2 with the api-4-4 floor.
  Characters skin correctly as of the bind pose fix: `nif.xml` declares
  `Matrix33` column major, which is right for a scene graph node and wrong for
  a `NiSkinData` bind, so the bind rotation needed transposing. Captures in
  `docs/images/skinned-idle.png` and `skinned-walk.png`. Skin partitions also
  address their own short bone list rather than the skin instance's, which the
  Special Edition path had been copying raw: 564 of 846 partitions across 400
  actor meshes reorder, so it was wrong more often than right.
- **Physics and movement.** Cell collision runs in two worlds, the headless
  one and Godot's bundled Jolt, which is the one we use rather than linking
  our own. A capsule character walks at the game's own speeds, with step
  height, crouch and jump. Activating a door moves the player between cells
  through its teleport data.
- **Determinism.** A per-tick state hash over world, VM and clock with a part
  per subsystem, `SOE_HASH_DUMP`, and gated 600-tick runs that hash equal
  twice. `LoadOrder::winners` walks a sorted list so nothing depends on hash
  map order.
