// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use soe_bsa::{file_name_hash, folder_hash, index_archive_in};

fn fixture(folder: &str, name: &str, folder_hash: u64, file_hash: u64) -> Vec<u8> {
    let folder = folder.as_bytes();
    let name = name.as_bytes();
    let data_offset = 36 + 24 + 1 + folder.len() + 16 + name.len();
    let mut data = Vec::new();
    data.extend_from_slice(b"BSA\0");
    data.extend_from_slice(&105u32.to_le_bytes());
    data.extend_from_slice(&36u32.to_le_bytes());
    data.extend_from_slice(&0x03u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&((folder.len() + name.len()) as u32).to_le_bytes());
    data.extend_from_slice(&(name.len() as u32).to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&folder_hash.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    data.push(folder.len() as u8 + 1);
    data.extend_from_slice(folder);
    data.push(0);
    data.extend_from_slice(&file_hash.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&(data_offset as u32).to_le_bytes());
    data.extend_from_slice(name);
    data.push(0);
    data.push(b'x');
    data
}

#[test]
fn fmt_14_independent_name_hash_fixture_is_accepted() {
    let folder = "textures/armor/amuletsandrings/elder council";
    let name = "elder_council_amulet_n.dds";
    let data = fixture(
        folder,
        name,
        folder_hash(folder),
        file_name_hash(name),
    );
    let archive = index_archive_in(&data, Path::new("correct-hash.bsa"));
    assert!(archive.is_ok(), "correct BSA hashes must be accepted: {archive:?}");
}

#[test]
fn fmt_14_wrong_file_hash_reports_the_path_and_mismatch() {
    let correct = file_name_hash("a.nif");
    let data = fixture("meshes", "a.nif", folder_hash("meshes"), correct ^ 1);
    let error = index_archive_in(&data, Path::new("wrong-file.bsa")).unwrap_err();
    assert!(error.to_string().contains("BSA file hash mismatch"));
    assert!(error.to_string().contains("meshes/a.nif"));
}

#[test]
fn fmt_14_wrong_directory_hash_is_checked_too() {
    let data = fixture(
        "meshes",
        "a.nif",
        folder_hash("meshes") ^ 1,
        file_name_hash("a.nif"),
    );
    let error = index_archive_in(&data, Path::new("wrong-folder.bsa")).unwrap_err();
    assert!(error.to_string().contains("BSA directory hash mismatch"));
    assert!(error.to_string().contains("meshes"));
}
