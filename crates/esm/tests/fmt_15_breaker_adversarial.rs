// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use soe_esm::{FileHeader, Plugin, Record, Tag};

#[test]
fn fmt_15_weapon_equipment_type_is_remapped() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"WEAP", 0x800).sub(b"ETYP", 0x801u32.to_le_bytes()));

    plugin.remap_form_ids(&HashMap::from([(0x801, 0x901)])).unwrap();

    assert_eq!(
        plugin.records[0].find(Tag::new(b"ETYP")),
        Some(0x901u32.to_le_bytes().as_slice())
    );
}

#[test]
fn fmt_15_weapon_pickup_sound_is_remapped() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"WEAP", 0x800).sub(b"YNAM", 0x801u32.to_le_bytes()));
    plugin.remap_form_ids(&HashMap::from([(0x801, 0x901)])).unwrap();

    assert_eq!(
        plugin.records[0].find(Tag::new(b"YNAM")),
        Some(0x901u32.to_le_bytes().as_slice())
    );
}

#[test]
fn fmt_15_weapon_putdown_sound_is_remapped() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"WEAP", 0x800).sub(b"ZNAM", 0x801u32.to_le_bytes()));
    plugin.remap_form_ids(&HashMap::from([(0x801, 0x901)])).unwrap();

    assert_eq!(
        plugin.records[0].find(Tag::new(b"ZNAM")),
        Some(0x901u32.to_le_bytes().as_slice())
    );
}

#[test]
fn fmt_15_repeating_same_mapping_does_not_change_remapped_links() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(b"WEAP", 0x800)
            .sub(b"EITM", 0x801u32.to_le_bytes())
            .sub(b"WNAM", 0x802u32.to_le_bytes()),
    );
    let mapping = HashMap::from([(0x800, 0x900), (0x801, 0x901), (0x802, 0x902)]);

    plugin.remap_form_ids(&mapping).unwrap();
    let once = plugin.clone();
    plugin.remap_form_ids(&mapping).unwrap();

    assert_eq!(plugin, once);
    assert_eq!(plugin.records[0].form_id, 0x900);
    assert_eq!(plugin.records[0].find(Tag::new(b"EITM")), Some(0x901u32.to_le_bytes().as_slice()));
    assert_eq!(plugin.records[0].find(Tag::new(b"WNAM")), Some(0x902u32.to_le_bytes().as_slice()));
}

#[test]
fn fmt_15_dstd_rejects_malformed_layout_without_partial_record_changes() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(b"WEAP", 0x800)
            .sub(b"EITM", 0x801u32.to_le_bytes())
            .sub(b"DSTD", [0x44; 19]),
    );
    let before = plugin.clone();

    assert!(plugin
        .remap_form_ids(&HashMap::from([(0x800, 0x900), (0x801, 0x901)]))
        .is_err());
    assert_eq!(plugin, before);
}
