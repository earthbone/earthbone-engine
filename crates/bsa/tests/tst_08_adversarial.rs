// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::io::Write;
use std::path::Path;

use soe_bsa::{Archive, Error};

fn ba2_dx10(chunks: &[(&[u8], bool)], width: u16, height: u16, mips: u8) -> Vec<u8> {
    let name = b"textures/edge.dds";
    let name_at = 24 + 24 + chunks.len() * 24;
    let data_at = name_at + 2 + name.len();
    let mut stored = Vec::new();
    let mut headers = Vec::new();
    let mut offset = data_at;
    for (bytes, compressed) in chunks {
        let payload = if *compressed {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(bytes).unwrap();
            encoder.finish().unwrap()
        } else {
            bytes.to_vec()
        };
        headers.extend_from_slice(&(offset as u64).to_le_bytes());
        headers
            .extend_from_slice(&(if *compressed { payload.len() as u32 } else { 0 }).to_le_bytes());
        headers.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        headers.extend_from_slice(&[0; 4]);
        headers.extend_from_slice(&0xBAAD_F00Du32.to_le_bytes());
        offset += payload.len();
        stored.extend(payload);
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"BTDX");
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(b"DX10");
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(name_at as u64).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(b"dds\0");
    out.extend_from_slice(&0u32.to_le_bytes());
    out.push(0);
    out.push(chunks.len() as u8);
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&width.to_le_bytes());
    out.push(mips);
    out.push(98);
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend(headers);
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(name);
    out.extend(stored);
    out
}

#[test]
fn dx10_mixed_raw_and_zlib_chunks_rebuild_in_chunk_order() {
    let top = [0x11; 16];
    let mip = [0x22; 4];
    let bytes = ba2_dx10(&[(&top, false), (&mip, true)], 2, 2, 2);
    let archive = Archive::from_bytes(Path::new("multi.ba2"), bytes).unwrap();
    let dds = archive.read("textures/edge.dds").unwrap();
    assert_eq!(u32::from_le_bytes(dds[12..16].try_into().unwrap()), 2);
    assert_eq!(u32::from_le_bytes(dds[16..20].try_into().unwrap()), 2);
    assert_eq!(u32::from_le_bytes(dds[28..32].try_into().unwrap()), 2);
    assert_eq!(&dds[148..], &[top.as_slice(), mip.as_slice()].concat());
}

#[test]
fn dx10_declared_unpacked_chunk_length_must_match_inflated_bytes() {
    let payload = [0x5A; 16];
    let mut bytes = ba2_dx10(&[(&payload, true)], 2, 2, 1);
    // The single chunk's unpacked-size field is record offset 24 + chunk offset 12.
    bytes[60..64].copy_from_slice(&17u32.to_le_bytes());
    let archive = Archive::from_bytes(Path::new("bad-size.ba2"), bytes).unwrap();
    assert!(matches!(
        archive.read("textures/edge.dds"),
        Err(Error::DecompressedSize { .. })
    ));
}

#[test]
fn truncated_dx10_payload_fails_for_memory_and_disk_archives() {
    let payload = [0xA5; 16];
    let mut bytes = ba2_dx10(&[(&payload, false)], 2, 2, 1);
    bytes.pop();
    let archive = Archive::from_bytes(Path::new("short.ba2"), bytes.clone()).unwrap();
    assert!(matches!(
        archive.read("textures/edge.dds"),
        Err(Error::Truncated { .. })
    ));
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("short.ba2");
    std::fs::write(&path, bytes).unwrap();
    let archive = Archive::open(&path).unwrap();
    assert!(matches!(
        archive.read("textures/edge.dds"),
        Err(Error::Truncated { .. })
    ));
}
