// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-28 installed block placement and multi-level distant scenery.

use soe_godot_host::{
    data_source::{vfs_from_loadout, DataSource},
    terrain_lod::{LodBlockCoord, LodLevel},
    units::GodotPos,
};

#[test]
fn rnd_28_installed_object_blocks_have_correct_placement_at_every_level() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let loadout = soe_profile::LoadoutInput {
        game_dir: std::path::Path::new(&data).parent().unwrap().to_path_buf(),
        data_roots: vec![data.clone().into()],
        plugins_txt: None,
        loadorder_txt: None,
        ini_dir: None,
        ccc: None,
    }
    .resolve()
    .unwrap();
    let vfs = vfs_from_loadout(&loadout).unwrap();
    for folder in [
        "meshes/terrain/tamriel/objects",
        "meshes/terrain/tamriel/trees",
    ] {
        let paths = vfs.list(folder);
        let mut counts = std::collections::BTreeMap::new();
        for path in &paths {
            *counts
                .entry(path.split('.').nth(1).unwrap_or("other"))
                .or_insert(0usize) += 1;
        }
        println!("{folder}: {counts:?}");
    }
    let authored_lod = vfs.list("meshes/lod");
    let mountain_path = authored_lod
        .iter()
        .find(|path| {
            path.contains("mountain")
                && std::path::Path::new(path)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("nif"))
        })
        .unwrap();
    let mountain_nif = soe_nif::parse(&vfs.read(mountain_path).unwrap()).unwrap();
    assert!(!mountain_nif.meshes().unwrap().is_empty());
    let source = DataSource::open(std::path::Path::new(&data)).unwrap();
    let (world, _) = source.resolve_world("Tamriel").unwrap();
    let object_blocks =
        source.scenery_lod_blocks("Tamriel", soe_godot_host::terrain_lod::LodKind::Object);
    let levels: std::collections::BTreeSet<_> =
        object_blocks.iter().map(|b| b.level.cells()).collect();
    assert_eq!(levels, std::collections::BTreeSet::from([4, 8, 16]));
    let tree_blocks =
        source.scenery_lod_blocks("Tamriel", soe_godot_host::terrain_lod::LodKind::Tree);
    assert!(tree_blocks.len() >= 300);
    let types = soe_godot_host::tree_lod::read_types(
        &vfs.read("meshes/terrain/tamriel/trees/tamriel.lst")
            .unwrap(),
    )
    .unwrap();
    let mut trees = 0;
    for block in &tree_blocks {
        let refs = soe_godot_host::tree_lod::read_references(
            &vfs.read(&soe_godot_host::terrain_lod::btt_path("Tamriel", *block))
                .unwrap(),
        )
        .unwrap();
        for reference in &refs {
            assert!(types.contains_key(&reference.tree_type));
        }
        trees += refs.len();
    }
    assert!(trees >= 10000);
    for block in tree_blocks.iter().take(3) {
        let loaded = source
            .load_tree_lod_block(world, "Tamriel", *block)
            .unwrap()
            .unwrap();
        assert!(loaded
            .surfaces
            .iter()
            .all(|surface| surface.reference == world && surface.textures.diffuse.is_some()));
        assert_eq!(loaded.triangles, loaded.authored_references * 4);
    }
    println!(
        "RND-28 tree survey: blocks={} trees={} types={}",
        tree_blocks.len(),
        trees,
        types.len()
    );
    let other_blocks = source.scenery_lod_blocks(
        "DLC2SolstheimWorld",
        soe_godot_host::terrain_lod::LodKind::Tree,
    );
    assert!(!other_blocks.is_empty());
    let other_types = soe_godot_host::tree_lod::read_types(
        &vfs.read("meshes/terrain/dlc2solstheimworld/trees/dlc2solstheimworld.lst")
            .unwrap(),
    )
    .unwrap();
    for block in &other_blocks {
        let refs = soe_godot_host::tree_lod::read_references(
            &vfs.read(&soe_godot_host::terrain_lod::btt_path(
                "DLC2SolstheimWorld",
                *block,
            ))
            .unwrap(),
        )
        .unwrap();
        assert!(refs
            .iter()
            .all(|reference| other_types.contains_key(&reference.tree_type)));
    }
    for level in [LodLevel::L4, LodLevel::L8, LodLevel::L16] {
        let block = LodBlockCoord::containing_from(
            soe_godot_host::streaming::CellCoord::new(12, -8),
            level,
            source.lod_settings("Tamriel").unwrap().unwrap().sw_cell,
        );
        let loaded = source
            .load_object_lod_block(world, "Tamriel", block)
            .unwrap()
            .unwrap();
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for surface in &loaded.surfaces {
            for p in &surface.arrays.positions {
                let q = GodotPos::new(p[0], p[1], p[2]).to_skyrim();
                for (i, v) in [q.x, q.y, q.z].into_iter().enumerate() {
                    lo[i] = lo[i].min(v);
                    hi[i] = hi[i].max(v);
                }
            }
        }
        println!(
            "{block:?}: bounds {lo:?} {hi:?}, triangles {}",
            loaded.triangles
        );
        let path = soe_godot_host::terrain_lod::bto_path("Tamriel", block);
        let file = soe_nif::parse(&vfs.read(&path).unwrap()).unwrap();
        let raw = file.meshes().unwrap();
        let expected: Vec<_> = raw
            .iter()
            .flat_map(|mesh| {
                mesh.positions.iter().map(|p| {
                    mesh.transform
                        .apply(soe_nif::Vec3::new(p[0], p[1], p[2]))
                        .to_array()
                })
            })
            .collect();
        let actual: Vec<_> = loaded
            .surfaces
            .iter()
            .flat_map(|surface| {
                surface.arrays.positions.iter().map(|p| {
                    let q = GodotPos::new(p[0], p[1], p[2]).to_skyrim();
                    [q.x, q.y, q.z]
                })
            })
            .collect();
        assert_eq!(expected.len(), actual.len());
        for (expected, actual) in expected.iter().zip(&actual) {
            for (a, b) in expected.iter().zip(actual) {
                assert!(
                    (a - b).abs() < 0.05,
                    "BTO must apply its authored transform exactly once: {a} != {b}"
                );
            }
        }
        for mesh in raw.iter().take(2) {
            println!(
                "transform {:?} first {:?}",
                mesh.transform,
                mesh.positions.first()
            );
        }
    }
}

