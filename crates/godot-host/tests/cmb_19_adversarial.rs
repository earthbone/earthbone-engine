// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used)]

//! CMB-19 installed HKX consistency checks.
//!
//! `hkaRagdollInstance` owns its rigid bodies, bone map and constraint body
//! references. The local serde-hkx reader validates indices while decoding.

use std::path::PathBuf;

use soe_bsa::Vfs;
use soe_godot_host::ragdoll::RagdollBoneMap;
use soe_hkx::ragdoll::{ConstraintKind, Ragdoll};
use soe_hkx::skeleton::AnimationContainer;
use soe_hkx::Packfile;

fn human_ragdoll() -> Option<(soe_hkx::skeleton::Skeleton, Ragdoll)> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    let mut vfs = Vfs::new();
    vfs.add_archive_path(&data.join("Skyrim - Animations.bsa"))
        .expect("vanilla animation archive");
    let bytes = vfs
        .read("meshes/actors/character/character assets/skeleton.hkx")
        .expect("vanilla human skeleton");
    let packfile = Packfile::parse(&bytes).expect("valid HKX");
    let animated = AnimationContainer::from_packfile(&packfile)
        .expect("animation container")
        .skeletons
        .into_iter()
        .next()
        .expect("animated skeleton");
    let ragdoll = Ragdoll::from_packfile(&packfile).expect("human ragdoll");
    Some((animated, ragdoll))
}

#[test]
fn cmb_19_installed_joint_endpoints_and_body_properties_are_usable() {
    let Some((_animated, ragdoll)) = human_ragdoll() else {
        return;
    };
    assert_eq!(ragdoll.bodies.len(), 18);
    assert_eq!(ragdoll.constraints.len(), 17);
    for body in &ragdoll.bodies {
        assert!(body.motion.inertia_and_mass_inverse[3].is_finite());
        assert!(body.material.friction.is_finite());
        assert!(body.material.restitution.is_finite());
    }
    for constraint in &ragdoll.constraints {
        for endpoint in constraint.bodies.into_iter().flatten() {
            assert!(endpoint.0 < ragdoll.bodies.len());
        }
        match &constraint.kind {
            ConstraintKind::Ragdoll(joint) => {
                assert!(joint.frames.iter().flat_map(|f| f.rotation).flatten().all(f32::is_finite));
                assert!(joint.frames.iter().flat_map(|f| f.translation).all(f32::is_finite));
            }
            ConstraintKind::LimitedHinge(joint) => {
                assert!(joint.frames.iter().flat_map(|f| f.rotation).flatten().all(f32::is_finite));
                assert!(joint.frames.iter().flat_map(|f| f.translation).all(f32::is_finite));
            }
        }
    }
}

#[test]
fn cmb_19_every_authored_body_maps_to_a_rendered_animated_bone() {
    let Some((animated, ragdoll)) = human_ragdoll() else {
        return;
    };
    let map = RagdollBoneMap::new(&animated, &ragdoll);
    assert_eq!(map.body_to_animated.len(), ragdoll.bodies.len());
    assert!(
        map.body_to_animated.iter().all(Option::is_some),
        "every vanilla ragdoll body needs an animated bone for initial pose placement"
    );
    assert!(map.animated_to_body.iter().any(Option::is_none));
}
