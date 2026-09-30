// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, BehaviorGraphData, EventProperty, Node, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};

fn graph() -> BehaviorGraph {
    BehaviorGraph {
        name: "event counter".into(),
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
                class: "BSEventEveryNEventsModifier".into(),
                name: "counter".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::EveryNEventsModifier {
                    enable: true,
                    event_to_check_for: EventProperty {
                        id: 4,
                        has_payload: false,
                    },
                    event_to_send: EventProperty {
                        id: 7,
                        has_payload: false,
                    },
                    number_of_events_before_send: 2,
                    minimum_number_of_events_before_send: 2,
                    randomize_number_of_events: false,
                },
            },
        ],
    }
}

#[test]
fn anm_26_mismatch_between_matching_events_never_finishes_interval() {
    let graph = graph();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.update(0.0);
    for event in [4, 99] {
        vm.send_event(event);
        vm.update(0.0);
        assert!(!vm.take_events().iter().any(|item| item.id == 7));
    }
    vm.send_event(4);
    vm.update(0.0);
    assert!(vm.take_events().iter().any(|item| item.id == 7));
}
