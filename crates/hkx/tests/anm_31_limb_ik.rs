// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, BehaviorGraphData, Node, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Packfile, QsTransform};

fn graph(limit_angle_degrees: f32) -> BehaviorGraph {
    BehaviorGraph {
        name: "limb ik fixture".into(),
        variable_mode: 0,
        root: Some(NodeId(0)),
        data: BehaviorGraphData::default(),
        nodes: vec![
            Node {
                id: NodeId(0),
                offset: 0,
                class: "hkbModifierGenerator".into(),
                name: "root".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::ModifierGenerator {
                    modifier: Some(NodeId(1)),
                    generator: None,
                },
            },
            Node {
                id: NodeId(1),
                offset: 1,
                class: "BSLimbIKModifier".into(),
                name: "left arm".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::LimbIk {
                    enable: true,
                    limit_angle_degrees,
                    start_bone_index: 0,
                    end_bone_index: 2,
                    gain: 1.0,
                    bone_radius: 0.0,
                    cast_offset: 0.0,
                },
            },
        ],
    }
}

fn pose() -> Vec<QsTransform> {
    vec![
        QsTransform::IDENTITY,
        QsTransform {
            translation: [0.0, 0.0, 1.0, 0.0],
            ..QsTransform::IDENTITY
        },
        QsTransform {
            translation: [0.0, 0.0, 2.0, 0.0],
            ..QsTransform::IDENTITY
        },
        QsTransform {
            translation: [7.0, 4.0, 3.0, 0.0],
            ..QsTransform::IDENTITY
        },
    ]
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.001
}

fn authored_modifier(limit: f32, start: i16, end: i16) -> Vec<u8> {
    let mut builder = PackfileBuilder::new();
    let container = builder.object("hkRootLevelContainer", 16);
    let graph = builder.object("hkbBehaviorGraph", 304);
    let variants = builder.array_structs(container, 0, 24, 1);
    builder.string(variants, 0, "hkbBehaviorGraph");
    builder.string(variants, 8, "hkbBehaviorGraph");
    builder.ptr(variants, 16, graph);
    builder.string(graph, 56, "limb ik fields");

    let data = builder.object("hkbBehaviorGraphData", 128);
    let strings = builder.object("hkbBehaviorGraphStringData", 80);
    builder.ptr(graph, 136, data);
    builder.ptr(data, 120, strings);
    let wrapper = builder.object("hkbModifierGenerator", 88);
    builder.ptr(graph, 128, wrapper);
    let modifier = builder.object("BSLimbIKModifier", 120);
    builder.ptr(wrapper, 72, modifier);
    builder.set_bool(modifier, 72, true);
    builder.set_f32(modifier, 80, limit);
    builder.set_f32(modifier, 84, 0.0);
    builder.set_i16(modifier, 88, start);
    builder.set_i16(modifier, 90, end);
    builder.set_f32(modifier, 92, 0.75);
    builder.set_f32(modifier, 96, 0.12);
    builder.set_f32(modifier, 100, -0.03);
    builder.set_f32(modifier, 104, 9.0);
    let bytes = builder.finish();
    let _ = strings;
    bytes
}

fn authored_modifier32() -> Vec<u8> {
    let mut builder = PackfileBuilder::new_32bit();
    let container = builder.object("hkRootLevelContainer", 16);
    let graph = builder.object("hkbBehaviorGraph", 120);
    let variants = builder.array_structs(container, 0, 12, 1);
    builder.string(variants, 0, "hkbBehaviorGraph");
    builder.string(variants, 4, "hkbBehaviorGraph");
    builder.ptr(variants, 8, graph);
    builder.string(graph, 32, "limb ik fields");

    let data = builder.object("hkbBehaviorGraphData", 128);
    let strings = builder.object("hkbBehaviorGraphStringData", 80);
    builder.ptr(graph, 80, data);
    builder.ptr(data, 84, strings);
    let wrapper = builder.object("hkbModifierGenerator", 60);
    builder.ptr(graph, 76, wrapper);
    let modifier = builder.object("BSLimbIKModifier", 76);
    builder.ptr(wrapper, 44, modifier);
    builder.set_bool(modifier, 40, true);
    builder.set_f32(modifier, 44, 35.0);
    builder.set_i16(modifier, 52, 2);
    builder.set_i16(modifier, 54, 5);
    builder.set_f32(modifier, 56, 0.75);
    builder.set_f32(modifier, 60, 0.12);
    builder.set_f32(modifier, 64, -0.03);
    builder.set_f32(modifier, 68, 9.0);
    let bytes = builder.finish();
    let _ = strings;
    bytes
}

