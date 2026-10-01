// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{
    BehaviorGraph, BehaviorGraphData, Binding, EventProperty, EventRange, Node, NodeId, NodeKind,
    VariableBindingSet, VariableInfo, VariableType,
};
use soe_hkx::interp::{ClipLibrary, Event, Interpreter};

fn graph() -> BehaviorGraph {
    BehaviorGraph {
        name: "range adversarial".into(),
        variable_mode: 1,
        root: Some(NodeId(0)),
        data: BehaviorGraphData {
            variable_names: vec!["Input".into()],
            variable_infos: vec![VariableInfo {
                role: 0,
                role_flags: 0,
                ty: VariableType::Real,
            }],
            ..Default::default()
        },
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
                class: "hkbEventsFromRangeModifier".into(),
                name: "ranges".into(),
                user_data: 0,
                bindings: Some(VariableBindingSet {
                    bindings: vec![Binding {
                        member_path: "inputValue".into(),
                        variable_index: 0,
                        bit_index: -1,
                        binding_type: 0,
                    }],
                    index_of_binding_to_enable: -1,
                }),
                kind: NodeKind::EventsFromRangeModifier {
                    enable: true,
                    input_value: 0.0,
                    lower_bound: 0.0,
                    ranges: vec![
                        (
                            1.0,
                            EventRange {
                                event: EventProperty {
                                    id: 10,
                                    has_payload: false,
                                },
                                event_mode: 0,
                            },
                        ),
                        (
                            2.0,
                            EventRange {
                                event: EventProperty {
                                    id: 20,
                                    has_payload: false,
                                },
                                event_mode: 1,
                            },
                        ),
                    ],
                },
            },
        ],
    }
}

#[test]
fn adjacent_ranges_use_lower_inclusive_upper_exclusive_boundaries() {
    let graph = graph();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.update(0.0);
    assert_eq!(vm.take_events(), [Event { id: 10 }]);
    vm.update(0.0);
    assert!(vm.take_events().is_empty());
    vm.variables.set_real(0, 1.0);
    vm.update(0.0);
    assert_eq!(vm.take_events(), [Event { id: 20 }]);
    vm.variables.set_real(0, 2.0);
    vm.update(0.0);
    assert!(vm.take_events().is_empty());
}
