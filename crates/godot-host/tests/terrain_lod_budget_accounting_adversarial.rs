// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial tests for RND-03's terrain LOD residency budget: the claim
//! in `crates/godot-host/src/data_source/lod.rs` that `load_lod_block`
//! "reports back the bytes its meshes actually built to, so the budget
//! reflects what is loaded rather than an estimate made before the read",
//! and the RND-03 done check itself, "Terrain LOD replaces distant full
//! resolution terrain within a recorded residency budget".
//!
//! These run with no game data: `Mesh::approx_bytes` and `build_surface`
//! are exercised directly on synthetic shapes, and `TerrainLodStreamer` is
//! pure logic over `CellCoord` values.
//!
//! ```text
//! cargo test -p soe-godot-host --test terrain_lod_budget_accounting_adversarial
//! ```

use soe_godot_host::mesh::{build_surface, MeshData, MeshOptions};
use soe_godot_host::streaming::CellCoord;
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel, TerrainLodConfig, TerrainLodStreamer};
use soe_nif::{Bound, Material, Mesh as NifMesh, Transform};

fn triangle_positions() -> Vec<[f32; 3]> {
    vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
}

/// A minimal decoded NIF shape, the same shape `DataSource::load_lod_block`
/// gets back from `NifFile::meshes()` for one BTR shape, with the given
/// normals and vertex colors and nothing else attached.
fn shape_with(normals: Vec<[f32; 3]>, colors: Vec<[f32; 4]>) -> NifMesh {
    let positions = triangle_positions();
    let indices = vec![0u32, 1, 2];
    NifMesh {
        name: "lod_shape".to_string(),
        block: 0,
        transform: Transform::default(),
        bound: Bound::default(),
        positions,
        normals,
        tangents: Vec::new(),
        uvs: Vec::new(),
        colors,
        indices,
        bone_weights: Vec::new(),
        bone_indices: Vec::new(),
        skin: None,
        material: Material::default(),
    }
}

/// Bug: a BTR shape with no NIF normals, the ordinary case for distant
/// terrain LOD saving space, gets normals generated during `build_surface`
/// (`MeshOptions::default().generate_missing_normals` is on, see
/// `crates/godot-host/src/mesh.rs`). `load_lod_block` charges the block's
/// residency cost with `shape.approx_bytes()` computed on the decoded NIF
/// `Mesh` before that generation happens, so it counts zero bytes for a
/// normals buffer the build step is about to allocate and hand to Godot.
/// The residency budget then reads low against a block that is actually
/// larger once resident, exactly the "budget only met on a warm cache"
/// failure mode: the undercount is permanent here, not just during a
/// cold-cache window.
#[test]
fn approx_bytes_undercounts_a_shape_whose_normals_get_generated() {
    let shape = shape_with(Vec::new(), Vec::new());
    let data = MeshData {
        positions: shape.positions.clone(),
        indices: shape.indices.clone(),
        ..Default::default()
    };
    let built = build_surface(&data, MeshOptions::default())
        .unwrap_or_else(|e| panic!("build_surface: {e}"));

    assert!(
        !built.normals.is_empty(),
        "MeshOptions::default() turns generate_missing_normals on, so a \
         shape with no source normals must still build a normals array; \
         if this assertion fails the other bug below is moot because the \
         pipeline stopped generating normals"
    );

    let built_bytes = built.positions.len() as u64 * 12
        + built.normals.len() as u64 * 12
        + built.indices.len() as u64 * 4;
    let claimed_bytes = shape.approx_bytes();

    assert!(
        claimed_bytes >= built_bytes,
        "Mesh::approx_bytes ({claimed_bytes} bytes) must not be smaller \
         than what build_surface actually built ({built_bytes} bytes, \
         including {} generated normals at 12 bytes each); \
         data_source/lod.rs promises the recorded figure is 'what is \
         loaded', not an estimate made before generate_missing_normals ran",
        built.normals.len(),
    );
}

/// Bug in the other direction: `Mesh::approx_bytes` (crates/nif/src/mesh.rs)
/// adds 16 bytes a vertex whenever the decoded shape's `colors` field is
/// non-empty. But `crate::mesh::MeshSource` (crates/godot-host/src/mesh.rs)
/// has no `colors()` method, and neither `MeshData` nor the built
/// `SurfaceArrays` carries a color field at all, so a shape's vertex
/// colors never reach `build_surface` and are never resident in the built
/// surface. `load_lod_block` therefore charges a residency cost for a
/// buffer that is never allocated whenever a BTR shape happens to carry
/// vertex colors, the same "not what was actually built" defect as the
/// missing-normals case, now over-counting instead of under-counting.
#[test]
fn approx_bytes_charges_for_vertex_colors_build_surface_never_builds() {
    let positions = triangle_positions();
    let without_colors = shape_with(Vec::new(), Vec::new());
    let with_colors = shape_with(Vec::new(), vec![[1.0, 1.0, 1.0, 1.0]; positions.len()]);

    let data = MeshData {
        positions: positions.clone(),
        indices: vec![0, 1, 2],
        ..Default::default()
    };
    let built = build_surface(&data, MeshOptions::default())
        .unwrap_or_else(|e| panic!("build_surface: {e}"));
    assert_eq!(
        built.positions.len(),
        positions.len(),
        "sanity: the shape must actually build"
    );

    assert_eq!(
        with_colors.approx_bytes(),
        without_colors.approx_bytes(),
        "a shape's vertex colors are dropped before build_surface (no \
         color field exists past MeshData), so the built surface is \
         identical with or without them. approx_bytes charged an extra \
         {} bytes for colors that are never resident, which contradicts \
         data_source/lod.rs's claim that the recorded figure is 'what is \
         loaded' rather than an estimate",
        with_colors.approx_bytes() - without_colors.approx_bytes(),
    );
}

