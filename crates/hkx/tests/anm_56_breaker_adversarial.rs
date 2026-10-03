// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::behavior::{
    transition_flags as flags, BehaviorGraph, NodeKind, StateMachine, TimeInterval,
};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::Packfile;

fn machine(graph: &mut BehaviorGraph) -> &mut StateMachine {
    let root = graph.root.unwrap();
    let NodeKind::StateMachine(machine) = &mut graph.nodes[root.0].kind else {
        panic!("machine")
    };
    machine
}

#[test]
fn anm_56_admitted_request_starts_when_time_initiate_window_opens() {
    let bytes = common::graph(&common::Root::Machine, "a", "b");
    let mut graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    let sm = machine(&mut graph);
    sm.wildcard_transitions.clear();
    sm.return_to_previous_state_event_id = -1;
    for state in &mut sm.states {
        state.enter_notify_events.clear();
        state.exit_notify_events.clear();
        state.generator = None;
    }
    sm.states[1].transitions.clear();
    let transition = &mut sm.states[0].transitions[0];
    transition.event_id = 10;
    transition.transition = None;
    transition.condition = None;
    transition.flags = flags::USE_INITIATE_INTERVAL;
    transition.initiate_interval = TimeInterval {
        enter_event_id: -1,
        exit_event_id: -1,
        enter_time: 1.0,
        exit_time: 2.0,
    };

    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.send_event(10);
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(0));

    vm.update(1.0);
    assert_eq!(
        vm.current_state(graph.root.unwrap()),
        Some(1),
        "a pending transition should start at the inclusive initiate enter time"
    );
}
