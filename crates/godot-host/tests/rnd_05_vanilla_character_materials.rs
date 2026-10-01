// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Vanilla character materials resolve through the archives used by the host.
//!
//! Gated on `SKYRIM_DATA`, skipping cleanly when it is unset.

use std::path::PathBuf;

use soe_bsa::Vfs;
use soe_godot_host::actor::ActorAssets;

const BODY: &str = "meshes/actors/character/character assets/malebody_1.nif";

#[test]
fn rnd_05_vanilla_skinned_body_resolves_its_authored_diffuse_textures() {
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
            eprintln!("skipping: {} is not there", path.display());
            return;
        }
        vfs.add_archive_path(&path).expect("archive indexes");
    }

    let assets = ActorAssets::load(&vfs, BODY).expect("the vanilla body loads");
    let diffuse: Vec<_> = assets
        .surfaces
        .iter()
        .filter_map(|surface| {
            surface
                .arrays
                .material
                .diffuse
                .as_deref()
                .map(|path| (path, surface.textures.diffuse.as_ref()))
        })
        .collect();

    assert!(!diffuse.is_empty(), "the body has authored diffuse paths");
    assert!(
        diffuse.iter().all(|(_, texture)| texture.is_some()),
        "every named diffuse texture resolves from the installed archives"
    );
    assert!(
        diffuse.iter().all(
            |(_, texture)| texture.is_some_and(|image| image.width() > 0 && image.height() > 0)
        ),
        "resolved diffuse textures carry uploadable image dimensions"
    );
}
