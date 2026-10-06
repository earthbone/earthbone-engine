// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Source-grounded checks for RND-29 distant water.

use std::path::PathBuf;

use soe_esm::{tag, Entry, FileHeader, GlobalFormId, Group, GroupKind, Plugin, Record};
use soe_godot_host::data_source::DataSource;
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel};
use soe_godot_host::units::METRES_PER_UNIT;

fn water_source() -> (tempfile::TempDir, DataSource, GlobalFormId) {
    let directory =
        tempfile::tempdir_in(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"))
            .unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let mut top = Group::new(GroupKind::Top(tag::WRLD));
    let world_id = 0x100;
    let world = Record::new(tag::WRLD, world_id)
        .zsub(tag::EDID, "WaterWorld")
        .sub(b"DATA", [0u8])
        .sub(b"DNAM", [0.0f32.to_le_bytes(), 500.0f32.to_le_bytes()].concat())
        .sub(b"NAM4", 700.0f32.to_le_bytes());
    top.entries.push(Entry::Record(plugin.push_record(world)));
    let mut children = Group::new(GroupKind::WorldChildren(world_id));
    let mut block = Group::new(GroupKind::ExteriorBlock { x: 0, y: 0 });
    let mut sub = Group::new(GroupKind::ExteriorSubBlock { x: 0, y: 0 });
    sub.entries.push(Entry::Record(plugin.push_record(
        Record::new(tag::CELL, 0x200)
            .zsub(tag::EDID, "OpenWater")
            .sub(b"DATA", 0x0002u16.to_le_bytes())
            .sub(b"XCLC", [0i32.to_le_bytes(), 0i32.to_le_bytes()].concat()),
    )));
    block.entries.push(Entry::Group(sub));
    children.entries.push(Entry::Group(block));
    top.entries.push(Entry::Group(children));
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
    let id = source.resolve_world("WaterWorld").unwrap().0;
    (directory, source, id)
}

fn lod_height(source: &DataSource, world: GlobalFormId) -> f32 {
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
    assert!(
        !surfaces.is_empty(),
        "water cell produces a distant plane, world={world}, cell={:?}",
        source.cell_index().exterior_cell(world, 0, 0)
    );
    surfaces[0].arrays.positions[0][1]
}

#[test]
fn rnd_29_distant_water_uses_wrld_lod_water_height() {
    let (_directory, source, world) = water_source();
    let got = lod_height(&source, world);
    let expected = 700.0 * METRES_PER_UNIT;
    assert!(
        (got - expected).abs() < 0.001,
        "expected WRLD NAM4 height {expected} m, observed {got} m"
    );
}
