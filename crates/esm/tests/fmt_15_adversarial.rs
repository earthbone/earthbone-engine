#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use soe_esm::{FileHeader, Plugin, Record, Tag};

fn push_object(data: &mut Vec<u8>, format: u16, form_id: u32) -> usize {
    if format == 2 {
        data.extend_from_slice(&0xfeedu16.to_le_bytes());
        data.extend_from_slice(&(-7i16).to_le_bytes());
        let at = data.len();
        data.extend_from_slice(&form_id.to_le_bytes());
        at
    } else {
        let at = data.len();
        data.extend_from_slice(&form_id.to_le_bytes());
        data.extend_from_slice(&(-7i16).to_le_bytes());
        data.extend_from_slice(&0xfeedu16.to_le_bytes());
        at
    }
}

fn nested_alias_vmad(root_format: u16, alias_format: u16) -> (Vec<u8>, [usize; 2]) {
    let mut data = Vec::new();
    data.extend_from_slice(&5u16.to_le_bytes());
    data.extend_from_slice(&root_format.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes()); // root scripts
    data.push(2); // extra bind data version
    data.extend_from_slice(&0u16.to_le_bytes()); // fragments
    data.extend_from_slice(&0u16.to_le_bytes()); // fragment file name
    data.extend_from_slice(&1u16.to_le_bytes()); // aliases
    let alias_at = push_object(&mut data, root_format, 0x801);
    data.extend_from_slice(&5u16.to_le_bytes()); // alias VMAD version
    data.extend_from_slice(&alias_format.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes()); // alias scripts
    data.extend_from_slice(&5u16.to_le_bytes());
    data.extend_from_slice(b"Alias");
    data.push(0); // script flags
    data.extend_from_slice(&1u16.to_le_bytes()); // properties
    data.extend_from_slice(&6u16.to_le_bytes());
    data.extend_from_slice(b"Target");
    data.extend_from_slice(&[1, 0]); // object property, flags
    let property_at = push_object(&mut data, alias_format, 0x802);
    (data, [alias_at, property_at])
}

#[test]
fn fmt_15_maps_nested_quest_alias_objects_in_each_object_format() {
    let mapping = HashMap::from([(0x801, 0x901), (0x802, 0x902)]);
    for (root_format, alias_format) in [(1, 2), (2, 1)] {
        let (original, links) = nested_alias_vmad(root_format, alias_format);
        let mut expected = original.clone();
        expected[links[0]..links[0] + 4].copy_from_slice(&0x901u32.to_le_bytes());
        expected[links[1]..links[1] + 4].copy_from_slice(&0x902u32.to_le_bytes());

        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(b"QUST", 0x800).sub(b"VMAD", original));
        plugin.remap_form_ids(&mapping).unwrap();
        let reloaded = Plugin::parse(&plugin.to_bytes()).unwrap();
        assert_eq!(
            reloaded.records[0].find(Tag::new(b"VMAD")).unwrap(),
            expected
        );
    }
}

#[test]
fn fmt_15_malformed_late_vmad_keeps_prior_record_ids_and_links_unchanged() {
    let mapping = HashMap::from([(0x800, 0x900), (0x801, 0x901)]);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"REFR", 0x800).sub(b"NAME", 0x801u32.to_le_bytes()));
    plugin.push_top(Record::new(b"QUST", 0x802).sub(b"VMAD", [5, 0, 2, 0, 1]));
    let before = plugin.clone();

    assert!(plugin.remap_form_ids(&mapping).is_err());
    assert_eq!(plugin, before);
}
