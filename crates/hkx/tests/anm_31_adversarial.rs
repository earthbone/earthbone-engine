// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, BehaviorGraphData, Node, NodeId, NodeKind};
use soe_hkx::{
    interp::{ClipLibrary, Interpreter},
    QsTransform,
};

fn graph() -> BehaviorGraph {
    BehaviorGraph {
        name: "limb".into(),
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
                name: "limb".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::LimbIk {
                    enable: true,
                    limit_angle_degrees: 45.0,
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

#[test]
fn anm_31_missing_target_leaves_reference_pose_unchanged() {
    let graph = graph();
    let pose = vec![QsTransform::IDENTITY; 4];
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, pose);
    let before = vm.pose().to_vec();
    vm.update(1.0 / 60.0);
    assert_eq!(vm.pose(), before);
}
