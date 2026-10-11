#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use soe_esm::{FileHeader, Plugin, Record, Tag};

fn mapping() -> HashMap<u32, u32> {
    HashMap::from([(0x801, 0x901), (0x802, 0x902)])
}

fn word(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().unwrap())
}

#[test]
fn fmt_15_repeated_npc_head_parts_are_remapped_after_reload() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(b"NPC_", 0x800)
            .sub(b"PNAM", 0x801u32.to_le_bytes())
            .sub(b"PNAM", 0x802u32.to_le_bytes()),
    );

    plugin.remap_form_ids(&mapping()).unwrap();
    let reloaded = Plugin::parse(&plugin.to_bytes()).unwrap();
    let head_parts: Vec<_> = reloaded.records[0]
        .all(Tag::new(b"PNAM"))
        .map(word)
        .collect();
    assert_eq!(head_parts, [0x901, 0x902]);
}

#[test]
fn fmt_15_repeated_equipment_slot_parents_are_remapped_after_reload() {
    let parents = [0x801u32.to_le_bytes(), 0x802u32.to_le_bytes()].concat();
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"EQUP", 0x800).sub(b"PNAM", parents));

    plugin.remap_form_ids(&mapping()).unwrap();
    let reloaded = Plugin::parse(&plugin.to_bytes()).unwrap();
    assert_eq!(
        reloaded.records[0].find(Tag::new(b"PNAM")).unwrap(),
        &[0x901u32.to_le_bytes(), 0x902u32.to_le_bytes()].concat()
    );
}

#[test]
fn fmt_15_legacy_vmad_v2_object_link_is_remapped() {
    let mut vmad = Vec::new();
    vmad.extend_from_slice(&2u16.to_le_bytes()); // VMAD v2 has no status bytes.
    vmad.extend_from_slice(&1u16.to_le_bytes()); // Object format 1.
    vmad.extend_from_slice(&1u16.to_le_bytes()); // One script.
    vmad.extend_from_slice(&1u16.to_le_bytes());
    vmad.extend_from_slice(b"S");
    vmad.extend_from_slice(&1u16.to_le_bytes()); // One property.
    vmad.extend_from_slice(&1u16.to_le_bytes());
    vmad.extend_from_slice(b"P");
    vmad.push(1); // Object property.
    vmad.extend_from_slice(&0x801u32.to_le_bytes());
    vmad.extend_from_slice(&u16::MAX.to_le_bytes()); // Alias ID -1.
    vmad.extend_from_slice(&0u16.to_le_bytes());

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"ACTI", 0x800).sub(b"VMAD", vmad));
    plugin.remap_form_ids(&mapping()).unwrap();

    let reloaded = Plugin::parse(&plugin.to_bytes()).unwrap();
    let data = reloaded.records[0].find(Tag::new(b"VMAD")).unwrap();
    assert_eq!(word(&data[data.len() - 8..data.len() - 4]), 0x901);
}

#[test]
fn fmt_15_unknown_subrecord_bytes_remain_unchanged() {
    let opaque = [0x801u32.to_le_bytes(), 0x802u32.to_le_bytes()].concat();
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"ACTI", 0x800).sub(b"ZZZZ", opaque.clone()));

    plugin.remap_form_ids(&mapping()).unwrap();
    let reloaded = Plugin::parse(&plugin.to_bytes()).unwrap();
    assert_eq!(reloaded.records[0].find(Tag::new(b"ZZZZ")).unwrap(), opaque);
}
