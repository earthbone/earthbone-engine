// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_bsa::Vfs;
use soe_godot_host::actor::ActorAssets;

const BODY: &str = "meshes/actors/character/character assets/malebody_1.nif";

#[test]
fn rnd_05_authored_body_materials_remain_attached_to_skinned_shapes() {
    let Some(data_dir) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let mut vfs = Vfs::new();
    for archive in [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Animations.bsa",
        "Skyrim - Textures0.bsa",
        "Skyrim - Textures1.bsa",
        "Skyrim - Textures2.bsa",
        "Skyrim - Textures3.bsa",
        "Skyrim - Textures4.bsa",
        "Skyrim - Textures5.bsa",
        "Skyrim - Textures6.bsa",
        "Skyrim - Textures7.bsa",
        "Skyrim - Textures8.bsa",
    ] {
        let path = data_dir.join(archive);
        if !path.exists() {
            eprintln!("{} missing, skipping", path.display());
            return;
        }
        vfs.add_archive_path(&path).expect("archive indexes");
    }

    let assets = ActorAssets::load(&vfs, BODY).expect("vanilla body loads");
    assert!(!assets.surfaces.is_empty(), "body has drawable shapes");
    assert!(
        assets.surfaces.iter().all(|surface| {
            surface.arrays.is_skinned()
                && surface.arrays.material.diffuse.is_some()
                && surface.textures.diffuse.is_some()
        }),
        "each skinned body shape keeps its authored diffuse texture"
    );
}