/// Chebyshev block assignment must hold west and south of the worldspace
/// origin, not only in the positive quadrant real test cells happen to
/// sit in. `LodBlockCoord::containing` uses `div_euclid`, the floor
/// division that keeps a block's cell span contiguous across zero. A
/// naive `/` would instead round toward zero and put cell -1 in the same
/// block as cell 0 at every level, splitting a block's western or
/// southern edge onto the wrong side of the origin.
#[test]
fn lod_block_coord_containing_is_contiguous_across_the_origin() {
    let span = LodLevel::L4.cells();
    for x in -(span * 3)..(span * 3) {
        let cell = CellCoord::new(x, 0);
        let block = LodBlockCoord::containing(cell, LodLevel::L4);
        let origin = block.origin_cell();
        assert!(
            cell.x >= origin.x && cell.x < origin.x + span,
            "cell x={} must fall inside its own block's span [{}, {}), \
             block x index {}",
            cell.x,
            origin.x,
            origin.x + span,
            block.x,
        );
    }
    // The classic off-by-one: cell -1 and cell 0 must land in different
    // blocks at every level, since block 0 starts exactly at cell 0.
    for level in [LodLevel::L4, LodLevel::L8, LodLevel::L16, LodLevel::L32] {
        let west_of_origin = LodBlockCoord::containing(CellCoord::new(-1, -1), level);
        let at_origin = LodBlockCoord::containing(CellCoord::new(0, 0), level);
        assert_ne!(
            west_of_origin, at_origin,
            "cell (-1, -1) and cell (0, 0) must not share a block at level {level:?}"
        );
    }
}

/// `TerrainLodConfig::validate` promises `lod_radius` "must exceed
/// full_res_radius", not merely reach it. A caller that configures the
/// two radii equal would otherwise get a streamer whose LOD ring is empty
/// by construction: no cell distance is simultaneously `> full_res_radius`
/// and `<= lod_radius` when the two are the same number, so distant
/// terrain would silently never replace full resolution terrain at all.
#[test]
fn lod_radius_equal_to_full_res_radius_is_rejected() {
    let config = TerrainLodConfig {
        level: LodLevel::L4,
        full_res_radius: 4,
        lod_radius: 4,
        max_loads_per_tick: 1,
        max_unloads_per_tick: 1,
        budget_bytes: 0,
    };
    assert!(
        config.validate().is_err(),
        "lod_radius == full_res_radius must be rejected: a streamer built \
         from it would never have anything to load"
    );
}

/// The done check's "within a recorded residency budget" is a postcondition
/// of `evict_to_budget`, not a hope: once it returns, resident usage must
/// actually fit inside the configured cap, or a caller that trusts the
/// method to enforce the budget is misled.
#[test]
fn evict_to_budget_brings_usage_to_or_under_the_cap() {
    let config = TerrainLodConfig {
        level: LodLevel::L4,
        full_res_radius: 0,
        lod_radius: 12,
        max_loads_per_tick: 0,
        max_unloads_per_tick: 0,
        budget_bytes: 100,
    };
    let mut streamer =
        TerrainLodStreamer::new(config).unwrap_or_else(|e| panic!("TerrainLodStreamer::new: {e}"));
    let focus = CellCoord::new(0, 0);
    let plan = streamer.settle(focus);
    assert!(
        !plan.load.is_empty(),
        "a 12 cell LOD radius around the origin must want at least one block"
    );
    for block in &plan.load {
        streamer.record_bytes(*block, 40);
    }
    assert!(
        streamer.usage_bytes() > config.budget_bytes,
        "test setup must actually exceed the cap to exercise eviction, \
         got {} bytes against a {} cap",
        streamer.usage_bytes(),
        config.budget_bytes,
    );

    streamer.evict_to_budget(focus);

    assert!(
        streamer.usage_bytes() <= config.budget_bytes,
        "evict_to_budget must leave usage_bytes ({}) at or under the \
         configured budget_bytes ({})",
        streamer.usage_bytes(),
        config.budget_bytes,
    );
}

/// A block `evict_to_budget` releases must actually stop being resident:
/// a caller iterating `plan.unload` or `evicted` to release engine-side
/// resources, then checking `is_resident` for anything it kept, must never
/// see a released block still reported resident. This is the state the
/// done check's "recorded residency budget" is tracked in, so a mismatch
/// here means the recorded budget can lie about what is actually kept.
#[test]
fn evicted_blocks_are_no_longer_resident() {
    let config = TerrainLodConfig {
        level: LodLevel::L4,
        full_res_radius: 0,
        lod_radius: 12,
        max_loads_per_tick: 0,
        max_unloads_per_tick: 0,
        budget_bytes: 1,
    };
    let mut streamer =
        TerrainLodStreamer::new(config).unwrap_or_else(|e| panic!("TerrainLodStreamer::new: {e}"));
    let focus = CellCoord::new(0, 0);
    let plan = streamer.settle(focus);
    for block in &plan.load {
        streamer.record_bytes(*block, 10);
    }

    let evicted = streamer.evict_to_budget(focus);
    assert!(!evicted.is_empty(), "a 1 byte budget must evict something");
    for block in &evicted {
        assert!(
            !streamer.is_resident(*block),
            "block {block:?} was returned by evict_to_budget but \
             is_resident still reports it resident"
        );
    }
}
