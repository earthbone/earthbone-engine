// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used)]

//! Installed human HKX data must supply the bodies and bone links the host
//! sends to Jolt. The archive is read only when SKYRIM_DATA is set.

use std::path::PathBuf;

use soe_bsa::Vfs;
use soe_godot_host::ragdoll::RagdollBoneMap;
use soe_hkx::ragdoll::Ragdoll;
use soe_hkx::skeleton::AnimationContainer;
use soe_hkx::Packfile;

#[test]
fn cmb_19_vanilla_human_death_has_renderable_bodies_and_joints() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        return;
    };
    let mut vfs = Vfs::new();
    vfs.add_archive_path(&data.join("Skyrim - Animations.bsa"))
        .expect("vanilla animation archive");
    let bytes = vfs
        .read("meshes/actors/character/character assets/skeleton.hkx")
        .expect("vanilla human skeleton");
    let packfile = Packfile::parse(&bytes).expect("valid HKX");
    let rig = AnimationContainer::from_packfile(&packfile)
        .expect("animation container")
        .skeletons
        .remove(0);
    let ragdoll = Ragdoll::from_packfile(&packfile).expect("human ragdoll");
    let map = RagdollBoneMap::new(&rig, &ragdoll);
    assert_eq!(ragdoll.bodies.len(), 18);
    assert_eq!(ragdoll.constraints.len(), 17);
    assert!(
        map.body_to_animated
            .iter()
            .filter(|entry| entry.is_some())
            .count()
            >= 16
    );
    assert!(
        map.animated_to_body
            .iter()
            .filter(|entry| entry.is_some())
            .count()
            >= 70
    );
}
