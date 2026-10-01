// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! `hkbKeyframeBonesModifier` parsing and local-pose application.

use soe_hkx::behavior::{BehaviorGraph, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

#[derive(Clone, Copy)]
struct Keyframe {
    position: [f32; 4],
    rotation: [f32; 4],
    bone: i16,
    valid: bool,
}

fn behavior(enable: bool, entries: &[Keyframe], bones: Option<&[i16]>, is_32bit: bool) -> Vec<u8> {
    let mut b = if is_32bit {
        PackfileBuilder::new_32bit()
    } else {
        PackfileBuilder::new()
    };
    let container = b.object("hkRootLevelContainer", if is_32bit { 12 } else { 16 });
    let graph = b.object("hkbBehaviorGraph", if is_32bit { 176 } else { 304 });
    let variant = b.array_structs(container, 0, if is_32bit { 12 } else { 24 }, 1);
    b.string(variant, 0, "hkbBehaviorGraph");
    b.string(variant, if is_32bit { 4 } else { 8 }, "hkbBehaviorGraph");
    b.ptr(variant, if is_32bit { 8 } else { 16 }, graph);

    let wrapper = b.object("hkbModifierGenerator", if is_32bit { 48 } else { 88 });
    let modifier = b.object("hkbKeyframeBonesModifier", if is_32bit { 60 } else { 104 });
    b.ptr(graph, if is_32bit { 76 } else { 128 }, wrapper);
    b.ptr(wrapper, if is_32bit { 40 } else { 72 }, modifier);
    let enable_offset = if is_32bit { 40 } else { 72 };
    let info_offset = if is_32bit { 44 } else { 80 };
    let list_offset = if is_32bit { 56 } else { 96 };
    b.set_bool(modifier, enable_offset, enable);
    let infos = b.array_structs(modifier, info_offset, 48, entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let at = infos + index as u32 * 48;
        b.set_vec4(at, 0, entry.position);
        b.set_vec4(at, 16, entry.rotation);
        b.set_i16(at, 32, entry.bone);
        b.set_bool(at, 34, entry.valid);
    }
    if let Some(bones) = bones {
        let list = b.object("hkbBoneIndexArray", if is_32bit { 40 } else { 64 });
        b.ptr(modifier, list_offset, list);
        b.array_i16(list, if is_32bit { 28 } else { 48 }, bones);
    }
    b.finish()
}

fn parse(bytes: &[u8]) -> BehaviorGraph {
    BehaviorGraph::from_packfile(&Packfile::parse(bytes).unwrap()).unwrap()
}

fn pose() -> Vec<QsTransform> {
    vec![
        QsTransform {
            translation: [4.0, 5.0, 6.0, 0.0],
            scale: [2.0, 2.0, 2.0, 0.0],
            ..QsTransform::IDENTITY
        },
        QsTransform {
            translation: [7.0, 8.0, 9.0, 0.0],
            ..QsTransform::IDENTITY
        },
    ]
}

fn update(bytes: &[u8]) -> Vec<QsTransform> {
    let graph = parse(bytes);
    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, pose());
    interpreter.update(0.0).pose.clone()
}

#[test]
fn anm_44_enabled_modifier_applies_valid_transforms_only_to_listed_bones() {
    let entries = [
        Keyframe {
            position: [1.0, 2.0, 3.0, 0.0],
            rotation: [0.0, 0.0, 0.707, 0.707],
            bone: 0,
            valid: true,
        },
        Keyframe {
            position: [10.0, 20.0, 30.0, 0.0],
            rotation: [0.0, 0.707, 0.0, 0.707],
            bone: 1,
            valid: true,
        },
    ];
    let graph = parse(&behavior(true, &entries, Some(&[0]), false));
    let modifier = graph
        .nodes
        .iter()
        .find(|node| node.class == "hkbKeyframeBonesModifier")
        .unwrap();
    assert!(matches!(
        &modifier.kind,
        NodeKind::KeyframeBonesModifier {
            keyframe_info,
            keyframed_bones: Some(bones),
            ..
        } if keyframe_info.len() == 2 && bones == &[0]
    ));
    let clips = ClipLibrary::new();
    let mut interpreter = Interpreter::new(&graph, &clips, pose());
    let output = &interpreter.update(0.0).pose;
    assert_eq!(output[0].translation, entries[0].position);
    assert_eq!(output[0].rotation, entries[0].rotation);
    assert_eq!(output[0].scale, [2.0, 2.0, 2.0, 0.0]);
    assert_eq!(output[1], pose()[1]);
}

#[test]
fn anm_44_invalid_entry_does_not_change_the_pose() {
    let entry = Keyframe {
        position: [1.0, 2.0, 3.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        bone: 1,
        valid: false,
    };
    assert_eq!(update(&behavior(true, &[entry], None, false)), pose());
}

#[test]
fn anm_44_disabled_modifier_does_not_change_the_pose() {
    let entry = Keyframe {
        position: [1.0, 2.0, 3.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        bone: 0,
        valid: true,
    };
    assert_eq!(update(&behavior(false, &[entry], None, false)), pose());
}

#[test]
fn anm_44_keyframe_layout_decodes_from_32_bit_packfiles() {
    let entry = Keyframe {
        position: [2.0, 3.0, 4.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        bone: 1,
        valid: true,
    };
    let output = update(&behavior(true, &[entry], None, true));
    assert_eq!(output[1].translation, entry.position);
    assert_eq!(output[1].rotation, entry.rotation);
}