#[test]
fn anm_31_authored_limb_ik_fields_decode_and_runtime_fields_are_ignored() {
    let bytes = authored_modifier(35.0, 2, 5);
    let packfile = Packfile::parse(&bytes).unwrap();
    let graph = BehaviorGraph::from_packfile(&packfile).unwrap();

    assert_eq!(
        &graph
            .nodes_of_class("BSLimbIKModifier")
            .next()
            .unwrap()
            .kind,
        &NodeKind::LimbIk {
            enable: true,
            limit_angle_degrees: 35.0,
            start_bone_index: 2,
            end_bone_index: 5,
            gain: 0.75,
            bone_radius: 0.12,
            cast_offset: -0.03,
        }
    );
}

#[test]
fn anm_31_32_bit_authored_offsets_decode_the_same_fields() {
    let bytes = authored_modifier32();
    let packfile = Packfile::parse(&bytes).unwrap();
    let graph = BehaviorGraph::from_packfile(&packfile).unwrap();

    assert_eq!(
        &graph
            .nodes_of_class("BSLimbIKModifier")
            .next()
            .unwrap()
            .kind,
        &NodeKind::LimbIk {
            enable: true,
            limit_angle_degrees: 35.0,
            start_bone_index: 2,
            end_bone_index: 5,
            gain: 0.75,
            bone_radius: 0.12,
            cast_offset: -0.03,
        }
    );
}

#[test]
fn anm_31_invalid_authored_limb_ranges_are_rejected() {
    for (start, end) in [(-1, 2), (3, 3), (5, 2)] {
        let bytes = authored_modifier(35.0, start, end);
        let packfile = Packfile::parse(&bytes).unwrap();
        assert!(BehaviorGraph::from_packfile(&packfile).is_err());
    }
}

#[test]
fn anm_31_reachable_target_moves_only_the_declared_limb() {
    let graph = graph(90.0);
    let library = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &library, pose());
    assert!(vm.set_limb_ik_target(NodeId(1), Some([2.0, 0.0, 0.0])));

    let output = vm.update(1.0 / 60.0);

    assert!(output.pose[0].rotation[1] > 0.0);
    assert!(output.pose[1].rotation[1] > 0.0);
    assert!(output.pose[2].rotation[1] > 0.0);
    assert_eq!(output.pose[3], pose()[3]);
    assert!(output
        .pose
        .iter()
        .zip(pose())
        .all(|(actual, expected)| actual.translation == expected.translation));
}

#[test]
fn anm_31_target_correction_stays_within_the_authored_angle_limit() {
    let graph = graph(25.0);
    let library = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &library, pose());
    assert!(vm.set_limb_ik_target(NodeId(1), Some([2.0, 0.0, 0.0])));

    let output = vm.update(1.0 / 60.0);
    let angle: f32 = output.pose[..=2]
        .iter()
        .map(|transform| 2.0 * transform.rotation[3].acos().to_degrees())
        .sum();

    assert!(close(angle, 25.0), "corrected angle was {angle}");
    assert!(output.pose[1].rotation[1] > 0.0);
}

#[test]
fn anm_31_disabled_or_missing_targets_leave_the_pose_unchanged() {
    let mut disabled_graph = graph(90.0);
    if let NodeKind::LimbIk { enable, .. } = &mut disabled_graph.nodes[1].kind {
        *enable = false;
    }
    let library = ClipLibrary::new();
    let mut vm = Interpreter::new(&disabled_graph, &library, pose());
    assert!(vm.set_limb_ik_target(NodeId(1), Some([2.0, 0.0, 0.0])));
    assert_eq!(vm.update(1.0 / 60.0).pose, pose());

    let enabled_graph = graph(90.0);
    let mut vm = Interpreter::new(&enabled_graph, &library, pose());
    assert_eq!(vm.update(1.0 / 60.0).pose, pose());
}
