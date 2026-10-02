// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for the rendered vanilla character's authored materials.

use std::collections::BTreeMap;
use std::path::PathBuf;

use soe_bsa::Vfs;
use soe_dds::Dds;
use soe_godot_host::actor::ActorAssets;
use soe_godot_host::data_source::TextureData;

const BODY: &str = "meshes/actors/character/character assets/malebody_1.nif";

fn installed_assets() -> Option<(Vfs, ActorAssets)> {
    let data_dir = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
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
            eprintln!("missing installed archive {}, skipping", path.display());
            return None;
        }
        vfs.add_archive_path(&path).expect("archive indexes");
    }
    let assets = ActorAssets::load(&vfs, BODY).expect("vanilla body loads");
    Some((vfs, assets))
}

#[test]
fn rnd_05_each_authored_body_diffuse_has_its_own_colored_image_data() {
    let Some((vfs, assets)) = installed_assets() else {
        eprintln!("SKYRIM_DATA or required archives unavailable, skipping");
        return;
    };

    let mut paths = BTreeMap::new();
    for surface in &assets.surfaces {
        let Some(path) = surface.arrays.material.diffuse.as_deref() else {
            continue;
        };
        let image = surface
            .textures
            .diffuse
            .as_ref()
            .expect("authored diffuse resolves through the VFS");
        assert!(image.width() > 0 && image.height() > 0);
        paths.insert(path.to_ascii_lowercase(), image);
    }
    assert!(paths.len() >= 2, "vanilla body uses multiple diffuse materials");

    for (path, upload) in paths {
        let bytes = vfs.read(&path).expect("the material's own texture path resolves");
        let dds = Dds::parse(&bytes).expect("resolved texture parses as DDS");
        let decoded = dds.decode(0, 0).expect("authored diffuse mip decodes");
        assert_eq!(upload.width(), decoded.width, "texture dimensions for {path}");
        assert_eq!(upload.height(), decoded.height, "texture dimensions for {path}");
        assert!(decoded.rgba.chunks_exact(4).any(|pixel| pixel[3] != 0));
        let colors: std::collections::BTreeSet<_> = decoded
            .rgba
            .chunks_exact(4)
            .map(|pixel| (pixel[0] / 16, pixel[1] / 16, pixel[2] / 16))
            .collect();
        assert!(colors.len() > 4, "{path} must retain authored color variation");

        match upload.as_ref() {
            TextureData::Compressed { bytes, mip0_len, .. } => {
                assert!(*mip0_len > 0);
                assert!(bytes.len() >= *mip0_len);
                assert_eq!(
                    &bytes[..*mip0_len],
                    dds.level_bytes(0, 0).expect("authored base mip"),
                    "compressed upload must use this surface's diffuse path: {path}"
                );
            }
            TextureData::Rgba8(image) => {
                assert_eq!(image.rgba, decoded.rgba, "RGBA upload bytes for {path}");
            }
            TextureData::Cubemap(_) => panic!("body diffuse cannot be a cubemap: {path}"),
        }
    }
}
