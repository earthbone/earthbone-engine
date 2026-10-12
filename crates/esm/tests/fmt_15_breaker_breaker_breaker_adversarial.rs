#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use soe_esm::{FileHeader, Plugin, Record, Tag};

#[test]
fn fmt_15_npc_head_part_links_are_remapped_after_reload() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(b"NPC_", 0x800)
            .sub(b"PNAM", 0x801u32.to_le_bytes())
            .sub(b"PNAM", 0x802u32.to_le_bytes()),
    );

    plugin
        .remap_form_ids(&HashMap::from([(0x801, 0x901), (0x802, 0x902)]))
        .unwrap();
    let reloaded = Plugin::parse(&plugin.to_bytes()).unwrap();
    let npc = &reloaded.records[0];
    let head_parts: Vec<_> = npc
        .all(Tag::new(b"PNAM"))
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
        .collect();

    assert_eq!(head_parts, [0x901, 0x902]);
}

#[test]
fn fmt_15_equipment_slot_parent_links_are_remapped_after_reload() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"EQUP", 0x800).sub(
        b"PNAM",
        [0x801u32.to_le_bytes(), 0x802u32.to_le_bytes()].concat(),
    ));

    plugin
        .remap_form_ids(&HashMap::from([(0x801, 0x901), (0x802, 0x902)]))
        .unwrap();
    let reloaded = Plugin::parse(&plugin.to_bytes()).unwrap();
    assert_eq!(
        reloaded.records[0].find(Tag::new(b"PNAM")).unwrap(),
        &[0x901u32.to_le_bytes(), 0x902u32.to_le_bytes()].concat()
    );
}
