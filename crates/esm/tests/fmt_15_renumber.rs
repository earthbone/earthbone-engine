#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use soe_esm::{Entry, FileHeader, Group, GroupKind, Plugin, Record, Tag};

fn vmad(object_format: u16, values: &[u32]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&5u16.to_le_bytes());
    data.extend_from_slice(&object_format.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes()); // scripts
    data.extend_from_slice(&1u16.to_le_bytes());
    data.push(b'S');
    data.push(0); // script flags
    data.extend_from_slice(&2u16.to_le_bytes()); // properties
    data.extend_from_slice(&1u16.to_le_bytes());
    data.push(b'A');
    data.extend_from_slice(&[1, 0]); // object property, flags
    if object_format == 2 {
        data.extend_from_slice(&0xfeedu16.to_le_bytes());
        data.extend_from_slice(&(-2i16).to_le_bytes());
    }
    data.extend_from_slice(&values[0].to_le_bytes());
    if object_format == 1 {
        data.extend_from_slice(&(-2i16).to_le_bytes());
        data.extend_from_slice(&0xfeedu16.to_le_bytes());
    }
    data.extend_from_slice(&1u16.to_le_bytes());
    data.push(b'B');
    data.extend_from_slice(&[11, 0]); // object array, flags
    data.extend_from_slice(&((values.len() - 1) as u32).to_le_bytes());
    for value in &values[1..] {
        if object_format == 2 {
            data.extend_from_slice(&[0; 4]);
            data.extend_from_slice(&value.to_le_bytes());
        } else {
            data.extend_from_slice(&value.to_le_bytes());
            data.extend_from_slice(&[0; 4]);
        }
    }
    // A fragment section follows the script properties and must be preserved.
    data.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    data
}

fn mapping() -> HashMap<u32, u32> {
    HashMap::from([(0x800, 0x900), (0x801, 0x901), (0x802, 0x902)])
}

#[test]
fn fmt_15_remaps_record_typed_fields_groups_and_overrides_after_reload() {
    let header = FileHeader {
        overridden_forms: vec![0x801],
        ..FileHeader::default()
    };
    let mut plugin = Plugin::new(header);
    let record = Record::new(b"REFR", 0x800)
        .sub(b"NAME", 0x801u32.to_le_bytes())
        .sub(b"XOWN", 0x802u32.to_le_bytes());
    let idx = plugin.push_top(record);
    plugin.push_top(
        Record::new(b"MGEF", 0x802)
            .sub(
                b"KWDA",
                [0x801u32.to_le_bytes(), 0x802u32.to_le_bytes()].concat(),
            )
            .sub(b"ESCE", 0x801u32.to_le_bytes()),
    );
    plugin.groups[0].entries.clear();
    let mut world = Group::new(GroupKind::WorldChildren(0x800));
    let mut cell = Group::new(GroupKind::CellChildren(0x801));
    cell.entries.push(Entry::Record(idx));
    world.entries.push(Entry::Group(cell));
    plugin.groups.push(world);
    plugin
        .header_record
        .subrecords
        .push(soe_esm::Subrecord::new(b"ONAM", 0x801u32.to_le_bytes()));

    plugin.remap_form_ids(&mapping()).unwrap();
    let reloaded = Plugin::parse(&plugin.to_bytes()).unwrap();
    let record = reloaded
        .records
        .iter()
        .find(|record| record.tag == Tag::new(b"REFR"))
        .unwrap();
    assert_eq!(record.form_id, 0x900);
    assert_eq!(
        u32::from_le_bytes(record.find(Tag::new(b"NAME")).unwrap().try_into().unwrap()),
        0x901
    );
    assert_eq!(
        u32::from_le_bytes(record.find(Tag::new(b"XOWN")).unwrap().try_into().unwrap()),
        0x902
    );
    let world = reloaded
        .groups
        .iter()
        .find(|group| matches!(group.kind, GroupKind::WorldChildren(_)))
        .unwrap();
    assert!(matches!(world.kind, GroupKind::WorldChildren(0x900)));
    assert!(
        matches!(world.entries[0], Entry::Group(ref child) if child.kind == GroupKind::CellChildren(0x901))
    );
    assert_eq!(reloaded.header.overridden_forms, [0x901]);
    let magic = reloaded
        .records
        .iter()
        .find(|record| record.tag == Tag::new(b"MGEF"))
        .unwrap();
    assert_eq!(magic.form_id, 0x902);
    assert_eq!(
        magic.find(Tag::new(b"KWDA")).unwrap(),
        &[1, 9, 0, 0, 2, 9, 0, 0]
    );
    assert_eq!(
        magic.find(Tag::new(b"ESCE")).unwrap(),
        &0x901u32.to_le_bytes()
    );
}

#[test]
fn fmt_15_remaps_typed_vmad_objects_and_arrays_in_both_formats() {
    for object_format in [1, 2] {
        let original = vmad(object_format, &[0x801, 0x802]);
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(b"QUST", 0x800).sub(b"VMAD", original));
        plugin.remap_form_ids(&mapping()).unwrap();
        let reloaded = Plugin::parse(&plugin.to_bytes()).unwrap();
        let data = reloaded.records[0].find(Tag::new(b"VMAD")).unwrap();
        let scalar_link = if object_format == 2 { 21 } else { 17 };
        let array_link = if object_format == 2 { 38 } else { 34 };
        assert_eq!(
            u32::from_le_bytes(data[scalar_link..scalar_link + 4].try_into().unwrap()),
            0x901
        );
        assert_eq!(
            u32::from_le_bytes(data[array_link..array_link + 4].try_into().unwrap()),
            0x902
        );
        assert_eq!(&data[data.len() - 4..], &[0xde, 0xad, 0xbe, 0xef]);
    }
}

#[test]
fn fmt_15_rejects_malformed_vmad_without_mutating_plugin() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"QUST", 0x800).sub(b"VMAD", [5, 0, 2, 0, 1]));
    let before = plugin.clone();
    assert!(plugin.remap_form_ids(&mapping()).is_err());
    assert_eq!(plugin, before);
}
