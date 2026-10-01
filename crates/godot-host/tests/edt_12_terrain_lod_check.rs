// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! EDT-12 terrain LOD geometry and BTR consumer checks. tes4skyrim's
//! terrain_lod.py and terrain_nif.py supply the tile sampling and mask rules.

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

#[test]
fn edt_12_generated_btr_matches_selected_source_cell_boundaries() {
    for level in [LodLevel::L4, LodLevel::L8, LodLevel::L16, LodLevel::L32] {
        let chosen = block(level, -4, 8);
        let cell = CellCoord::new(-4, 8);
        let mut units = vec![0.0; 33 * 33];
        for row in 0..33 {
            for col in 0..33 {
                units[row * 33 + col] = col as f32 * 10.0 + row as f32 * 20.0;
            }
        }
        let cells = HashMap::from([(cell, CellHeights::new(cell, units).unwrap())]);
        let bytes = generate_terrain_lod_btr(&cells, chosen).unwrap().unwrap();
        let meshes = soe_nif::parse(&bytes).unwrap().meshes().unwrap();
        assert_eq!(meshes.len(), 1, "{level:?}");
        let mesh = &meshes[0];
        assert_eq!(
            mesh.material.kind,
            soe_nif::mesh::MaterialKind::Lighting(soe_nif::blocks::ShaderType::LodLandscapeNoise),
            "{level:?} uses Skyrim's terrain LOD shader"
        );
        assert_eq!(mesh.positions.len(), 33 * 33, "{level:?}");
        assert_eq!(
            mesh.indices.len() / 3,
            (32 / level.cells() as usize).pow(2) * 2,
            "{level:?}"
        );
        assert_eq!(mesh.positions[0], [0.0, 0.0, 0.0], "{level:?}");
        let edge = (32 / level.cells() as usize) * 33 + 32 / level.cells() as usize;
        assert_eq!(
            mesh.positions[edge],
            [CELL_UNITS, CELL_UNITS, 960.0],
            "{level:?} must reach the selected cell's north-east LAND sample"
        );
    }
}

#[test]
fn edt_12_missing_land_has_no_invented_ground() {
    let chosen = block(LodLevel::L4, 0, 0);
    assert!(generate_terrain_lod_btr(&HashMap::new(), chosen)
        .unwrap()
        .is_none());
    let cell = CellCoord::new(0, 0);
    let cells = HashMap::from([(cell, CellHeights::flat(cell, 42.0))]);
    let bytes = generate_terrain_lod_btr(&cells, chosen).unwrap().unwrap();
    let mesh = soe_nif::parse(&bytes).unwrap().meshes().unwrap().remove(0);
    assert_eq!(mesh.triangle_count(), 8 * 8 * 2);
    assert!(mesh.indices.iter().all(|&index| {
        let row = index as usize / 33;
        let col = index as usize % 33;
        row <= 8 && col <= 8
    }));
}

#[test]
fn edt_12_neighbor_blocks_share_the_authored_edge() {
    let west = CellCoord::new(3, 0);
    let east = CellCoord::new(4, 0);
    let cells = HashMap::from([
        (west, CellHeights::flat(west, 10.0)),
        (east, CellHeights::flat(east, 50.0)),
    ]);
    let left = generate_terrain_lod_btr(&cells, block(LodLevel::L4, 0, 0))
        .unwrap()
        .unwrap();
    let right = generate_terrain_lod_btr(&cells, block(LodLevel::L4, 4, 0))
        .unwrap()
        .unwrap();
    let left_mesh = soe_nif::parse(&left).unwrap().meshes().unwrap().remove(0);
    let right_mesh = soe_nif::parse(&right).unwrap().meshes().unwrap().remove(0);
    assert_eq!(left_mesh.positions[32][2], 50.0);
    assert_eq!(left_mesh.positions[32][2], right_mesh.positions[0][2]);
}

#[test]
fn edt_12_invalid_land_grid_is_rejected() {
    let cell = CellCoord::new(0, 0);
    let cells = HashMap::from([(
        cell,
        CellHeights {
            cell,
            units: vec![f32::NAN; 33 * 33],
        },
    )]);
    let error = generate_terrain_lod_btr(&cells, block(LodLevel::L4, 0, 0)).unwrap_err();
    assert!(error.to_string().contains("1089 finite samples"));
}

#[test]
fn edt_12_vanilla_land_generates_btr_loaded_by_consumer() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        return;
    };
    let source = DataSource::open(&data).unwrap();
    let world = GlobalFormId {
        file: 0,
        object: 0x3c,
    };
    let chosen = block(LodLevel::L4, 0, 0);
    let bytes = source
        .generate_terrain_lod_block(world, chosen)
        .unwrap()
        .unwrap();
    let loaded = source.load_lod_block_bytes(world, chosen, &bytes).unwrap();
    assert!(!loaded.surfaces.is_empty());
    assert!(loaded.approx_bytes > 0);
    let mut xs = Vec::new();
    for surface in &loaded.surfaces {
        for corner in [surface.arrays.bounds.min, surface.arrays.bounds.max] {
            xs.push(GodotPos::new(corner.x, corner.y, corner.z).to_skyrim().x / CELL_UNITS);
        }
    }
    assert!(xs.iter().all(|&x| (0.0..=4.0).contains(&x)), "{xs:?}");
}
