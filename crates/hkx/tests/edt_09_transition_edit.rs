// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::behavior::{BehaviorGraph, NodeId, NodeKind};
use soe_hkx::document::PackfileDocument;
use soe_hkx::{Error, Packfile};

fn fixture() -> Vec<u8> {
    common::graph(&common::Root::Machine, "idle", "walk")
}

fn root_machine(graph: &BehaviorGraph) -> NodeId {
    let id = graph.root.expect("fixture has a root");
    assert!(matches!(graph.nodes[id.0].kind, NodeKind::StateMachine(_)));
    id
}

#[test]
fn edt_09_supported_edit_changes_one_transition_and_roundtrips() {
    let original = fixture();
    let original_packfile = Packfile::parse(&original).unwrap();
    let original_graph = BehaviorGraph::from_packfile(&original_packfile).unwrap();
    let mut expected = original_graph.clone();
    let machine_id = root_machine(&original_graph);
    let NodeKind::StateMachine(machine) = &mut expected.nodes[machine_id.0].kind else {
        unreachable!();
    };
    let transition = &mut machine.states[0].transitions[0];
    assert_eq!(transition.to_state_id, 1);
    transition.to_state_id = 0;

    let mut document = PackfileDocument::parse(&original).unwrap();
    original_graph
        .edit_transition_destination(&mut document, machine_id, 0, 0, 0)
        .unwrap();
    let rewritten = document.to_bytes().unwrap();
    let reread_document = PackfileDocument::parse(&rewritten).unwrap();
    assert_eq!(reread_document.sections, document.sections);
    let reread_packfile = Packfile::parse(&rewritten).unwrap();
    let reread_graph = BehaviorGraph::from_packfile(&reread_packfile).unwrap();
    assert_eq!(reread_graph, expected);

    let NodeKind::StateMachine(before) = &original_graph.nodes[machine_id.0].kind else {
        unreachable!();
    };
    let NodeKind::StateMachine(after) = &reread_graph.nodes[machine_id.0].kind else {
        unreachable!();
    };
    let changed = before
        .states
        .iter()
        .flat_map(|state| &state.transitions)
        .zip(after.states.iter().flat_map(|state| &state.transitions))
        .filter(|(left, right)| left != right)
        .count();
    assert_eq!(changed, 1);
}

#[test]
fn edt_09_rejects_missing_destination_without_changing_document() {
    let original = fixture();
    let graph = BehaviorGraph::from_packfile(&Packfile::parse(&original).unwrap()).unwrap();
    let machine_id = root_machine(&graph);
    let mut document = PackfileDocument::parse(&original).unwrap();
    let before = document.clone();
    assert!(matches!(
        graph.edit_transition_destination(&mut document, machine_id, 0, 0, 999),
        Err(Error::Unsupported(message)) if message == "destination state does not exist"
    ));
    assert_eq!(document, before);
}

#[test]
fn edt_09_rejects_invalid_locations_and_stale_documents_without_mutation() {
    let original = fixture();
    let graph = BehaviorGraph::from_packfile(&Packfile::parse(&original).unwrap()).unwrap();
    let machine_id = root_machine(&graph);
    let clip_id = graph
        .nodes
        .iter()
        .find(|node| matches!(node.kind, NodeKind::Clip(_)))
        .unwrap()
        .id;
    let cases = [
        (machine_id, 999, 0, 1, "source state does not exist"),
        (machine_id, 0, 99, 1, "transition index is out of range"),
        (clip_id, 0, 0, 1, "node is not a state machine"),
    ];
    for (node, from, index, to, expected) in cases {
        let mut document = PackfileDocument::parse(&original).unwrap();
        let before = document.clone();
        assert!(matches!(
            graph.edit_transition_destination(&mut document, node, from, index, to),
            Err(Error::Unsupported(message)) if message == expected
        ));
        assert_eq!(document, before);
    }

    let mut stale = PackfileDocument::parse(&original).unwrap();
    graph
        .edit_transition_destination(&mut stale, machine_id, 0, 0, 0)
        .unwrap();
    let before = stale.clone();
    assert!(matches!(
        graph.edit_transition_destination(&mut stale, machine_id, 0, 0, 1),
        Err(Error::Unsupported(message)) if message == "document does not match the loaded graph transition"
    ));
    assert_eq!(stale, before);
}
