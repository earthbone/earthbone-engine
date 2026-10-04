// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::behavior::{
    transition_flags as flags, BehaviorGraph, Condition, NodeId, NodeKind, StateMachine,
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

fn fixture() -> BehaviorGraph {
    let bytes = common::graph(&common::Root::Machine, "a", "b");
    let mut graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    let b = graph.find_by_name("WalkClip").unwrap().id;
    let mut c = graph.nodes[b.0].clone();
    c.id = NodeId(graph.nodes.len());
    c.bindings = None;
    let c_id = c.id;
    graph.nodes.push(c);
    let sm = machine(&mut graph);
    sm.wildcard_transitions.clear();
    sm.return_to_previous_state_event_id = -1;
    sm.states[1].transitions.clear();
    let mut third = sm.states[1].clone();
    third.state_id = 2;
    third.generator = Some(c_id);
    third.transitions.clear();
    sm.states.push(third);
    let local = &mut sm.states[0].transitions[0];
    local.event_id = 10;
    local.condition = None;
    local.flags = 0;
    graph
}

#[test]
fn anm_54_false_local_tie_falls_through_to_passing_wildcard() {
    let mut graph = fixture();
    let sm = machine(&mut graph);
    sm.states[0].transitions[0].condition = Some(Condition::Expression("false".into()));
    let mut wildcard = sm.states[0].transitions[0].clone();
    wildcard.condition = None;
    wildcard.to_state_id = 2;
    wildcard.flags = flags::IS_LOCAL_WILDCARD;
    sm.wildcard_transitions.push(wildcard);

    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.send_event(10);
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(2));
}
