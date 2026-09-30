// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, BehaviorGraphData, LookAtBone, Node, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::QsTransform;

fn graph() -> BehaviorGraph {
    BehaviorGraph {
        name: "look".into(),
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
                class: "BSLookAtModifier".into(),
                name: "look".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::LookAtModifier {
                    enable: true,
                    bones: vec![
                        LookAtBone {
                            index: 0,
                            forward_axis: [0.0, 1.0, 0.0, 0.0],
                            limit_angle_degrees: 90.0,
                            on_gain: 1.0,
                            off_gain: 1.0,
                            enabled: true,
                        },
                        LookAtBone {
                            index: 1,
                            forward_axis: [0.0, 1.0, 0.0, 0.0],
                            limit_angle_degrees: 90.0,
                            on_gain: 1.0,
                            off_gain: 1.0,
                            enabled: false,
                        },
                    ],
                    eye_bones: vec![],
                    limit_angle_degrees: 90.0,
                    on_gain: 1.0,
                    off_gain: 1.0,
                    use_bone_gains: false,
                    target_location: [1.0, 0.0, 0.0, 0.0],
                    look_at_target: true,
                    look_at_camera: false,
                    look_at_camera_location: [0.0; 3],
                },
            },
        ],
    }
}

#[test]
fn anm_32_disabled_look_at_bone_remains_unchanged() {
    let graph = graph();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY; 3]);
    vm.update(1.0);
    assert_ne!(vm.pose()[0].rotation, QsTransform::IDENTITY.rotation);
    assert_eq!(vm.pose()[1], QsTransform::IDENTITY);
    assert_eq!(vm.pose()[2], QsTransform::IDENTITY);
}
