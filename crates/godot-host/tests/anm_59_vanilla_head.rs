// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use soe_bsa::Vfs;
use soe_godot_host::actor::ActorAssets;

#[test]
fn anm_59_vanilla_head_mesh_loads_expression_blend_shapes() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping vanilla head test");
        return;
    };
    let mut vfs = Vfs::new();
    for archive in [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Animations.bsa",
    ] {
        let path = data.join(archive);
        if !path.exists() {
            eprintln!("{} is absent, skipping vanilla head test", path.display());
            return;
        }
        vfs.add_archive_path(Path::new(&path)).unwrap();
    }
    let assets = ActorAssets::load(
        &vfs,
        "meshes/actors/character/character assets/femalehead.nif",
    )
    .unwrap();
    assert!(
        !assets.surfaces.is_empty(),
        "vanilla head has no skinned surfaces"
    );
    let shapes: Vec<_> = assets
        .surfaces
        .iter()
        .flat_map(|surface| surface.expression_shapes.iter())
        .map(|shape| shape.name.as_str())
        .collect();
    assert!(
        shapes.contains(&"MoodHappy"),
        "vanilla head TRI does not supply the MoodHappy expression target"
    );
    assert!(
        assets
            .surfaces
            .iter()
            .flat_map(|surface| &surface.expression_shapes)
            .any(|shape| shape.positions.iter().any(|delta| delta != &[0.0; 3])),
        "vanilla expression target carries no vertex deltas"
    );
}
