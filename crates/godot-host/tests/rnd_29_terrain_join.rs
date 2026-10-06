// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-29 cell coverage, height seams, restoration and recorded LOD water.

use std::collections::HashMap;
use std::path::PathBuf;

use soe_godot_host::data_source::DataSource;
use soe_godot_host::mesh::{build_surface, MaterialDesc, MeshData, MeshOptions, SurfaceArrays};
use soe_godot_host::streaming::CellCoord;
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel, TerrainLodConfig, TerrainLodStreamer};
use soe_godot_host::terrain_lod_join::join_surface;
use soe_godot_host::units::{CELL_METRES, CELL_UNITS, LAND_SAMPLES, METRES_PER_UNIT};

fn block() -> LodBlockCoord {
    LodBlockCoord {
        level: LodLevel::L4,
        x: -4,
        y: -4,
    }
}

fn flat() -> SurfaceArrays {
    build_surface(
        &MeshData {
            positions: vec![
                [-4.0 * CELL_UNITS, -4.0 * CELL_UNITS, 0.0],
                [0.0, -4.0 * CELL_UNITS, 0.0],
                [0.0, 0.0, 0.0],
                [-4.0 * CELL_UNITS, 0.0, 0.0],
            ],
            normals: None,
            tangents: None,
            uvs: Some(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]),
            colors: Some(vec![[0.4, 0.5, 0.6, 1.0]; 4]),
            indices: vec![0, 1, 2, 0, 2, 3],
            bone_indices: None,
            bone_weights: None,
            material: MaterialDesc::default(),
        },
        MeshOptions::default(),
    )
    .unwrap()
}

fn area(surface: &SurfaceArrays) -> f64 {
    surface
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| {
            let a = surface.positions[t[0] as usize];
            let b = surface.positions[t[1] as usize];
            let c = surface.positions[t[2] as usize];
            ((f64::from(b[0]) - f64::from(a[0])) * (f64::from(c[2]) - f64::from(a[2]))
                - (f64::from(b[2]) - f64::from(a[2])) * (f64::from(c[0]) - f64::from(a[0])))
            .abs()
                * 0.5
        })
        .sum()
}

#[test]
fn rnd_29_loaded_cells_remove_only_their_coverage_and_unload_restores_it() {
    let source = flat();
    let cell = CellCoord::new(-3, -2);
    let near = HashMap::from([(cell, vec![10.0; LAND_SAMPLES.pow(2)])]);
    let joined = join_surface(&source, block(), &near).unwrap();
    for t in joined.indices.as_chunks::<3>().0 {
        let p: [f32; 3] = std::array::from_fn(|axis| {
            t.iter()
                .map(|i| joined.positions[*i as usize][axis])
                .sum::<f32>()
                / 3.0
        });
        let coord = CellCoord::new(
            (p[0] / CELL_METRES).floor() as i32,
            (-p[2] / CELL_METRES).floor() as i32,
        );
        assert_ne!(coord, cell, "LOD triangle covers loaded LAND");
    }
    assert!(
        (area(&joined) - 15.0 * f64::from(CELL_METRES).powi(2)).abs() < 0.1,
        "unloaded cells have no coverage gap"
    );
    let restored = join_surface(&source, block(), &HashMap::new()).unwrap();
    assert!((area(&restored) - 16.0 * f64::from(CELL_METRES).powi(2)).abs() < 0.1);
    assert_eq!(joined.material, source.material);
    assert_eq!(joined.uvs.len(), joined.positions.len());
    assert!(joined
        .colors
        .iter()
        .all(|color| *color == [0.4, 0.5, 0.6, 1.0]));
}

