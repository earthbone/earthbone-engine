// SPDX-License-Identifier: GPL-3.0-or-later

//! Independent LIGH and REFR fields follow xEdit's LIGH DATA definition.

use soe_esm::{tag, Entry, FileHeader, Group, GroupKind, Plugin, Record};

#[test]
fn r4_record_light_fixture_keeps_shadow_types_and_disabled_lights() {
    let temporary = tempfile::tempdir().unwrap();
    let export = std::env::var_os("SOE_R4_LIGHT_FIXTURE");
    let directory = export
        .as_deref()
        .map(std::path::Path::new)
        .unwrap_or(temporary.path());
    std::fs::create_dir_all(directory).unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let mut lights = Group::new(GroupKind::Top(soe_esm::Tag::new(b"LIGH")));
    let mut children = Group::new(GroupKind::CellTemporary(0x100));
    for (i, flags) in [0x1008_u32, 0x400, 0, 32].into_iter().enumerate() {
        let id = 0x200 + i as u32;
        let mut data = vec![0; 48];
        data[4..8].copy_from_slice(&350_u32.to_le_bytes());
        data[8..11].copy_from_slice(&[255, 128, 32]);
        data[12..16].copy_from_slice(&flags.to_le_bytes());
        for (offset, value) in [(16, 1.0_f32), (20, 60.0), (24, 1.0), (28, 0.5), (32, 0.25)] {
            data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        let light = plugin.push_record(
            Record::new(soe_esm::Tag::new(b"LIGH"), id)
                .sub(b"DATA", data)
                .sub(b"FNAM", 2.0_f32.to_le_bytes()),
        );
        lights.entries.push(Entry::Record(light));
        let transform = [56.0_f32, -105.0, 49.0, 0.0, 0.0, 0.0]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>();
        let reference = plugin.push_record(
            Record::new(tag::REFR, id + 0x100)
                .sub(b"NAME", id.to_le_bytes())
                .sub(b"DATA", transform),
        );
        children.entries.push(Entry::Record(reference));
    }
    let cell = plugin.push_record(
        Record::new(tag::CELL, 0x100)
            .zsub(tag::EDID, "R4Lights")
            .sub(b"DATA", [1, 0]),
    );
    let mut cells = Group::new(GroupKind::Top(tag::CELL));
    cells.entries.push(Entry::Record(cell));
    let mut cell_children = Group::new(GroupKind::CellChildren(0x100));
    cell_children.entries.push(Entry::Group(children));
    cells.entries.push(Entry::Group(cell_children));
    plugin.groups.extend([lights, cells]);
    std::fs::write(directory.join("Skyrim.esm"), plugin.to_bytes()).unwrap();
    for name in [
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            directory.join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
    let source = super::DataSource::open(directory).unwrap();
    let world = soe_game::World::from_shared_order(source.shared_order());
    let cell = source.resolve_cell("R4Lights").unwrap();
    assert_eq!(world.light_references_in_cell(cell).len(), 4);
    assert_eq!(source.stream_cell_id(cell).unwrap().surfaces.len(), 0);
}
