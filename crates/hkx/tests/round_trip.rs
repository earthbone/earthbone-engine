// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! A synthetic `skeleton.hkx`-shaped file with one skeleton, one spline
//! animation and one binding, written with the builder and read back.

use soe_hkx::animation::Animation;
use soe_hkx::skeleton::{AnimationContainer, BlendHint};
use soe_hkx::spline::encode;
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

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

fn build(le: bool) -> Vec<u8> {
    let off = |x86, x64| if le { x86 } else { x64 };
    let mut b = if le {
        PackfileBuilder::new_32bit()
    } else {
        PackfileBuilder::new()
    };
    let root = b.object("hkRootLevelContainer", off(12, 16) as usize);
    let container = b.object("hkaAnimationContainer", off(68, 96) as usize);
    let variant = b.array_structs(root, 0, off(12, 24) as usize, 1);
    b.string(variant, 0, "Merged Animation Container");
    b.string(variant, off(4, 8), "hkaAnimationContainer");
    b.ptr(variant, off(8, 16), container);

    let skeleton = b.object("hkaSkeleton", off(84, 120) as usize);
    b.string(skeleton, off(8, 16), "NPC Root [Root]");
    b.array_i16(skeleton, off(12, 24), &[-1, 0]);
    let bones = b.array_structs(skeleton, off(24, 40), off(8, 16) as usize, 2);
    b.string(bones, 0, "NPC Root [Root]");
    b.string(bones + off(8, 16), 0, "NPC Pelvis [Pelv]");
    b.set_bool(bones + off(8, 16), off(4, 8), true);
    let pelvis = QsTransform {
        translation: [0.0, 0.0, 60.0, 0.0],
        ..QsTransform::IDENTITY
    };
    b.array_qs_transform(skeleton, off(36, 56), &[QsTransform::IDENTITY, pelvis]);
    b.array_strings(skeleton, off(60, 88), &["Weapon_Left", "Weapon_Right"]);
    b.array_ptrs(container, off(8, 16), &[skeleton]);

    let anim = b.object("hkaSplineCompressedAnimation", off(132, 176) as usize);
    b.set_i32(anim, off(8, 16), 5);
    b.set_f32(anim, off(12, 20), 2.0 / 30.0);
    b.set_i32(anim, off(16, 24), 1);
    b.set_i32(anim, off(20, 28), 0);
    b.set_i32(anim, off(40, 56), 3);
    b.set_i32(anim, off(44, 60), 1);
    b.set_i32(anim, off(48, 64), 256);
    b.set_i32(anim, off(52, 68), 4);
    b.set_f32(anim, off(56, 72), 256.0 / 30.0);
    b.set_f32(anim, off(60, 76), 30.0 / 256.0);
    b.set_f32(anim, off(64, 80), 1.0 / 30.0);
    b.array_u32(anim, off(68, 88), &[0]);
    b.array_u32(anim, off(80, 104), &[0]);
    b.array_u32(anim, off(92, 120), &[4]);
    let data = block();
    b.array_bytes(anim, off(116, 152), &data, data.len());
    let tracks = b.array_structs(anim, off(28, 40), off(16, 24) as usize, 1);
    b.string(tracks, 0, "NPC Pelvis [Pelv]");
    let ann = b.array_structs(tracks, off(4, 8), off(8, 16) as usize, 1);
    b.set_f32(ann, 0, 0.05);
    b.string(ann, off(4, 8), "FootLeft");
    b.array_ptrs(container, off(20, 32), &[anim]);

    let binding = b.object("hkaAnimationBinding", off(44, 72) as usize);
    b.string(binding, off(8, 16), "NPC Root [Root]");
    b.ptr(binding, off(12, 24), anim);
    b.array_i16(binding, off(16, 32), &[1]);
    b.set_i8(binding, off(40, 64), 1);
    b.array_ptrs(container, off(32, 48), &[binding]);
    b.finish()
}

fn check_round_trip(le: bool) {
    let bytes = build(le);
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

#[test]
fn se_skeleton_and_clip_round_trip() {
    check_round_trip(false);
}

#[test]
fn le_skeleton_and_clip_round_trip() {
    check_round_trip(true);
}

#[test]
fn both_layouts_decode_the_same_animation_model() {
    let le = Packfile::parse(&build(true)).unwrap();
    let se = Packfile::parse(&build(false)).unwrap();
    let mut left = AnimationContainer::from_packfile(&le).unwrap();
    let mut right = AnimationContainer::from_packfile(&se).unwrap();
    for model in [&mut left, &mut right] {
        for binding in &mut model.bindings {
            binding.animation_offset = binding.animation_offset.map(|_| 0);
        }
    }
    assert_eq!(left, right);
}
