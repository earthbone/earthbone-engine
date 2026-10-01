// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use soe_esm::{FileHeader, Plugin, Record, Tag};

fn assert_remapped(record: Record, tag: &[u8; 4], old: u32, new: u32) {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(record.sub(*tag, old.to_le_bytes()));
    plugin
        .remap_form_ids(&HashMap::from([(old, new)]))
        .unwrap();
    assert_eq!(
        plugin.records[0].find(Tag::new(tag)),
        Some(new.to_le_bytes().as_slice()),
        "{} is a FormID link",
        std::str::from_utf8(tag).unwrap()
    );
}

#[test]
fn fmt_15_weapon_equipment_type_remaps() {
    assert_remapped(Record::new(b"WEAP", 0x800), b"ETYP", 0x801, 0x901);
}

#[test]
fn fmt_15_weapon_pickup_sound_remaps() {
    assert_remapped(Record::new(b"WEAP", 0x800), b"YNAM", 0x801, 0x901);
}

#[test]
fn fmt_15_weapon_putdown_sound_remaps() {
    assert_remapped(Record::new(b"WEAP", 0x800), b"ZNAM", 0x801, 0x901);
}

#[test]
fn fmt_15_armor_race_remaps() {
    assert_remapped(Record::new(b"ARMO", 0x800), b"RNAM", 0x801, 0x901);
}

#[test]
fn fmt_15_armor_template_remaps() {
    assert_remapped(Record::new(b"ARMO", 0x800), b"TNAM", 0x801, 0x901);
}

#[test]
fn fmt_15_scalar_data_is_not_remapped_as_a_form_id() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"WEAP", 0x800).sub(b"DATA", [1, 2, 3, 4]));
    plugin
        .remap_form_ids(&HashMap::from([(0x0403_0201, 0x9999)]))
        .unwrap();
    assert_eq!(
        plugin.records[0].find(Tag::new(b"DATA")),
        Some(&[1, 2, 3, 4][..])
    );
}
