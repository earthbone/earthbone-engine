// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::interp::{ClipLibrary, FootIkGroundTarget, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

#[test]
fn foot_ik_clamps_ground_correction_and_changes_only_ankle() {
    let mut b = PackfileBuilder::new();
    let root = b.object("hkRootLevelContainer", 16);
    let graph = b.object("hkbBehaviorGraph", 304);
    let variant = b.array_structs(root, 0, 24, 1);
    b.string(variant, 8, "hkbBehaviorGraph"); b.ptr(variant, 16, graph);
    let wrapper = b.object("hkbModifierGenerator", 88);
    let ik = b.object("hkbFootIkModifier", 256);
    b.ptr(graph, 128, wrapper); b.ptr(wrapper, 72, ik); b.set_bool(ik, 72, true);
    b.set_f32(ik, 80, 1.0); b.set_f32(ik, 84, 1.0); b.set_f32(ik, 88, 1.0);
    b.set_f32(ik, 144, 2.0); b.set_f32(ik, 148, 1.0); b.set_f32(ik, 192, 0.0);
    let leg = b.array_structs(ik, 128, 160, 1);
    b.set_f32(leg, 120, 3.0); b.set_f32(leg, 124, -3.0);
    b.set_i16(leg, 144, 1); b.set_i16(leg, 146, 2); b.set_i16(leg, 148, 3);
    let graph = BehaviorGraph::from_packfile(&Packfile::parse(&b.finish()).unwrap()).unwrap();
    let mut pose = vec![QsTransform::IDENTITY; 4];
    pose[3].translation[2] = 5.0;
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, pose.clone());
    vm.set_foot_ik_ground_targets(&[FootIkGroundTarget { ankle_index: 3, vertical_correction: 9.0 }]);
    let out = vm.update(1.0 / 60.0).pose.clone();
    assert_eq!(out[0], pose[0]); assert_ne!(out[1].rotation, pose[1].rotation); assert_ne!(out[2].rotation, pose[2].rotation);
    assert_eq!(out[3].translation[2], 7.0);
}
