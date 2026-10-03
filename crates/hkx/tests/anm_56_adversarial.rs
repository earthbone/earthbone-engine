// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial boundary checks for ANM-56 transition interval replay.

mod common;

use soe_hkx::behavior::{
    transition_flags as flags, BehaviorGraph, NodeKind, StateMachine, TimeInterval,
};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::Packfile;

fn machine(graph: &mut BehaviorGraph) -> &mut StateMachine {
    let root = graph.root.unwrap();
    let NodeKind::StateMachine(sm) = &mut graph.nodes[root.0].kind else {
        panic!("state machine fixture")
    };
    sm
}

fn delayed_time_graph() -> BehaviorGraph {
    let bytes = common::graph(&common::Root::Machine, "a", "b");
    let mut graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    let sm = machine(&mut graph);
    sm.wildcard_transitions.clear();
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
    graph
}

#[test]
fn anm_56_delayed_request_starts_at_the_initiate_enter_boundary() {
    let graph = delayed_time_graph();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);

    vm.send_event(10);
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(0));

    vm.update(0.5);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(0));
    vm.update(0.5);
    assert_eq!(
        vm.current_state(graph.root.unwrap()),
        Some(1),
        "the admitted request should start when the half-open initiate window opens"
    );
}
