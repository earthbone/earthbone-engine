// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

fn graph(enabled: bool, ignored_elapsed: f32) -> BehaviorGraph {
    let mut b = PackfileBuilder::new();
    let container = b.object("hkRootLevelContainer", 16);
    let behavior = b.object("hkbBehaviorGraph", 304);
    let variant = b.array_structs(container, 0, 24, 1);
    b.string(variant, 8, "hkbBehaviorGraph");
    b.ptr(variant, 16, behavior);

    let data = b.object("hkbBehaviorGraphData", 128);
    let strings = b.object("hkbBehaviorGraphStringData", 80);
    let values = b.object("hkbVariableValueSet", 64);
    b.ptr(behavior, 136, data);
    b.ptr(data, 120, strings);
    b.ptr(data, 112, values);

    let wrapper = b.object("hkbModifierGenerator", 96);
    let modifier = b.object("hkbGetUpModifier", 128);
    b.ptr(behavior, 128, wrapper);
    b.ptr(wrapper, 72, modifier);
    b.set_bool(modifier, 72, enabled);
    b.set_f32(modifier, 80, 0.0);
    b.set_f32(modifier, 84, 1.0);
    b.set_f32(modifier, 88, 0.0);
    b.set_f32(modifier, 92, 0.0);
    b.set_f32(modifier, 96, 0.4);
    b.set_f32(modifier, 100, 0.2);
    b.set_i16(modifier, 104, 1);
    b.set_i16(modifier, 106, 3);
    b.set_i16(modifier, 108, 4);
    b.set_f32(modifier, 112, ignored_elapsed);
    b.set_f32(modifier, 116, ignored_elapsed);
    b.set_bool(modifier, 120, true);

    let bytes = b.finish();
    BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap()
}

fn reference_pose() -> Vec<QsTransform> {
    vec![QsTransform::IDENTITY; 5]
}

#[test]
fn anm_43_decodes_ground_timing_and_the_three_bone_indices() {
    let graph = graph(true, 9.0);
    let Some(node) = graph
        .nodes
        .iter()
        .find(|node| node.class == "hkbGetUpModifier")
    else {
        panic!("get-up modifier was not decoded");
    };
    assert!(matches!(
        node.kind,
        NodeKind::GetUpModifier {
            enable: true,
            ground_normal: [0.0, 1.0, 0.0, 0.0],
            duration: 0.4,
            align_with_ground_duration: 0.2,
            root_bone_index: 1,
            other_bone_index: 3,
            another_bone_index: 4,
        }
    ));
}

#[test]
fn anm_43_ground_alignment_changes_only_the_selected_bones() {
    let graph = graph(true, 0.0);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, reference_pose());

    let pose = vm.update(0.1).pose.clone();
    assert_eq!(pose[0], QsTransform::IDENTITY);
    assert_ne!(pose[1].rotation, QsTransform::IDENTITY.rotation);
    assert_eq!(pose[2], QsTransform::IDENTITY);
    assert_ne!(pose[3].rotation, QsTransform::IDENTITY.rotation);
    assert_ne!(pose[4].rotation, QsTransform::IDENTITY.rotation);
    for index in [1, 3, 4] {
        assert_eq!(pose[index].translation, QsTransform::IDENTITY.translation);
        assert_eq!(pose[index].scale, QsTransform::IDENTITY.scale);
    }
}

#[test]
fn anm_43_alignment_and_duration_control_the_adjustment() {
    let graph = graph(true, 0.0);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, reference_pose());

    let early = vm.update(0.1).pose[1].rotation;
    let aligned = vm.update(0.1).pose[1].rotation;
    let expired = vm.update(0.2).pose[1].rotation;
    assert_ne!(early, aligned);
    assert_eq!(expired, QsTransform::IDENTITY.rotation);
}

#[test]
fn anm_43_inherited_enable_and_ignored_runtime_state_are_respected() {
    let disabled = graph(false, 0.0);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&disabled, &clips, reference_pose());
    assert_eq!(vm.update(0.1).pose, reference_pose());

    let zero_state = graph(true, 0.0);
    let large_ignored_state = graph(true, 1000.0);
    let mut zero_vm = Interpreter::new(&zero_state, &clips, reference_pose());
    let mut ignored_vm = Interpreter::new(&large_ignored_state, &clips, reference_pose());
    assert_eq!(
        zero_vm.update(0.1).pose,
        ignored_vm.update(0.1).pose,
        "serialized ignored timers must not seed interpreter time"
    );
}
