// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-19 world inheritance and near-cell handoff from xEdit WRLD PNAM
//! definitions and the host's full-resolution streaming contract.

use soe_esm::{tag, Entry, FileHeader, GlobalFormId, Group, GroupKind, Plugin, Record};
use soe_godot_host::data_source::DataSource;
use soe_godot_host::streaming::CellCoord;
use soe_godot_host::terrain_lod::{LodLevel, TerrainLodConfig, TerrainLodStreamer};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture(flags: u16, parent: u32) -> (tempfile::TempDir, DataSource) {
    let directory = tempfile::tempdir().unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let mut worlds = Group::new(GroupKind::Top(tag::WRLD));
    for record in [
        Record::new(tag::WRLD, 0x100).zsub(tag::EDID, "Parent"),
        Record::new(tag::WRLD, 0x101)
            .zsub(tag::EDID, "Child")
            .sub(b"WNAM", parent.to_le_bytes())
            .sub(b"PNAM", flags.to_le_bytes()),
        Record::new(tag::WRLD, 0x102)
            .zsub(tag::EDID, "Grandchild")
            .sub(b"WNAM", 0x101u32.to_le_bytes())
            .sub(b"PNAM", 2u16.to_le_bytes()),
    ] {
        worlds
            .entries
            .push(Entry::Record(plugin.push_record(record)));
    }
    plugin.groups.push(worlds);
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
fn rnd_19_lod_inheritance_is_independent_of_land_inheritance() {
    for (flags, object, name) in [
        (0, 0x101, "Child"),
        (1, 0x101, "Child"),
        (2, 0x100, "Parent"),
        (3, 0x100, "Parent"),
    ] {
        let (_directory, source) = fixture(flags, 0x100);
        assert_eq!(
            source.lod_worldspace(id(0x101)),
            Some((id(object), name.into())),
            "PNAM {flags}"
        );
    }
}

#[test]
fn rnd_19_lod_inheritance_follows_nested_parent_worlds() {
    let (_directory, source) = fixture(2, 0x100);
    assert_eq!(
        source.lod_worldspace(id(0x102)),
        Some((id(0x100), "Parent".into()))
    );
}

#[test]
fn rnd_19_invalid_lod_parent_returns_no_asset_identity() {
    for parent in [0x101, 0x102, 0x999] {
        let (_directory, source) = fixture(2, parent);
        assert_eq!(source.lod_worldspace(id(0x101)), None, "parent {parent:x}");
    }
}

#[test]
fn rnd_19_near_handoff_releases_every_overlap_before_loading_full_cells() {
    let config = TerrainLodConfig {
        level: LodLevel::L4,
        full_res_radius: 2,
        lod_radius: 16,
        max_unloads_per_tick: 1,
        ..Default::default()
    };
    let mut streamer = TerrainLodStreamer::new(config).unwrap();
    streamer.settle(CellCoord::new(0, 0));
    let moved = CellCoord::new(8, 8);
    let overlap: Vec<_> = streamer
        .resident_blocks()
        .filter(|block| block.ring_distance(moved) <= 2)
        .collect();
    assert!(overlap.len() > config.max_unloads_per_tick);
    let plan = streamer.update(moved);
    assert!(
        overlap.iter().all(|block| plan.unload.contains(block)),
        "near handoff cannot wait for the distant unload quota"
    );
    assert!(streamer
        .resident_blocks()
        .all(|block| block.ring_distance(moved) > 2));
    assert!(
        plan.unload
            .iter()
            .filter(|block| block.ring_distance(moved) > 16)
            .count()
            <= 1
    );
}
