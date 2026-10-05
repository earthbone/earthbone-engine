// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for gameplay-host LOD world selection and focus updates.

use std::path::PathBuf;

use soe_esm::records::wrld::World;
use soe_esm::{tag, GlobalFormId};
use soe_godot_host::data_source::DataSource;
use soe_godot_host::streaming::CellCoord;
use soe_godot_host::terrain_lod::{LodLevel, TerrainLodConfig, TerrainLodStreamer};

fn data_path() -> Option<PathBuf> {
    std::env::var_os("SKYRIM_DATA").map(PathBuf::from)
}

#[test]
fn lod_parent_selection_uses_the_wrld_use_lod_flag() {
    let Some(path) = data_path() else {
        eprintln!("SKYRIM_DATA not set, skipping installed WRLD LOD inheritance check");
        return;
    };
    let source = DataSource::open(&path).expect("installed Data directory opens");
    let mut order = soe_esm::LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let plugin = soe_esm::read_plugin(path.join(name))
            .unwrap_or_else(|error| panic!("{name} reads: {error}"))
            .unwrap_or_else(|error| panic!("{name} parses: {error}"));
        order
            .add(name, plugin)
            .unwrap_or_else(|error| panic!("{name} joins the base load order: {error}"));
    }

    let mut checked = 0;
    for (child_id, record) in order.winners_of(tag::WRLD) {
        let record_ref = order.winner_ref(child_id).expect("WRLD winner reference");
        let localized = order.file(record_ref.file).plugin.header.is_localized();
        let world = World::from_record(record, localized).expect("winning WRLD parses");
        let (Some(editor_id), Some(parent), Some(flags)) =
            (&world.editor_id, world.parent, world.parent_flags)
        else {
            continue;
        };
        let parent_id = order.resolve(record_ref.file, parent);
        let parent_editor_id = order
            .winner(parent_id)
            .and_then(|parent_record| parent_record.editor_id())
            .expect("parent worldspace resolves in the base load order");
        let (expected_id, expected_editor_id) = if flags & 0x0002 != 0 {
            (parent_id, parent_editor_id)
        } else {
            (child_id, editor_id.clone())
        };
        let actual = source
            .lod_worldspace(child_id)
            .expect("installed child WRLD resolves for LOD");
        assert_eq!(
            actual,
            (expected_id, expected_editor_id.to_owned()),
            "{} must follow its PNAM Use LOD Data bit",
            editor_id
        );
        checked += 1;
    }
    assert!(checked > 0, "Skyrim.esm should include parented WRLD records");
}

#[test]
fn unknown_worldspace_has_no_lod_identity() {
    let Some(path) = data_path() else {
        eprintln!("SKYRIM_DATA not set, skipping unknown worldspace LOD lookup");
        return;
    };
    let source = DataSource::open(&path).expect("installed Data directory opens");
    assert_eq!(
        source.lod_worldspace(GlobalFormId {
            file: 0,
            object: 0x00ff_fffe,
        }),
        None
    );
}

#[test]
fn lod_focus_move_releases_departed_ring_and_fills_new_ring() {
    let config = TerrainLodConfig {
        level: LodLevel::L4,
        full_res_radius: 1,
        lod_radius: 8,
        ..Default::default()
    };
    let mut streamer = TerrainLodStreamer::new(config).expect("valid radii");
    let first = CellCoord::new(-17, 23);
    assert!(!streamer.settle(first).load.is_empty());
    let old = streamer.resident_blocks().collect::<std::collections::HashSet<_>>();
    let moved = CellCoord::new(29, -11);
    let plan = streamer.settle(moved);

    assert!(!plan.load.is_empty());
    assert!(plan.unload.iter().any(|block| old.contains(block)));
    assert!(streamer.resident_blocks().all(|block| {
        let distance = block.ring_distance(moved);
        distance > config.full_res_radius && distance <= config.lod_radius
    }));
}

#[test]
fn lod_radius_must_extend_beyond_full_resolution_coverage() {
    for radius in [-1, 2] {
        let result = TerrainLodStreamer::new(TerrainLodConfig {
            level: LodLevel::L4,
            full_res_radius: 2,
            lod_radius: radius,
            ..Default::default()
        });
        assert!(result.is_err(), "invalid LOD radius {radius} was accepted");
    }
}
