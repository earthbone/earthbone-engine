// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::path::PathBuf;

use soe_esm::GlobalFormId;
use soe_godot_host::data_source::DataSource;
use soe_godot_host::streaming::CellCoord;
use soe_godot_host::terrain::CellHeights;
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel};
use soe_godot_host::terrain_lod_generate::generate_terrain_lod_btr;
use soe_godot_host::units::{GodotPos, CELL_UNITS};

fn block(level: LodLevel, x: i32, y: i32) -> LodBlockCoord {
    LodBlockCoord { level, x, y }
}

fn continuous_cells(chosen: LodBlockCoord) -> HashMap<CellCoord, CellHeights> {
    let mut cells = HashMap::new();
    for cy in 0..chosen.level.cells() {
        for cx in 0..chosen.level.cells() {
            let cell = CellCoord::new(chosen.x + cx, chosen.y + cy);
            let mut units = Vec::with_capacity(33 * 33);
            for row in 0..33 {
                for col in 0..33 {
                    units.push(
                        ((cell.x * 32 + col as i32) + (cell.y * 32 + row as i32)) as f32,
                    );
                }
            }
            cells.insert(cell, CellHeights::new(cell, units).expect("valid LAND grid"));
        }
    }
    cells
}

#[test]
fn edt_12_every_lod_sample_uses_the_matching_authored_land_boundary() {
    for level in [LodLevel::L4, LodLevel::L8, LodLevel::L16, LodLevel::L32] {
        let chosen = block(level, -level.cells(), level.cells());
        let bytes = generate_terrain_lod_btr(&continuous_cells(chosen), chosen)
            .expect("generate BTR")
            .expect("block contains LAND");
        let mesh = soe_nif::parse(&bytes)
            .expect("parse generated BTR")
            .meshes()
            .expect("read BTR shape")
            .remove(0);
        assert_eq!(mesh.positions.len(), 33 * 33);
        for row in 0..33 {
            for col in 0..33 {
                let source_x = chosen.x * 32 + col as i32 * level.cells();
                let source_y = chosen.y * 32 + row as i32 * level.cells();
                assert_eq!(
                    mesh.positions[row * 33 + col][2],
                    (source_x + source_y) as f32,
                    "{level:?}, sample ({col},{row})"
                );
            }
        }
    }
}

#[test]
fn edt_12_missing_interior_land_emits_no_triangles_for_that_cell() {
    let cell = CellCoord::new(-2, 1);
    let cells = HashMap::from([(cell, CellHeights::flat(cell, 17.0))]);
    let chosen = block(LodLevel::L4, -4, 0);
    let bytes = generate_terrain_lod_btr(&cells, chosen)
        .expect("generate partial BTR")
        .expect("one cell has LAND");
    let mesh = soe_nif::parse(&bytes)
        .expect("parse partial BTR")
        .meshes()
        .expect("read partial BTR shape")
        .remove(0);
    assert_eq!(mesh.triangle_count(), 8 * 8 * 2);
    for triangle in mesh.indices.chunks_exact(3) {
        let columns: Vec<usize> = triangle.iter().map(|index| *index as usize % 33).collect();
        let rows: Vec<usize> = triangle.iter().map(|index| *index as usize / 33).collect();
        assert!(columns.iter().all(|column| (16..=24).contains(column)));
        assert!(rows.iter().all(|row| (8..=16).contains(row)));
    }
}

#[test]
fn edt_12_generated_btr_loads_at_its_negative_worldspace_origin() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        return;
    };
    let source = DataSource::open(&data).expect("open Skyrim data");
    let world = GlobalFormId {
        file: 0,
        object: 0x3c,
    };
    let vanilla_block = block(LodLevel::L4, 0, 0);
    let vanilla_bytes = source
        .generate_terrain_lod_block(world, vanilla_block)
        .expect("generate from winning vanilla LAND records")
        .expect("vanilla block contains LAND");
    let vanilla_loaded = source
        .load_lod_block_bytes(world, vanilla_block, &vanilla_bytes)
        .expect("load record-generated BTR through consumer");
    assert!(!vanilla_loaded.surfaces.is_empty());
    for surface in &vanilla_loaded.surfaces {
        for corner in [surface.arrays.bounds.min, surface.arrays.bounds.max] {
            let skyrim = GodotPos::new(corner.x, corner.y, corner.z).to_skyrim();
            assert!((0.0..=4.0 * CELL_UNITS).contains(&skyrim.x));
            assert!((0.0..=4.0 * CELL_UNITS).contains(&skyrim.y));
        }
    }

    let chosen = block(LodLevel::L4, -4, -4);
    let bytes = generate_terrain_lod_btr(&continuous_cells(chosen), chosen)
        .expect("generate authored LAND fixture")
        .expect("fixture contains LAND");
    let loaded = source
        .load_lod_block_bytes(world, chosen, &bytes)
        .expect("load through BTR consumer");
    assert!(!loaded.surfaces.is_empty());
    assert!(loaded.approx_bytes > 0);
    for surface in &loaded.surfaces {
        for corner in [surface.arrays.bounds.min, surface.arrays.bounds.max] {
            let skyrim = GodotPos::new(corner.x, corner.y, corner.z).to_skyrim();
            assert!(((-4.0 * CELL_UNITS)..=0.0).contains(&skyrim.x));
            assert!(((-4.0 * CELL_UNITS)..=0.0).contains(&skyrim.y));
        }
    }
}

#[test]
fn edt_12_block_beyond_cell_coordinate_limit_is_rejected() {
    let cell = CellCoord::new(i32::MAX, 0);
    let cells = HashMap::from([(cell, CellHeights::flat(cell, 0.0))]);
    let error = generate_terrain_lod_btr(&cells, block(LodLevel::L4, i32::MAX, 0))
        .expect_err("overflowing LOD block is invalid");
    assert!(error.to_string().contains("cell coordinate range"));
}