fn block(level: LodLevel, x: i32, y: i32) -> LodBlockCoord {
    LodBlockCoord { level, x, y }
}

#[test]
fn rnd_28_selects_every_available_level_without_overlap() {
    use soe_godot_host::streaming::CellCoord;
    use soe_godot_host::terrain_lod::select_lod_blocks;
    let catalog = vec![
        block(LodLevel::L4, 4, 0),
        block(LodLevel::L8, 16, 0),
        block(LodLevel::L16, 48, 0),
        block(LodLevel::L32, 96, 0),
        block(LodLevel::L4, 96, 0),
        block(LodLevel::L4, 0, 0),
    ];
    let selected = select_lod_blocks(&catalog, CellCoord::new(0, 0), 2, 100);
    assert_eq!(
        selected,
        vec![catalog[0], catalog[1], catalog[2], catalog[3]]
    );
    let mut reversed = catalog.clone();
    reversed.reverse();
    assert_eq!(
        selected,
        select_lod_blocks(&reversed, CellCoord::new(0, 0), 2, 100)
    );
}

#[test]
fn rnd_28_refines_coarse_blocks_and_releases_overlap_before_loading_full_cells() {
    use soe_godot_host::streaming::CellCoord;
    use soe_godot_host::terrain_lod::{TerrainLodConfig, TerrainLodStreamer};
    let coarse = block(LodLevel::L16, 48, 0);
    let fine = block(LodLevel::L4, 48, 0);
    let catalog = vec![coarse, fine, block(LodLevel::L4, 52, 0)];
    let mut streamer = TerrainLodStreamer::with_catalog(
        TerrainLodConfig {
            lod_radius: 96,
            max_loads_per_tick: 1,
            max_unloads_per_tick: 1,
            ..Default::default()
        },
        CellCoord::new(0, 0),
        catalog,
    )
    .unwrap();
    assert_eq!(streamer.settle(CellCoord::new(0, 0)).load, vec![coarse]);
    let plan = streamer.update(CellCoord::new(44, 0));
    assert_eq!(plan.unload, vec![coarse]);
    assert_eq!(plan.load, vec![fine]);
    streamer.set_full_cells(&[CellCoord::new(49, 0)]);
    assert!(streamer
        .update(CellCoord::new(44, 0))
        .unload
        .contains(&fine));
    streamer.set_full_cells(&[]);
    assert!(streamer.update(CellCoord::new(44, 0)).load.contains(&fine));
    let plan = streamer.update(CellCoord::new(49, 0));
    assert!(plan.unload.contains(&fine));
}

