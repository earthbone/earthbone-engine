// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use soe_bsa::Vfs;
use soe_godot_host::actor::ActorAssets;

#[test]
fn anm_59_vanilla_head_mesh_loads_for_expression_target_mapping() {
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
    assert_eq!(
        soe_godot_host::facial::expression_name(10),
        Some("MoodHappy")
    );
}