#[test]
fn rnd_29_exposed_edges_match_every_land_sample_on_all_four_sides() {
    let cell = CellCoord::new(-3, -2);
    let heights: Vec<_> = (0..LAND_SAMPLES.pow(2))
        .map(|i| (i % 33) as f32 + 2.0 * (i / 33) as f32 + 10.0)
        .collect();
    let near = HashMap::from([(cell, heights.clone())]);
    let joined = join_surface(&flat(), block(), &near).unwrap();
    for sample in 0..33 {
        for (col, row) in [(sample, 0), (sample, 32), (0, sample), (32, sample)] {
            let x = (cell.x as f32 + col as f32 / 32.0) * CELL_METRES;
            let z = -(cell.y as f32 + row as f32 / 32.0) * CELL_METRES;
            let matching: Vec<_> = joined
                .positions
                .iter()
                .filter(|p| (p[0] - x).abs() < 0.001 && (p[2] - z).abs() < 0.001)
                .collect();
            assert!(
                !matching.is_empty(),
                "missing edge vertex col={col} row={row}"
            );
            assert!(
                matching
                    .iter()
                    .all(|p| (p[1] - heights[row * 33 + col]).abs() < 0.001),
                "LOD edge height differs from LAND"
            );
        }
    }
}

#[test]
fn rnd_29_water_coverage_keeps_recorded_height_instead_of_land_edge_height() {
    let mut water = flat();
    water.material.water = Some(soe_godot_host::mesh::WaterMaterial::default());
    let near = HashMap::from([(CellCoord::new(-3, -2), vec![100.0; 33 * 33])]);
    let joined = join_surface(&water, block(), &near).unwrap();
    assert!(joined.positions.iter().all(|p| p[1] == 0.0));
    assert!((area(&joined) - 15.0 * f64::from(CELL_METRES).powi(2)).abs() < 0.1);
}

#[test]
fn rnd_29_handoff_streamer_keeps_blocks_covering_pending_and_retained_cells() {
    let mut streamer = TerrainLodStreamer::new(TerrainLodConfig {
        full_res_radius: 2,
        lod_radius: 8,
        ..Default::default()
    })
    .unwrap()
    .with_cell_handoff();
    let focus = CellCoord::new(1, 1);
    streamer.settle(focus);
    assert!(streamer.is_resident(LodBlockCoord::containing(focus, LodLevel::L4)));
    streamer.settle(CellCoord::new(2, 1));
    assert!(streamer.is_resident(LodBlockCoord::containing(focus, LodLevel::L4)));
}

#[test]
fn rnd_29_malformed_join_inputs_return_errors() {
    let mut source = flat();
    source.indices.push(0);
    assert!(join_surface(&source, block(), &HashMap::new()).is_err());
    assert!(join_surface(
        &flat(),
        block(),
        &HashMap::from([(CellCoord::new(0, 0), vec![0.0])])
    )
    .is_err());
    assert!(join_surface(
        &flat(),
        LodBlockCoord {
            x: i32::MAX,
            ..block()
        },
        &HashMap::new()
    )
    .is_err());
}

#[test]
fn rnd_29_installed_lod_water_matches_cell_heights_and_no_lod_flags() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping");
        return;
    };
    let source = DataSource::open(&PathBuf::from(data)).unwrap();
    let (world, _) = source.resolve_world("Tamriel").unwrap();
    let order = source.shared_order();
    let mut planes = 0;
    let mut heights = std::collections::HashSet::new();
    for (x, y) in [(0, -4), (0, 0), (-4, -4), (-4, 0), (-48, 8)] {
        let block = LodBlockCoord {
            level: LodLevel::L4,
            x,
            y,
        };
        for surface in source.lod_water_surfaces(world, block).unwrap() {
            let cx = (surface.arrays.bounds.min.x / CELL_METRES).round() as i32;
            let cy = (-surface.arrays.bounds.max.z / CELL_METRES).round() as i32;
            let id = source.cell_index().exterior_cell(world, cx, cy).unwrap();
            let owner = order.winner_ref(id).unwrap();
            let view = soe_esm::records::cell::Cell::from_record(
                order.record(owner),
                order.file(owner.file).plugin.header.is_localized(),
            )
            .unwrap();
            assert_eq!(view.flags & soe_esm::records::cell::flags::NO_LOD_WATER, 0);
            let height = view
                .water_height
                .filter(|h| h.is_finite() && h.abs() < 1.0e30)
                .unwrap_or(-14000.0);
            for p in &surface.arrays.positions {
                assert!(
                    (p[1] - height * METRES_PER_UNIT).abs() < 0.001,
                    "LOD water height differs from CELL"
                );
            }
            heights.insert(height.to_bits());
            planes += 1;
        }
    }
    assert!(
        planes >= 16,
        "insufficient installed water survey: {planes}"
    );
    println!("RND-29 water: {planes} planes, {} heights", heights.len());
}

