// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, BehaviorGraphData, Node, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::QsTransform;

#[test]
fn disabled_rotate_modifier_does_not_rotate_and_enabled_rotation_uses_dt_once() {
    let graph = BehaviorGraph {
        name: "rotate adversarial".into(),
        variable_mode: 1,
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
                offset: 0,
                class: "hkbRotateCharacterModifier".into(),
                name: "rotate".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::RotateCharacterModifier {
                    enable: false,
                    degrees_per_second: 90.0,
                    speed_multiplier: 2.0,
                    axis_of_rotation: [0.0, 2.0, 0.0, 0.0],
                },
            },
        ],
    };
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    assert_eq!(vm.update(0.5).pose[0].rotation, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(vm.update(0.5).pose[0].rotation, [0.0, 0.0, 0.0, 1.0]);

    let mut enabled_graph = graph;
    if let NodeKind::RotateCharacterModifier { enable, .. } = &mut enabled_graph.nodes[1].kind {
        *enable = true;
    }
    let mut vm = Interpreter::new(&enabled_graph, &clips, vec![QsTransform::IDENTITY]);
    let first = vm.update(0.5).pose[0].rotation;
    assert!((first[1] - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.001);
    assert!((first[3] - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.001);
    let second = vm.update(0.5).pose[0].rotation;
    assert!(second[1].abs() > 0.999);
    assert!(second[3].abs() < 0.001);
}
