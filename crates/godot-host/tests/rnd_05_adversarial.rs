// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_bsa::Vfs;
use soe_godot_host::actor::ActorAssets;

const BODY: &str = "meshes/actors/character/character assets/malebody_1.nif";
const ARCHIVES: [&str; 12] = [
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
];

fn assets() -> Option<ActorAssets> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    let mut vfs = Vfs::new();
    for archive in ARCHIVES {
        let path = data.join(archive);
        if !path.exists() {
            return None;
        }
        vfs.add_archive_path(&path).expect("archive indexes");
    }
    Some(ActorAssets::load(&vfs, BODY).expect("vanilla body loads"))
}

#[test]
fn rnd_05_repeated_actor_loads_keep_each_authored_texture() {
    let Some(first) = assets() else {
        eprintln!("SKYRIM_DATA or a vanilla archive is unavailable, skipping");
        return;
    };
    let Some(second) = assets() else {
        panic!("the same installed archives remain available");
    };
    let material_signature = |assets: &ActorAssets| {
        assets
            .surfaces
            .iter()
            .map(|surface| {
                let path = surface
                    .arrays
                    .material
                    .diffuse
                    .as_deref()
                    .expect("each vanilla body shape names an authored diffuse texture");
                let texture = surface
                    .textures
                    .diffuse
                    .as_ref()
                    .expect("each authored body texture resolves");
                assert!(texture.width() > 1 && texture.height() > 1);
                (path.to_lowercase(), texture.clone())
            })
            .collect::<Vec<_>>()
    };

    let first = material_signature(&first);
    let second = material_signature(&second);
    assert_eq!(first, second, "texture resolution is stable across actor loads");
}
