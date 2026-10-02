// SPDX-License-Identifier: GPL-3.0-or-later

//! Synthetic masters for native menu runtime checks, with linked installed meshes.

use soe_esm::{Entry, FileHeader, Group, GroupKind, Plugin, Record, Tag};
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let t = Tag::new;
    let args: Vec<_> = std::env::args().collect();
    let output = args
        .get(1)
        .ok_or("usage: inventory_fixture OUTPUT SKYRIM_DATA")?;
    let data = args
        .get(2)
        .ok_or("usage: inventory_fixture OUTPUT SKYRIM_DATA")?;
    let root = Path::new(output);
    std::fs::create_dir_all(root)?;
    let mut p = Plugin::new(FileHeader::default());
    p.push_top(
        Record::new(t(b"NPC_"), 7)
            .zsub(t(b"EDID"), "Player")
            .sub(t(b"ACBS"), vec![0; 24])
            .sub(t(b"RNAM"), 0x100_u32.to_le_bytes())
            .sub(
                t(b"CNTO"),
                [0x500_u32.to_le_bytes(), 2_i32.to_le_bytes()].concat(),
            )
            .sub(
                t(b"CNTO"),
                [0x501_u32.to_le_bytes(), 3_i32.to_le_bytes()].concat(),
            )
            .sub(
                t(b"CNTO"),
                [0x512_u32.to_le_bytes(), 2_i32.to_le_bytes()].concat(),
            ),
    );
    p.push_top(Record::new(t(b"RACE"), 0x100).sub(t(b"WNAM"), 0x200_u32.to_le_bytes()));
    p.push_top(Record::new(t(b"WEAP"), 0x500).zsub(t(b"FULL"), "Fixture sword"));
    p.push_top(
        Record::new(t(b"ALCH"), 0x501)
            .zsub(t(b"FULL"), "Fixture bread")
            .sub(
                t(b"ENIT"),
                [0_u32.to_le_bytes(), 2_u32.to_le_bytes()].concat(),
            ),
    );
    p.push_top(
        Record::new(t(b"MISC"), 0x512)
            .zsub(t(b"EDID"), "Lockpick")
            .zsub(t(b"FULL"), "Lockpick"),
    );
    p.push_top(Record::new(t(b"DOOR"), 0x510).zsub(t(b"EDID"), "Hst03FixtureDoor"));
    let mut armor = Record::new(t(b"ARMO"), 0x200);
    for (i, (name, mask)) in [
        ("malebody_1.nif", 4_u32),
        ("malehands_1.nif", 8),
        ("malefeet_1.nif", 128),
    ]
    .iter()
    .enumerate()
    {
        let id = 0x300 + i as u32;
        armor = armor.sub(t(b"MODL"), id.to_le_bytes());
        p.push_top(
            Record::new(t(b"ARMA"), id)
                .sub(t(b"RNAM"), 0x100_u32.to_le_bytes())
                .sub(t(b"DNAM"), vec![0; 12])
                .sub(t(b"BOD2"), mask.to_le_bytes())
                .zsub(
                    t(b"MOD2"),
                    &format!("actors/character/character assets/{name}"),
                ),
        );
    }
    p.push_top(armor);
    p.push_top(Record::new(t(b"STAT"), 0x400).zsub(
        t(b"MODL"),
        "architecture/whiterun/wrinteriors/wrintfloormid01.nif",
    ));
    let reference = Record::new(t(b"REFR"), 0x401)
        .sub(t(b"NAME"), 0x400_u32.to_le_bytes())
        .sub(t(b"DATA"), vec![0; 24]);
    let mut lock = vec![0; 20];
    lock[0] = 1;
    let lockpick_door = Record::new(t(b"REFR"), 0x511)
        .sub(t(b"NAME"), 0x510_u32.to_le_bytes())
        .sub(t(b"DATA"), vec![0; 24])
        .sub(t(b"XLOC"), lock);
    let mut key_only_lock = vec![0; 20];
    key_only_lock[0] = 255;
    let key_only_door = Record::new(t(b"REFR"), 0x513)
        .sub(t(b"NAME"), 0x510_u32.to_le_bytes())
        .sub(t(b"DATA"), vec![0; 24])
        .sub(t(b"XLOC"), key_only_lock);
    let cell = Record::new(t(b"CELL"), 0x800)
        .zsub(t(b"EDID"), "WhiterunBanneredMare")
        .sub(t(b"DATA"), 1_u16.to_le_bytes());
    let mut temp = Group::new(GroupKind::CellTemporary(0x800));
    temp.entries.push(Entry::Record(p.push_record(reference)));
    temp.entries
        .push(Entry::Record(p.push_record(lockpick_door)));
    temp.entries
        .push(Entry::Record(p.push_record(key_only_door)));
    let mut children = Group::new(GroupKind::CellChildren(0x800));
    children.entries.push(Entry::Group(temp));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(p.push_record(cell)));
    sub.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(sub));
    let mut top = Group::new(GroupKind::Top(t(b"CELL")));
    top.entries.push(Entry::Group(block));
    p.groups.push(top);
    std::fs::write(root.join("Skyrim.esm"), p.to_bytes())?;
    for name in [
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            root.join(name),
            Plugin::new(FileHeader {
                masters: vec!["Skyrim.esm".into()],
                ..Default::default()
            })
            .to_bytes(),
        )?;
    }
    #[cfg(unix)]
    for name in [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Animations.bsa",
    ] {
        let source = Path::new(data).join(name).canonicalize()?;
        let destination = root.join(name);
        if !destination.exists() {
            std::os::unix::fs::symlink(source, destination)?;
        }
    }
    #[cfg(not(unix))]
    let _ = data;
    Ok(())
}
