// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use soe_esm::{FileHeader, Plugin, Record, Tag};

#[test]
fn fmt_15_adversarial_every_vmad_object_array_link_is_remapped() {
    for format in [1u16, 2] {
        let mut vmad = Vec::new();
        vmad.extend_from_slice(&5u16.to_le_bytes());
        vmad.extend_from_slice(&format.to_le_bytes());
        vmad.extend_from_slice(&1u16.to_le_bytes());
        vmad.extend_from_slice(&1u16.to_le_bytes());
        vmad.push(b'S');
        vmad.push(0);
        vmad.extend_from_slice(&1u16.to_le_bytes());
        vmad.extend_from_slice(&1u16.to_le_bytes());
        vmad.push(b'O');
        vmad.extend_from_slice(&[11, 0]);
        vmad.extend_from_slice(&3u32.to_le_bytes());
        let mut locations = Vec::new();
        for id in [0x801u32, 0x802, 0x803] {
            if format == 2 {
                vmad.extend_from_slice(&0xfeed_beefu32.to_le_bytes());
                locations.push(vmad.len());
                vmad.extend_from_slice(&id.to_le_bytes());
            } else {
                locations.push(vmad.len());
                vmad.extend_from_slice(&id.to_le_bytes());
                vmad.extend_from_slice(&0xfeed_beefu32.to_le_bytes());
            }
        }
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(b"QUST", 0x800).sub(b"VMAD", vmad));
        let mapping = HashMap::from([(0x801, 0x901), (0x802, 0x902), (0x803, 0x903)]);
        plugin.remap_form_ids(&mapping).unwrap();
        let bytes = plugin.records[0].find(Tag::new(b"VMAD")).unwrap();
        for (at, expected) in locations.into_iter().zip([0x901u32, 0x902, 0x903]) {
            assert_eq!(
                u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()),
                expected
            );
        }
    }
}

#[test]
fn fmt_15_adversarial_malformed_typed_data_does_not_partially_remap() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"REFR", 0x800).sub(b"NAME", 0x801u32.to_le_bytes()));
    plugin.push_top(Record::new(b"QUST", 0x802).sub(b"VMAD", [5, 0, 9, 0]));
    let before = plugin.clone();
    assert!(plugin
        .remap_form_ids(&HashMap::from([(0x800, 0x900), (0x801, 0x901)]))
        .is_err());
    assert_eq!(plugin, before);
}

#[test]
fn fmt_15_adversarial_qust_alias_object_formid_is_remapped() {
    let mut vmad = Vec::new();
    vmad.extend_from_slice(&5u16.to_le_bytes()); // VMAD version
    vmad.extend_from_slice(&2u16.to_le_bytes()); // object format
    vmad.extend_from_slice(&0u16.to_le_bytes()); // primary scripts
    vmad.push(2); // QUST fragment section version
    vmad.extend_from_slice(&0u16.to_le_bytes()); // fragment count
    vmad.extend_from_slice(&0u16.to_le_bytes()); // fragment file name
    vmad.extend_from_slice(&1u16.to_le_bytes()); // aliases
    vmad.extend_from_slice(&[0, 0]); // alias object reserved bytes
    vmad.extend_from_slice(&0i16.to_le_bytes()); // alias id
    vmad.extend_from_slice(&0x801u32.to_le_bytes()); // alias object FormID
    vmad.extend_from_slice(&5u16.to_le_bytes()); // alias VMAD version
    vmad.extend_from_slice(&2u16.to_le_bytes()); // alias object format
    vmad.extend_from_slice(&0u16.to_le_bytes()); // alias scripts

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"QUST", 0x800).sub(b"VMAD", vmad));
    plugin
        .remap_form_ids(&HashMap::from([(0x801, 0x901)]))
        .unwrap();
    let bytes = plugin.records[0].find(Tag::new(b"VMAD")).unwrap();
    assert_eq!(u32::from_le_bytes(bytes[17..21].try_into().unwrap()), 0x901);
}

#[test]
fn fmt_15_refr_struct_links_remap_without_changing_scalar_words() {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut teleport = Vec::from(0x801u32.to_le_bytes());
    teleport.extend_from_slice(&[0x55; 24]);
    let mut lock = vec![42, 0, 0, 0];
    lock.extend_from_slice(&0x801u32.to_le_bytes());
    lock.push(4);
    let mut linked = Vec::from(0x801u32.to_le_bytes());
    linked.extend_from_slice(&0x802u32.to_le_bytes());
    let mut navmesh = Vec::from(0x801u32.to_le_bytes());
    navmesh.extend_from_slice(&[0x22; 4]);
    plugin.push_top(
        Record::new(b"REFR", 0x800)
            .sub(
                b"XESP",
                [0x801u32.to_le_bytes(), 0x801u32.to_le_bytes()].concat(),
            )
            .sub(b"XTEL", teleport)
            .sub(b"XLOC", lock)
            .sub(b"XLKR", linked)
            .sub(b"XLKR", 0x802u32.to_le_bytes())
            .sub(b"XNDP", navmesh)
            .sub(b"XLRL", 0x802u32.to_le_bytes()),
    );
    plugin
        .remap_form_ids(&HashMap::from([(0x801, 0x901), (0x802, 0x902)]))
        .unwrap();
    let record = &plugin.records[0];
    let read = |data: &[u8], offset: usize| {
        u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
    };
    assert_eq!(read(record.find(Tag::new(b"XESP")).unwrap(), 0), 0x901);
    assert_eq!(read(record.find(Tag::new(b"XESP")).unwrap(), 4), 0x801);
    assert_eq!(read(record.find(Tag::new(b"XTEL")).unwrap(), 0), 0x901);
    assert_eq!(read(record.find(Tag::new(b"XLOC")).unwrap(), 4), 0x901);
    let links: Vec<_> = record.all(Tag::new(b"XLKR")).collect();
    assert_eq!(read(links[0], 0), 0x901);
    assert_eq!(read(links[0], 4), 0x902);
    assert_eq!(read(links[1], 0), 0x902);
    assert_eq!(read(record.find(Tag::new(b"XNDP")).unwrap(), 0), 0x901);
    assert_eq!(
        read(record.find(Tag::new(b"XNDP")).unwrap(), 4),
        0x2222_2222
    );
    assert_eq!(read(record.find(Tag::new(b"XLRL")).unwrap(), 0), 0x902);
}