fn water_fixture(world_flags: u8) -> (tempfile::TempDir, DataSource) {
    use soe_esm::{tag, Entry, FileHeader, Group, GroupKind, Plugin, Record};
    let directory =
        tempfile::tempdir_in(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"))
            .unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let mut top = Group::new(GroupKind::Top(tag::WRLD));
    for (id, parent) in [(0x100u32, false), (0x101, true)] {
        let mut record = Record::new(tag::WRLD, id)
            .zsub(tag::EDID, if parent { "ChildWater" } else { "ParentWater" })
            .sub(b"DATA", [world_flags]);
        if parent {
            record = record
                .sub(b"WNAM", 0x100u32.to_le_bytes())
                .sub(b"PNAM", 8u16.to_le_bytes());
        } else {
            record = record.sub(
                b"DNAM",
                [0.0f32.to_le_bytes(), 500.0f32.to_le_bytes()].concat(),
            );
        }
        top.entries.push(Entry::Record(plugin.push_record(record)));
        if !parent {
            continue;
        }
        let mut children = Group::new(GroupKind::WorldChildren(id));
        let mut block = Group::new(GroupKind::ExteriorBlock { x: 0, y: 0 });
        let mut sub = Group::new(GroupKind::ExteriorSubBlock { x: 0, y: 0 });
        for (x, flags, height) in [
            (0i32, 2u16, 120.0f32),
            (1, 2, f32::MAX),
            (2, 10, 300.0),
            (3, 0, 0.0),
        ] {
            let record = Record::new(tag::CELL, 0x200 + x as u32)
                .zsub(tag::EDID, &format!("Water{x}"))
                .sub(b"DATA", flags.to_le_bytes())
                .sub(b"XCLC", [x.to_le_bytes(), 0i32.to_le_bytes()].concat())
                .sub(b"XCLW", height.to_le_bytes());
            sub.entries.push(Entry::Record(plugin.push_record(record)));
        }
        block.entries.push(Entry::Group(sub));
        children.entries.push(Entry::Group(block));
        top.entries.push(Entry::Group(children));
    }
    plugin.groups.push(top);
    std::fs::write(directory.path().join("Skyrim.esm"), plugin.to_bytes()).unwrap();
    for name in [
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            directory.path().join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
    let source = DataSource::open(directory.path()).unwrap();
    (directory, source)
}

#[test]
fn rnd_29_lod_water_honors_cell_world_flags_and_inherited_default_heights() {
    for world_flags in [0, soe_esm::records::wrld::flags::NO_LOD_WATER] {
        let (_directory, source) = water_fixture(world_flags);
        let world = source.resolve_world("ChildWater").unwrap().0;
        let surfaces = source
            .lod_water_surfaces(
                world,
                LodBlockCoord {
                    level: LodLevel::L4,
                    x: 0,
                    y: 0,
                },
            )
            .unwrap();
        if world_flags != 0 {
            assert!(surfaces.is_empty());
            continue;
        }
        assert_eq!(
            surfaces.len(),
            2,
            "dry and No LOD Water cells must not draw planes"
        );
        for (surface, height) in surfaces.iter().zip([120.0, 500.0]) {
            assert!(surface
                .arrays
                .positions
                .iter()
                .all(|p| (p[1] - height * METRES_PER_UNIT).abs() < 0.0001));
        }
    }
}
