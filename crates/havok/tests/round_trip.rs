// SPDX-License-Identifier: GPL-3.0-or-later

//! A synthetic `skeleton.hkx`-shaped file with one skeleton, one spline
//! animation and one binding, written with the builder and read back.

use soe_havok::animation::Animation;
use soe_havok::skeleton::{AnimationContainer, BlendHint};
use soe_havok::spline::encode;
use soe_havok::writer::PackfileBuilder;
use soe_havok::{Packfile, QsTransform};

fn pad4(v: &mut Vec<u8>) {
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
}

/// One transform track: x rides a linear spline 0, 10, 20 over three
/// frames, rotation is a static 90-degree turn about z in 48-bit form,
/// scale is identity.
fn block() -> Vec<u8> {
    let mut b = Vec::new();
    // pos quant 16-bit, rot quant index 4 (48-bit) static, scale identity.
    b.extend_from_slice(&[1 | (2 << 2), 0b0001_0000, 0b0000_0001, 0]);
    pad4(&mut b);
    b.extend_from_slice(&2u16.to_le_bytes());
    b.push(1);
    b.extend_from_slice(&[0, 0, 1, 2, 2]);
    pad4(&mut b);
    b.extend_from_slice(&0.0f32.to_le_bytes());
    b.extend_from_slice(&20.0f32.to_le_bytes());
    for v in [0.0f32, 10.0, 20.0] {
        b.extend_from_slice(&encode::point_16(0.0, 20.0, v).to_le_bytes());
    }
    pad4(&mut b);
    let s = std::f32::consts::FRAC_1_SQRT_2;
    b.extend_from_slice(&encode::quat_48([0.0, 0.0, s, s]));
    pad4(&mut b);
    b
}

fn build() -> Vec<u8> {
    let mut b = PackfileBuilder::new();
    let root = b.object("hkRootLevelContainer", 16);
    let container = b.object("hkaAnimationContainer", 96);
    let variant = b.array_structs(root, 0, 24, 1);
    b.string(variant, 0, "Merged Animation Container");
    b.string(variant, 8, "hkaAnimationContainer");
    b.ptr(variant, 16, container);

    let skeleton = b.object("hkaSkeleton", 120);
    b.string(skeleton, 16, "NPC Root [Root]");
    b.array_i16(skeleton, 24, &[-1, 0]);
    let bones = b.array_structs(skeleton, 40, 16, 2);
    b.string(bones, 0, "NPC Root [Root]");
    b.string(bones + 16, 0, "NPC Pelvis [Pelv]");
    b.set_bool(bones + 16, 8, true);
    let pelvis = QsTransform {
        translation: [0.0, 0.0, 60.0, 0.0],
        ..QsTransform::IDENTITY
    };
    b.array_qs_transform(skeleton, 56, &[QsTransform::IDENTITY, pelvis]);
    b.array_strings(skeleton, 88, &["Weapon_Left", "Weapon_Right"]);
    b.array_ptrs(container, 16, &[skeleton]);

    let anim = b.object("hkaSplineCompressedAnimation", 176);
    b.set_i32(anim, 16, 5);
    b.set_f32(anim, 20, 2.0 / 30.0);
    b.set_i32(anim, 24, 1);
    b.set_i32(anim, 28, 0);
    b.set_i32(anim, 56, 3);
    b.set_i32(anim, 60, 1);
    b.set_i32(anim, 64, 256);
    b.set_i32(anim, 68, 4);
    b.set_f32(anim, 72, 256.0 / 30.0);
    b.set_f32(anim, 76, 30.0 / 256.0);
    b.set_f32(anim, 80, 1.0 / 30.0);
    b.array_u32(anim, 88, &[0]);
    b.array_u32(anim, 104, &[0]);
    b.array_u32(anim, 120, &[4]);
    let data = block();
    b.array_bytes(anim, 152, &data, data.len());
    let tracks = b.array_structs(anim, 40, 24, 1);
    b.string(tracks, 0, "NPC Pelvis [Pelv]");
    let ann = b.array_structs(tracks, 8, 16, 1);
    b.set_f32(ann, 0, 0.05);
    b.string(ann, 8, "FootLeft");
    b.array_ptrs(container, 32, &[anim]);

    let binding = b.object("hkaAnimationBinding", 72);
    b.string(binding, 16, "NPC Root [Root]");
    b.ptr(binding, 24, anim);
    b.array_i16(binding, 32, &[1]);
    b.set_i8(binding, 64, 1);
    b.array_ptrs(container, 48, &[binding]);
    b.finish()
}

#[test]
fn a_skeleton_and_a_clip_survive_the_round_trip() {
    let bytes = build();
    let pf = Packfile::parse(&bytes).unwrap();
    let c = AnimationContainer::from_packfile(&pf).unwrap();

    assert_eq!(c.skeletons.len(), 1);
    let sk = &c.skeletons[0];
    assert_eq!(sk.name, "NPC Root [Root]");
    assert_eq!(sk.bones.len(), 2);
    assert_eq!(sk.bones[1].name, "NPC Pelvis [Pelv]");
    assert!(sk.bones[1].lock_translation);
    assert_eq!(sk.parent_indices, vec![-1, 0]);
    assert_eq!(sk.reference_pose[1].translation[2], 60.0);
    assert_eq!(sk.float_slots, vec!["Weapon_Left", "Weapon_Right"]);
    assert_eq!(sk.roots(), vec![0]);
    assert_eq!(sk.bone_index("NPC Pelvis [Pelv]"), Some(1));

    assert_eq!(c.animations.len(), 1);
    let Animation::SplineCompressed(a) = &c.animations[0] else {
        panic!("{:?}", c.animations[0]);
    };
    assert_eq!(a.num_frames, 3);
    assert_eq!(a.num_transform_tracks, 1);
    assert_eq!(a.annotation_tracks[0].track_name, "NPC Pelvis [Pelv]");
    assert_eq!(a.annotation_tracks[0].annotations[0].text, "FootLeft");
    let blocks = a.decode().unwrap();
    assert_eq!(blocks.len(), 1);
    let mid = a.sample(&blocks, 0, 1.0 / 30.0).unwrap();
    assert!((mid.translation[0] - 10.0).abs() < 0.01, "{mid:?}");
    let s = std::f32::consts::FRAC_1_SQRT_2;
    assert!((mid.rotation[2] - s).abs() < 2e-3);
    assert!((mid.rotation[3] - s).abs() < 2e-3);
    let end = a.sample(&blocks, 0, 10.0).unwrap();
    assert!((end.translation[0] - 20.0).abs() < 0.01, "{end:?}");

    assert_eq!(c.bindings.len(), 1);
    let bd = &c.bindings[0];
    assert_eq!(bd.animation, Some(0));
    assert_eq!(bd.transform_track_to_bone_indices, vec![1]);
    assert_eq!(bd.blend_hint, BlendHint::Additive);

    // Apply the sampled track to the bone it binds and walk to model space.
    let mut pose = sk.reference_pose.clone();
    pose[bd.transform_track_to_bone_indices[0] as usize] = mid;
    let model = sk.to_model_space(&pose).unwrap();
    assert!((model[1].translation[0] - 10.0).abs() < 0.01);
    assert_eq!(c.spline_animations().count(), 1);
}
