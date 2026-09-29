// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::behavior::{BehaviorGraph, NodeId, NodeKind};
use soe_hkx::document::PackfileDocument;
use soe_hkx::Packfile;

#[test]
fn edt_09_reparsed_edit_is_stable_and_preserves_other_transitions() {
    let bytes = common::graph(&common::Root::Machine, "idle", "walk");
    let packfile = Packfile::parse(&bytes).unwrap();
    let graph = BehaviorGraph::from_packfile(&packfile).unwrap();
    let machine_id: NodeId = graph.root.unwrap();
    let mut document = PackfileDocument::parse(&bytes).unwrap();
    graph
        .edit_transition_destination(&mut document, machine_id, 0, 0, 0)
        .unwrap();
    let edited = document.to_bytes().unwrap();
    let parsed_graph = BehaviorGraph::from_packfile(&Packfile::parse(&edited).unwrap()).unwrap();
    let NodeKind::StateMachine(machine) = &parsed_graph.nodes[machine_id.0].kind else {
        panic!("fixture root must remain a state machine");
    };
    assert_eq!(machine.states[0].transitions[0].to_state_id, 0);
    assert_eq!(
        machine.states[1].transitions,
        graph_machine_transitions(&graph, machine_id)
    );

    let mut second_document = PackfileDocument::parse(&edited).unwrap();
    parsed_graph
        .edit_transition_destination(&mut second_document, machine_id, 0, 0, 0)
        .unwrap();
    assert_eq!(second_document.to_bytes().unwrap(), edited);
}

fn graph_machine_transitions(
    graph: &BehaviorGraph,
    id: NodeId,
) -> Vec<soe_hkx::behavior::TransitionInfo> {
    let NodeKind::StateMachine(machine) = &graph.nodes[id.0].kind else {
        panic!("fixture root must be a state machine");
    };
    machine.states[1].transitions.clone()
}
