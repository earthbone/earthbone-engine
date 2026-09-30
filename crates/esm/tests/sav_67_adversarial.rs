// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used)]

use soe_esm::{tag, FileHeader, LoadOrder, Plugin, Record};
use std::io::Write;

fn source_plugin(master: &str) -> Plugin {
    let mut plugin = Plugin::new(FileHeader {
        masters: vec![master.to_owned()],
        ..Default::default()
    });
    plugin.push_top(Record::new(tag::STAT, 0x800).zsub(tag::EDID, "StableDefinition"));
    plugin
}

fn compressed_plugin() -> Plugin {
    let mut bytes = source_plugin("Master.esm").to_bytes();
    let group_start = bytes.windows(4).position(|tag| tag == b"GRUP").unwrap();
    let record_start = group_start + 24;
    assert_eq!(&bytes[record_start..record_start + 4], b"STAT");
    let old_size = u32::from_le_bytes(
        bytes[record_start + 4..record_start + 8]
            .try_into()
            .unwrap(),
    ) as usize;
    let body_start = record_start + 24;
    let body_end = body_start + old_size;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&bytes[body_start..body_end]).unwrap();
    let compressed = encoder.finish().unwrap();
    let mut body = (old_size as u32).to_le_bytes().to_vec();
    body.extend(compressed);
    bytes.splice(body_start..body_end, body.iter().copied());
    let new_size = body.len() as u32;
    bytes[record_start + 4..record_start + 8].copy_from_slice(&new_size.to_le_bytes());
    let flags = soe_esm::plugin::flags::COMPRESSED.to_le_bytes();
    bytes[record_start + 8..record_start + 12].copy_from_slice(&flags);
    let group_size =
        u32::from_le_bytes(bytes[group_start + 4..group_start + 8].try_into().unwrap());
    let adjusted_group_size = group_size as usize + new_size as usize - old_size;
    bytes[group_start + 4..group_start + 8]
        .copy_from_slice(&(adjusted_group_size as u32).to_le_bytes());
    Plugin::parse(&bytes).unwrap()
}

#[test]
fn sav_67_compression_normalization_keeps_same_loaded_definition_fingerprint() {
    let plain = Plugin::parse(&source_plugin("Master.esm").to_bytes()).unwrap();
    let compressed = compressed_plugin();
    let mut first = LoadOrder::new();
    let mut second = LoadOrder::new();
    first
        .add("Master.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    second
        .add("Master.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    first.add("Child.esp", plain).unwrap();
    second.add("Child.esp", compressed).unwrap();
    assert_eq!(first.file(1).fingerprint, second.file(1).fingerprint);
}
