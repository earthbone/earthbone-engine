// SPDX-License-Identifier: GPL-3.0-or-later

//! Generate a flat exterior grid for godot/exterior_stream_test.gd.
//! Uses the existing ESM writer and LAND height encoder. No installed assets.

use std::path::PathBuf;

use soe_esm::records::land::{Heights, VERTS};
use soe_esm::{tag, Entry, FileHeader, Group, GroupKind, Plugin, Record};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("provide a new output directory")?,
    );
    std::fs::create_dir(&destination)?;
    let mut plugin = Plugin::new(FileHeader::default());
    let world = plugin.push_record(Record::new(tag::WRLD, 0x100).zsub(tag::EDID, "StreamingWorld"));
    let persistent = plugin.push_record(Record::new(tag::CELL, 0x200).sub(b"DATA", [0, 0]));
    let mut children = Group::new(GroupKind::WorldChildren(0x100));
    children.entries.push(Entry::Record(persistent));
    let mut block = Group::new(GroupKind::ExteriorBlock { x: 0, y: 0 });
    let mut sub = Group::new(GroupKind::ExteriorSubBlock { x: 0, y: 0 });
    for y in -2_i32..=2 {
        for x in -2_i32..=5 {
            let id = 0x300 + u32::try_from((y + 2) * 8 + x + 2)?;
            let mut grid = x.to_le_bytes().to_vec();
            grid.extend_from_slice(&y.to_le_bytes());
            grid.extend_from_slice(&0_u32.to_le_bytes());
            let cell = plugin.push_record(
                Record::new(tag::CELL, id)
                    .zsub(tag::EDID, &format!("Grid{x}_{y}"))
                    .sub(b"DATA", [0, 0])
                    .sub(b"XCLC", grid),
            );
            let land = plugin.push_record(
                Record::new(tag::LAND, id + 0x100)
                    .sub(b"DATA", 1_u32.to_le_bytes())
                    .sub(b"VHGT", Heights::encode(&vec![0.0; VERTS]).to_bytes()),
            );
            let mut temporary = Group::new(GroupKind::CellTemporary(id));
            temporary.entries.push(Entry::Record(land));
            let mut cell_children = Group::new(GroupKind::CellChildren(id));
            cell_children.entries.push(Entry::Group(temporary));
            sub.entries.push(Entry::Record(cell));
            sub.entries.push(Entry::Group(cell_children));
        }
    }
    block.entries.push(Entry::Group(sub));
    children.entries.push(Entry::Group(block));
    let mut top = Group::new(GroupKind::Top(tag::WRLD));
    top.entries.push(Entry::Record(world));
    top.entries.push(Entry::Group(children));
    plugin.groups.push(top);
    std::fs::write(destination.join("Skyrim.esm"), plugin.to_bytes())?;
    for name in [
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            destination.join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )?;
    }
    println!("Exterior fixture: {}", destination.display());
    Ok(())
}
