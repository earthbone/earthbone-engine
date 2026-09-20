// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Authored controller shapes, read in place from the installed archives.

use std::path::PathBuf;

use soe_physics::{collect, Layer};

#[test]
fn cmb_29_installed_skeleton_controllers_decode_without_partial_shapes() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let mut files = 0;
    let mut authored = 0;
    let mut controllers = 0;
    let mut giant = false;
    let mut dragon = false;
    for name in ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"] {
        let archive = soe_bsa::Archive::open(&data.join(name)).unwrap();
        for path in archive.files().iter().filter(|path| {
            path.contains("actors/")
                && path.contains("skeleton")
                && std::path::Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("nif"))
        }) {
            let file = soe_nif::parse(&archive.read(path).unwrap()).unwrap();
            let collected = collect(&file).unwrap_or_else(|error| panic!("{path}: {error}"));
            files += 1;
            authored += usize::from(!collected.phantoms.is_empty());
            controllers += collected.phantoms.len();
            assert!(collected
                .phantoms
                .iter()
                .all(|p| p.layer == Layer::CHAR_CONTROLLER));
            if path == "meshes/actors/giant/character assets/skeleton.nif" {
                giant = !collected.phantoms.is_empty();
            }
            if path == "meshes/actors/dragon/character assets/skeleton.nif" {
                dragon = !collected.phantoms.is_empty();
            }
        }
    }
    assert!(
        giant && dragon,
        "known authored controller skeletons were not found"
    );
    eprintln!(
        "CMB29: {files} skeletons, {authored} with controller geometry, {controllers} phantoms"
    );
}