#[test]
fn rnd_28_missing_levels_use_available_coarse_files_and_respect_budget() {
    use soe_godot_host::streaming::CellCoord;
    use soe_godot_host::terrain_lod::{TerrainLodConfig, TerrainLodStreamer};
    let catalog = vec![block(LodLevel::L16, 16, 0), block(LodLevel::L16, 48, 0)];
    let mut streamer = TerrainLodStreamer::with_catalog(
        TerrainLodConfig {
            lod_radius: 96,
            budget_bytes: 10,
            ..Default::default()
        },
        CellCoord::new(0, 0),
        catalog,
    )
    .unwrap();
    let step = streamer.step(CellCoord::new(0, 0), true, |_| Ok::<_, String>(Some(10)));
    assert_eq!(step.loaded.len(), 2);
    assert_eq!(step.evicted, vec![block(LodLevel::L16, 48, 0)]);
    assert_eq!(streamer.usage_bytes(), 10);
    assert!(streamer
        .step(CellCoord::new(0, 0), false, |_| Ok::<_, String>(Some(10)))
        .loaded
        .is_empty());
}

fn tree_fixtures() -> (Vec<u8>, Vec<u8>) {
    let mut list = 1u32.to_le_bytes().to_vec();
    list.extend(7u32.to_le_bytes());
    for value in [100f32, 200., 0.25, 0.125, 0.75, 0.875] {
        list.extend(value.to_le_bytes());
    }
    list.extend(0u32.to_le_bytes());
    let mut refs = 1u32.to_le_bytes().to_vec();
    refs.extend(7u32.to_le_bytes());
    refs.extend(1u32.to_le_bytes());
    for value in [49152f32, -32768., 1000., std::f32::consts::FRAC_PI_2, 2.] {
        refs.extend(value.to_le_bytes());
    }
    refs.extend(0x123456u32.to_le_bytes());
    refs.extend([0; 8]);
    (list, refs)
}

#[test]
fn rnd_28_tree_atlas_indices_world_positions_rotation_scale_and_alpha_are_preserved() {
    use soe_godot_host::tree_lod::{build_billboards, read_references, read_types};
    let (list, bytes) = tree_fixtures();
    let types = read_types(&list).unwrap();
    let refs = read_references(&bytes).unwrap();
    assert_eq!(refs[0].raw_form_id, 0x123456);
    let mesh = build_billboards(&types, &refs, "textures/trees.dds").unwrap();
    assert_eq!(mesh.positions.len(), 8);
    assert_eq!(mesh.indices.len(), 12);
    assert_eq!(mesh.positions[0], [49152., -32868., 1000.]);
    assert_eq!(mesh.positions[2], [49152., -32668., 1400.]);
    assert_eq!(mesh.uvs.as_ref().unwrap()[0], [0.25, 0.875]);
    assert_eq!(mesh.uvs.as_ref().unwrap()[2], [0.75, 0.125]);
    assert_eq!(
        mesh.material.alpha,
        soe_godot_host::mesh::AlphaMode::Cutout(128)
    );
    assert!(mesh.material.double_sided);
}

#[test]
fn rnd_28_tree_files_reject_truncation_bad_counts_nonfinite_values_and_missing_types() {
    use soe_godot_host::tree_lod::{build_billboards, read_references, read_types};
    let (list, refs) = tree_fixtures();
    for end in 0..list.len() {
        assert!(read_types(&list[..end]).is_err(), "LST cut {end}");
    }
    for end in 0..refs.len() {
        assert!(read_references(&refs[..end]).is_err(), "BTT cut {end}");
    }
    assert!(read_types(&u32::MAX.to_le_bytes()).is_err());
    assert!(read_references(&u32::MAX.to_le_bytes()).is_err());
    let mut invalid = list.clone();
    invalid[8..12].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(read_types(&invalid).is_err());
    let mut invalid = refs.clone();
    invalid[28..32].copy_from_slice(&0f32.to_le_bytes());
    assert!(read_references(&invalid).is_err());
    let refs = read_references(&refs).unwrap();
    assert!(build_billboards(&std::collections::BTreeMap::default(), &refs, "a.dds").is_err());
    let mut trailing = list;
    trailing.push(0);
    assert!(read_types(&trailing).is_err());
}

