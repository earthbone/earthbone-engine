// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, BehaviorGraphData, EventProperty, Node, NodeId, NodeKind, StateInfo, StateMachine, VariableInfo, VariableType};
use soe_hkx::interp::{ClipLibrary, Interpreter};

fn graph() -> BehaviorGraph {
    let state = StateInfo { name: "current".into(), state_id: 4, probability: 1.0, enable: true,
        generator: None, enter_notify_events: vec![], exit_notify_events: vec![], transitions: vec![], bindings: None };
    let machine = StateMachine { event_to_send_when_state_or_transition_changes: EventProperty { id: -1, has_payload: false },
        start_state_id: 4, return_to_previous_state_event_id: -1, random_transition_event_id: -1,
        transition_to_next_higher_state_event_id: -1, transition_to_next_lower_state_event_id: -1,
        sync_variable_index: -1, wrap_around_state_id: false, max_simultaneous_transitions: 0,
        start_state_mode: 0, self_transition_mode: 0, states: vec![state], wildcard_transitions: vec![], has_start_state_chooser: false };
    BehaviorGraph { name: "iState".into(), variable_mode: 0, root: Some(NodeId(0)),
        data: BehaviorGraphData { variable_infos: vec![VariableInfo { ty: VariableType::Int32, ..Default::default() }],
            variable_names: vec!["iState".into()], ..Default::default() },
        nodes: vec![
            Node { id: NodeId(0), offset: 0, class: "hkbModifierGenerator".into(), name: "root".into(), user_data: 0, bindings: None,
                kind: NodeKind::ModifierGenerator { modifier: Some(NodeId(1)), generator: Some(NodeId(2)) } },
            Node { id: NodeId(1), offset: 1, class: "BSIStateManagerModifier".into(), name: "manager".into(), user_data: 0, bindings: None,
                kind: NodeKind::IStateManager { enable: true, state_variable: 0, state_data: vec![soe_hkx::behavior::IStateData { state_machine: NodeId(2), state_id: 3, state_to_set_as: 99 }] } },
            Node { id: NodeId(2), offset: 2, class: "hkbStateMachine".into(), name: "machine".into(), user_data: 0, bindings: None,
                kind: NodeKind::StateMachine(machine) }
        ] }
}

#[test]
fn anm_29_unmatched_or_out_of_range_i_state_mapping_preserves_value() {
    let graph = graph();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.variables.set_word(0, 17);
    vm.update(0.0);
    assert_eq!(vm.variables.word(0), Some(17));
}

#[test]
fn anm_29_invalid_declared_variable_index_does_not_write_another_slot() {
    let mut graph = graph();
    graph.data.variable_infos.push(VariableInfo { ty: VariableType::Int32, ..Default::default() });
    graph.data.variable_names.push("other".into());
    graph.nodes[1].kind = NodeKind::IStateManager { enable: true, state_variable: 7,
        state_data: vec![soe_hkx::behavior::IStateData { state_machine: NodeId(2), state_id: 4, state_to_set_as: 23 }] };
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.variables.set_word(0, 17);
    vm.variables.set_word(1, 29);
    vm.update(0.0);
    assert_eq!(vm.variables.word(0), Some(17));
    assert_eq!(vm.variables.word(1), Some(29));
}
