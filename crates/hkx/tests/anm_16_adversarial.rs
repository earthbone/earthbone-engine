// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, BehaviorGraphData, EventProperty, Node, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Event, Interpreter};

#[test]
fn zero_alarm_fires_once_on_first_enabled_update() {
    let graph = BehaviorGraph {
        name: "timer adversarial".into(),
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
                class: "hkbTimerModifier".into(),
                name: "timer".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::TimerModifier {
                    enable: true,
                    alarm_time_seconds: 0.0,
                    alarm_event: EventProperty {
                        id: 3,
                        has_payload: false,
                    },
                },
            },
        ],
    };
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.update(0.0);
    assert_eq!(vm.take_events(), [Event { id: 3 }]);
    vm.update(0.0);
    assert!(vm.take_events().is_empty());
}
