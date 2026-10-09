// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-29 cause classes must describe the work that actually ran.

use soe_bsa::{file_name_hash, folder_hash, Archive};
use soe_trace::{LoadCause, LoadScope, LoadTrace};
use std::path::Path;
use std::time::Duration;

fn raw_archive() -> Vec<u8> {
    let folder = b"meshes";
    let name = b"raw.nif";
    let data_start = 36 + 24 + 1 + folder.len() + 1 + 16 + name.len() + 1;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"BSA\0");
    bytes.extend_from_slice(&105u32.to_le_bytes());
    bytes.extend_from_slice(&36u32.to_le_bytes());
    bytes.extend_from_slice(&0x03u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&7u32.to_le_bytes());
    bytes.extend_from_slice(&8u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&folder_hash("meshes").to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.push((folder.len() + 1) as u8);
    bytes.extend_from_slice(folder);
    bytes.push(0);
    bytes.extend_from_slice(&file_name_hash("raw.nif").to_le_bytes());
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&(data_start as u32).to_le_bytes());
    bytes.extend_from_slice(name);
    bytes.push(0);
    bytes.extend_from_slice(b"nif");
    bytes
}

#[test]
fn prf_29_uncompressed_archive_record_is_not_reported_as_decode() {
    let load = LoadScope::operation("raw_archive_fixture", "fixture.bsa", Duration::from_secs(1));
    let root = load.id();
    assert_eq!(
        Archive::from_bytes(Path::new("fixture.bsa"), raw_archive())
            .unwrap()
            .read("meshes/raw.nif")
            .unwrap(),
        b"nif"
    );
    drop(load);

    let events = LoadTrace::events();
    assert!(!events
        .iter()
        .any(|event| { event.root == root && event.cause == Some(LoadCause::Decode) }));
}
