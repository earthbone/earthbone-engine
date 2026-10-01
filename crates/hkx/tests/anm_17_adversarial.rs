// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, BehaviorGraphData, Node, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};

#[test]
fn repeated_activation_keeps_child_enabled_until_deactivation() {
    let graph = BehaviorGraph {
        name: "event wrapper adversarial".into(),
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
                class: "hkbEventDrivenModifier".into(),
                name: "wrapper".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::EventDrivenModifier {
                    enable: true,
                    modifier: Some(NodeId(2)),
                    activate_event_id: 1,
                    deactivate_event_id: 2,
                    active_by_default: false,
                },
            },
            Node {
                id: NodeId(2),
                offset: 0,
                class: "hkbModifier".into(),
                name: "child".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::Modifier { enable: true },
            },
        ],
    };
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.update(0.0);
    assert!(!vm.is_active(NodeId(2)));
    vm.send_event(1);
    vm.send_event(1);
    vm.update(0.0);
    assert!(vm.is_active(NodeId(2)));
    vm.send_event(2);
    vm.update(0.0);
    assert!(!vm.is_active(NodeId(2)));
}
