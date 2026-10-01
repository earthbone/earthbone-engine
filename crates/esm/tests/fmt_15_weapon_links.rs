#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use soe_esm::{FileHeader, Plugin, Record, Tag};

#[test]
fn fmt_15_weapon_xedit_links_remap_after_serialization() {
    let tags = [
        b"EITM", b"BIDS", b"BAMT", b"EFSD", b"INAM", b"WNAM", b"SNAM", b"XNAM", b"NAM7", b"TNAM",
        b"UNAM", b"NAM9", b"NAM8", b"CNAM",
    ];
    let mut weapon = Record::new(b"WEAP", 0x800);
    for (index, tag) in tags.iter().enumerate() {
        weapon = weapon.sub(*tag, (0x801 + index as u32).to_le_bytes());
    }
    weapon = weapon.sub(
        b"KWDA",
        [0x821u32.to_le_bytes(), 0x822u32.to_le_bytes()].concat(),
    );
    let mut critical = vec![0x55; 24];
    critical[16..20].copy_from_slice(&0x820u32.to_le_bytes());
    let mut destruction = vec![0x44; 20];
    destruction[8..12].copy_from_slice(&0x821u32.to_le_bytes());
    destruction[12..16].copy_from_slice(&0x822u32.to_le_bytes());
    weapon = weapon
        .sub(b"CRDT", critical.clone())
        .sub(b"DSTD", destruction.clone());

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(weapon);
    let mapping = (0..=0x22)
        .map(|offset| (0x800 + offset, 0x900 + offset))
        .collect::<HashMap<_, _>>();
    plugin.remap_form_ids(&mapping).unwrap();

    let reloaded = Plugin::parse(&plugin.to_bytes()).unwrap();
    let weapon = &reloaded.records[0];
    for (index, tag) in tags.iter().enumerate() {
        assert_eq!(
            weapon.find(Tag::new(tag)).unwrap(),
            &(0x901 + index as u32).to_le_bytes(),
            "{} link should be remapped",
            std::str::from_utf8(*tag).unwrap()
        );
    }
    let expected = 0x920u32.to_le_bytes();
    let actual = weapon.find(Tag::new(b"CRDT")).unwrap();
    assert_eq!(&actual[..16], &critical[..16]);
    assert_eq!(&actual[16..20], &expected);
    assert_eq!(&actual[20..], &critical[20..]);
    assert_eq!(
        weapon.find(Tag::new(b"KWDA")).unwrap(),
        &[0x921u32.to_le_bytes(), 0x922u32.to_le_bytes()].concat()
    );
    let actual = weapon.find(Tag::new(b"DSTD")).unwrap();
    assert_eq!(&actual[..8], &destruction[..8]);
    assert_eq!(&actual[8..12], &0x921u32.to_le_bytes());
    assert_eq!(&actual[12..16], &0x922u32.to_le_bytes());
    assert_eq!(&actual[16..], &destruction[16..]);
}

#[test]
fn fmt_15_weapon_critical_effect_accepts_legacy_layout() {
    let mut critical = vec![0x33; 20];
    critical[12..16].copy_from_slice(&0x801u32.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"WEAP", 0x800).sub(b"CRDT", critical));
    plugin
        .remap_form_ids(&HashMap::from([(0x801, 0x901)]))
        .unwrap();
    assert_eq!(
        &plugin.records[0].find(Tag::new(b"CRDT")).unwrap()[12..16],
        &0x901u32.to_le_bytes()
    );
}

#[test]
fn fmt_15_weapon_malformed_critical_effect_leaves_plugin_unchanged() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"WEAP", 0x800).sub(b"CRDT", [0x55; 23]));
    let before = plugin.clone();
    assert!(plugin
        .remap_form_ids(&HashMap::from([(0x800, 0x900)]))
        .is_err());
    assert_eq!(plugin, before);
}
