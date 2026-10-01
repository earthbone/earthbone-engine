// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, NodeKind};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::Packfile;

fn fixture() -> BehaviorGraph {
    let mut builder = PackfileBuilder::new();
    let root = builder.object("hkRootLevelContainer", 16);
    let graph = builder.object("hkbBehaviorGraph", 304);
    let variant = builder.array_structs(root, 0, 24, 1);
    builder.string(variant, 0, "hkbBehaviorGraph");
    builder.string(variant, 8, "hkbBehaviorGraph");
    builder.ptr(variant, 16, graph);
    let wrapper = builder.object("hkbModifierGenerator", 88);
    builder.ptr(graph, 128, wrapper);
    let modifier = builder.object("BSLookAtModifier", 224);
    builder.string(wrapper, 56, "wrapper");
    builder.ptr(wrapper, 72, modifier);
    builder.string(modifier, 56, "look");
    builder.set_bool(modifier, 72, true);
    builder.set_bool(modifier, 80, true);
    builder.set_f32(modifier, 120, 35.0);
    builder.set_f32(modifier, 132, 0.75);
    builder.set_bool(modifier, 140, true);
    builder.set_vec4(modifier, 144, [2.0, 3.0, 4.0, 0.0]);
    let bones = builder.array_structs(modifier, 88, 64, 1);
    builder.set_i16(bones, 0, 3);
    builder.set_vec4(bones, 16, [0.0, 0.0, 1.0, 0.0]);
    builder.set_f32(bones, 32, 20.0);
    builder.set_f32(bones, 36, 0.5);
    builder.set_bool(bones, 44, true);
    let bytes = builder.finish();
    BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap()
}

#[test]
fn anm_32_decodes_authored_look_at_bones_target_and_limits() {
    let graph = fixture();
    let NodeKind::LookAtModifier {
        enable,
        bones,
        limit_angle_degrees,
        on_gain,
        use_bone_gains,
        target_location,
        look_at_target,
        ..
    } = &graph.find_by_name("look").unwrap().kind
    else {
        panic!("look-at modifier should be decoded")
    };

    assert!(*enable);
    assert_eq!(bones.len(), 1);
    assert_eq!(bones[0].index, 3);
    assert_eq!(bones[0].forward_axis, [0.0, 0.0, 1.0, 0.0]);
    assert_eq!(bones[0].limit_angle_degrees, 20.0);
    assert_eq!(*limit_angle_degrees, 35.0);
    assert_eq!(*on_gain, 0.75);
    assert!(*use_bone_gains);
    assert_eq!(*target_location, [2.0, 3.0, 4.0, 0.0]);
    assert!(*look_at_target);
}
