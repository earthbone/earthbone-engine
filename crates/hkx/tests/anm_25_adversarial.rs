// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, BehaviorGraphData, Node, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::QsTransform;

fn graph() -> BehaviorGraph {
    BehaviorGraph {
        name: "direct-at adversarial".into(),
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
                class: "BSDirectAtModifier".into(),
                name: "aim".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::DirectAt {
                    enable: true,
                    direct_at_target: true,
                    source_bone_index: 0,
                    start_bone_index: 0,
                    end_bone_index: 0,
                    limit_heading_degrees: 20.0,
                    limit_pitch_degrees: 15.0,
                    offset_heading_degrees: 0.0,
                    offset_pitch_degrees: 0.0,
                    on_gain: 1.0,
                    off_gain: 1.0,
                    target_location: [10.0, 0.0, 0.0, 0.0],
                    user_info: 0,
                    direct_at_camera: false,
                    direct_at_camera_location: [0.0; 3],
                    active: true,
                    current_heading_offset: 0.0,
                    current_pitch_offset: 0.0,
                },
            },
        ],
    }
}

#[test]
fn anm_25_zero_pose_weight_cannot_rotate_chain() {
    let graph = graph();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    let before = vm.pose().to_vec();
    vm.update(1.0);
    assert_eq!(vm.pose(), before);
}
