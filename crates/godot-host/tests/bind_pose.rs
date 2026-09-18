// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! What space is a skinned character's bind pose in, and does the composition
//! agree with it?
//!
//! Three files describe a vanilla character and agree only by name.
//! `malebody_1.nif` carries one bind transform per bone it names, which
//! `nifly` calls `transformSkinToBone`. `skeleton.hkx` carries a reference
//! pose for the same bones. `skeleton.nif` carries a node hierarchy that is a
//! third pose again. Skinning is bone world times skin to bone, so at the
//! reference pose every bone must land on one shared transform, the one
//! carrying skin space into model space.
//!
//! The rendered character came out with a correct torso and flattened arms,
//! zero error at the pelvis and growing outward. That is the signature of a
//! transposed bind rotation, because an identity rotation is its own
//! transpose and the pelvis has one. This pins the three facts that catch it:
//! the bones agree, they agree on the shape's own offset, and the character
//! that results stands at a human height.
//!
//! Gated on `SKYRIM_DATA`, skipping cleanly when it is unset.

use std::collections::HashMap;
use std::path::PathBuf;

use soe_bsa::Vfs;
use soe_godot_host::actor::ActorAssets;
use soe_godot_host::skinned::{skinning_transforms_into, Affine};
use soe_havok::pose::to_model_space_into;

const BODY: &str = "meshes/actors/character/character assets/malebody_1.nif";

fn character_vfs() -> Option<Vfs> {
    let dir = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    let mut vfs = Vfs::new();
    for name in [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Animations.bsa",
    ] {
        let path = dir.join(name);
        if !path.exists() {
            eprintln!("skipping: {} is not there", path.display());
            return None;
        }
        vfs.add_archive_path(&path).expect("archive indexes");
    }
    Some(vfs)
}

/// The largest absolute difference between two transforms, treating a metre of
/// origin as comparable to a unit of basis.
fn difference(a: &Affine, b: &Affine) -> f32 {
    let basis = (0..3)
        .flat_map(|r| (0..3).map(move |c| (r, c)))
        .map(|(r, c)| (a.basis[r][c] - b.basis[r][c]).abs())
        .fold(0.0f32, f32::max);
    let origin = (0..3)
        .map(|i| (a.origin[i] - b.origin[i]).abs())
        .fold(0.0f32, f32::max);
    basis.max(origin)
}

#[test]
fn the_bind_pose_puts_every_bone_on_one_shared_transform() {
    let Some(vfs) = character_vfs() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };

    let assets = ActorAssets::load(&vfs, BODY).expect("the character loads");
    let mut bind = Vec::new();
    to_model_space_into(&assets.skeleton, &assets.skeleton.reference_pose, &mut bind)
        .expect("the reference pose lifts");

    // Fact one: at the reference pose every bone of every shape agrees.
    let mut shared: Option<Affine> = None;
    let mut worst_spread = 0.0f32;
    let mut worst_bone = String::new();
    for surface in &assets.surfaces {
        let mut skins = Vec::new();
        skinning_transforms_into(&surface.map, &surface.skin, &bind, &mut skins)
            .expect("the bind pose skins");
        for (a, bone) in skins.iter().zip(&surface.skin.bones) {
            let first = *shared.get_or_insert(*a);
            let d = difference(a, &first);
            if d > worst_spread {
                worst_spread = d;
                worst_bone.clone_from(&bone.name);
            }
        }
    }
    let shared = shared.expect("the body has bones");
    println!("bones agree to {worst_spread:.6}, worst at {worst_bone}");
    println!("  shared transform origin {:?}", shared.origin);
    assert!(
        worst_spread < 1e-3,
        "every bone must give the same transform at the reference pose, {worst_bone} is {worst_spread} out"
    );

    // Fact two: what they agree on is the shape's own offset, inverted. That
    // is the skin to model step, and it is why the offset is not composed into
    // the skinning transform as well.
    let offset = Affine::from_nif(&assets.surfaces[0].skin.transform).to_godot();
    for i in 0..3 {
        assert!(
            (shared.origin[i] + offset.origin[i]).abs() < 1e-3,
            "the shared transform should invert the shape offset, {:?} against {:?}",
            shared.origin,
            offset.origin
        );
    }

    // Fact three: the character that results is human sized and stands on the
    // floor rather than buried under it. The body shape carries no head, hands
    // or feet, so its own span is shorter than a whole character.
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for surface in &assets.surfaces {
        for v in &surface.arrays.positions {
            lo = lo.min(v[1] + shared.origin[1]);
            hi = hi.max(v[1] + shared.origin[1]);
        }
    }
    println!("  skinned height range {lo:.3} m to {hi:.3} m");
    assert!(
        lo > -0.05 && lo < 0.4,
        "the lowest vertex should sit near the floor, it is at {lo} m"
    );
    assert!(
        hi > 1.2 && hi < 2.0,
        "the highest vertex should be near shoulder height, it is at {hi} m"
    );

    // And the reason the transpose is needed, stated as a measurement. The
    // rotation stored in the bind slot is the bone's own pose rotation, not
    // its inverse, so reading it straight and multiplying gives the rotation
    // twice over instead of cancelling. Checked on a bone whose rotation is
    // not the identity, since the pelvis would pass either way.
    let hkx: HashMap<&str, Affine> = assets
        .skeleton
        .bones
        .iter()
        .zip(&bind)
        .map(|(b, t)| (b.name.as_str(), Affine::from_qs(t)))
        .collect();
    let arm = assets
        .surfaces
        .iter()
        .flat_map(|s| s.skin.bones.iter())
        .find(|b| b.name.contains("UpperArm"))
        .expect("the body names an upper arm");
    let pose = hkx.get(arm.name.as_str()).expect("the skeleton has it");
    let stored = Affine::from_nif(&arm.inverse_bind);
    let mut turned = 0.0f32;
    let mut same = 0.0f32;
    for r in 0..3 {
        for c in 0..3 {
            same = same.max((stored.basis[r][c] - pose.basis[r][c]).abs());
            turned = turned.max((stored.basis[r][c] - pose.basis[c][r]).abs());
        }
    }
    println!(
        "  {}: bind against the pose {same:.6}, against its transpose {turned:.6}",
        arm.name
    );
    assert!(
        same < 1e-3,
        "the stored bind rotation should equal the pose rotation, out by {same}"
    );
    assert!(
        turned > 0.1,
        "this bone's rotation must not be symmetric or the check proves nothing"
    );
}