#[test]
fn rnd_28_tree_atlas_accepts_authored_overscan_and_mirrored_uvs() {
    let (mut list, refs) = tree_fixtures();
    list[16..20].copy_from_slice(&(-0.0005f32).to_le_bytes());
    list[20..24].copy_from_slice(&0.9f32.to_le_bytes());
    let types = soe_godot_host::tree_lod::read_types(&list).unwrap();
    let refs = soe_godot_host::tree_lod::read_references(&refs).unwrap();
    let mesh = soe_godot_host::tree_lod::build_billboards(&types, &refs, "a.dds").unwrap();
    assert_eq!(mesh.uvs.as_ref().unwrap()[0], [-0.0005, 0.875]);
    assert_eq!(mesh.uvs.as_ref().unwrap()[2], [0.75, 0.9]);
}

fn fixture_source(root: &std::path::Path) -> DataSource {
    use soe_esm::{tag, Entry, FileHeader, Group, GroupKind, Plugin, Record};
    let mut plugin = Plugin::new(FileHeader::default());
    let mut worlds = Group::new(GroupKind::Top(tag::WRLD));
    worlds.entries.push(Entry::Record(
        plugin.push_record(Record::new(tag::WRLD, 0x100).zsub(tag::EDID, "TestWorld")),
    ));
    plugin.groups.push(worlds);
    std::fs::write(root.join("Skyrim.esm"), plugin.to_bytes()).unwrap();
    for name in [
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            root.join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
    DataSource::open(root).unwrap()
}

#[test]
fn rnd_28_archived_level_inventory_uses_loose_paths_and_ignores_unrelated_files() {
    use soe_godot_host::terrain_lod::LodKind;
    let dir = tempfile::tempdir().unwrap();
    let object = dir.path().join("meshes/terrain/testworld/objects");
    let tree = dir.path().join("meshes/terrain/testworld/trees");
    std::fs::create_dir_all(&object).unwrap();
    std::fs::create_dir_all(&tree).unwrap();
    for name in [
        "testworld.4.-8.4.bto",
        "testworld.8.-8.8.bto",
        "testworld.16.16.16.bto",
        "testworld.32.32.32.bto",
        "testworld.64.0.0.bto",
        "otherworld.4.0.0.bto",
        "testworld.4.bad.0.bto",
        "testworld.4.0.0.nif",
    ] {
        std::fs::write(object.join(name), []).unwrap();
    }
    for level in [4, 8, 16, 32] {
        std::fs::write(tree.join(format!("testworld.{level}.0.0.btt")), []).unwrap();
    }
    let source = fixture_source(dir.path());
    assert_eq!(
        source.scenery_lod_blocks("TestWorld", LodKind::Object),
        vec![
            block(LodLevel::L4, -8, 4),
            block(LodLevel::L8, -8, 8),
            block(LodLevel::L16, 16, 16),
            block(LodLevel::L32, 32, 32)
        ]
    );
    assert_eq!(
        source
            .scenery_lod_blocks("TestWorld", LodKind::Tree)
            .iter()
            .map(|b| b.level.cells())
            .collect::<Vec<_>>(),
        vec![4, 8, 16, 32]
    );
    assert!(source
        .scenery_lod_blocks("OtherWorld", LodKind::Tree)
        .is_empty());
}

#[test]
fn rnd_28_tree_loader_returns_missing_blocks_and_reports_malformed_assets() {
    let dir = tempfile::tempdir().unwrap();
    let tree = dir.path().join("meshes/terrain/testworld/trees");
    std::fs::create_dir_all(&tree).unwrap();
    let (list, refs) = tree_fixtures();
    std::fs::write(tree.join("testworld.lst"), list).unwrap();
    std::fs::write(tree.join("testworld.4.4.0.btt"), refs).unwrap();
    std::fs::write(tree.join("testworld.4.8.0.btt"), [255]).unwrap();
    let source = fixture_source(dir.path());
    let (world, _) = source.resolve_world("TestWorld").unwrap();
    let loaded = source
        .load_tree_lod_block(world, "TestWorld", block(LodLevel::L4, 4, 0))
        .unwrap()
        .unwrap();
    let p = loaded.surfaces[0].arrays.positions[0];
    let position = GodotPos::new(p[0], p[1], p[2]).to_skyrim();
    assert!((position.x - 49152.).abs() < 0.01);
    assert!((position.y + 32868.).abs() < 0.01);
    assert_eq!(loaded.triangles, 4);
    assert_eq!(loaded.authored_references, 1);
    assert_eq!(loaded.surfaces[0].reference, world);
    assert!(source
        .load_tree_lod_block(world, "TestWorld", block(LodLevel::L4, 0, 0))
        .unwrap()
        .is_none());
    assert!(matches!(
        source.load_tree_lod_block(world, "TestWorld", block(LodLevel::L4, 8, 0)),
        Err(soe_godot_host::data_source::DataSourceError::TreeLod(_))
    ));
}

#[test]
fn rnd_28_object_loader_uses_nif_coordinates_without_adding_filename_origin() {
    let dir = tempfile::tempdir().unwrap();
    let objects = dir.path().join("meshes/terrain/testworld/objects");
    std::fs::create_dir_all(&objects).unwrap();
    let mesh = soe_nif::Mesh {
        name: "absolute object".into(),
        block: 0,
        transform: soe_nif::Transform::IDENTITY,
        bound: soe_nif::Bound {
            center: soe_nif::Vec3::new(0., 0., 0.),
            radius: 0.,
        },
        positions: vec![
            [16384., 32768., 100.],
            [16484., 32768., 100.],
            [16384., 32868., 100.],
        ],
        normals: vec![],
        tangents: vec![],
        uv_sets: vec![],
        uvs: vec![],
        colors: vec![],
        indices: vec![0, 1, 2],
        bone_weights: vec![],
        bone_indices: vec![],
        skin: None,
        material: soe_nif::mesh::Material::default(),
    };
    std::fs::write(
        objects.join("testworld.4.4.8.bto"),
        soe_nif::static_mesh::export_nif(&[mesh]).unwrap(),
    )
    .unwrap();
    let source = fixture_source(dir.path());
    let (world, _) = source.resolve_world("TestWorld").unwrap();
    let loaded = source
        .load_object_lod_block(world, "TestWorld", block(LodLevel::L4, 4, 8))
        .unwrap()
        .unwrap();
    let vertex = loaded.surfaces[0].arrays.positions[0];
    let position = GodotPos::new(vertex[0], vertex[1], vertex[2]).to_skyrim();
    assert!((position.x - 16384.).abs() < 0.01);
    assert!((position.y - 32768.).abs() < 0.01);
}

#[test]
fn rnd_28_tree_block_trailers_leave_declared_reference_groups_unchanged() {
    let (_, refs) = tree_fixtures();
    let expected = soe_godot_host::tree_lod::read_references(&refs).unwrap();
    let mut bytes = refs;
    bytes.extend([7, 4, 5, 0, 255]);
    assert_eq!(
        soe_godot_host::tree_lod::read_references(&bytes).unwrap(),
        expected
    );
}

#[test]
fn rnd_28_extreme_archive_coordinates_do_not_overflow_selection_or_distance() {
    use soe_godot_host::streaming::CellCoord;
    use soe_godot_host::terrain_lod::select_lod_blocks;
    let near = block(LodLevel::L16, 16, 0);
    let blocks = vec![
        near,
        block(LodLevel::L4, i32::MAX, 0),
        block(LodLevel::L4, i32::MIN, 0),
    ];
    assert_eq!(
        select_lod_blocks(&blocks, CellCoord::new(0, 0), 2, 96),
        vec![near]
    );
    assert_eq!(
        blocks[1].ring_distance(CellCoord::new(i32::MIN, 0)),
        i32::MAX
    );
    assert_eq!(blocks[1].ring_distance(CellCoord::new(i32::MAX, 0)), 0);
}
