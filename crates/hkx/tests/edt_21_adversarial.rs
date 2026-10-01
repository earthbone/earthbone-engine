// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::behavior::{
    BehaviorGraph, BehaviorPatch, BehaviorPatchDiagnostic, BehaviorPatchEdit, NodeKind,
};
use soe_hkx::Packfile;

fn graph() -> BehaviorGraph {
    let bytes = common::graph(&common::Root::Machine, "idle.hkx", "walk.hkx");
    BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap()
}

fn destination(graph: &BehaviorGraph, machine: &str, source: i32, transition: usize) -> i32 {
    let node = graph.find_by_name(machine).unwrap();
    let NodeKind::StateMachine(machine) = &node.kind else {
        panic!("expected a state machine")
    };
    machine.state(source).unwrap().transitions[transition].to_state_id
}

fn transition_patch(name: &str, destination: i32) -> BehaviorPatch {
    BehaviorPatch {
        name: name.to_string(),
        edits: vec![BehaviorPatchEdit::SetTransitionDestination {
            machine: "Locomotion".to_string(),
            from_state_id: 0,
            transition_index: 0,
            to_state_id: destination,
        }],
    }
}

#[test]
fn edt_21_transition_conflicts_are_order_independent_and_report_the_winner() {
    let patches = [
        transition_patch("a_early", 1),
        transition_patch("z_late", 0),
    ];
    let mut forward = graph();
    let forward_report = forward.apply_patches(&patches);
    let mut reverse = graph();
    let reverse_report = reverse.apply_patches(&patches.into_iter().rev().collect::<Vec<_>>());

    assert_eq!(forward, reverse);
    assert_eq!(forward_report, reverse_report);
    assert_eq!(destination(&forward, "Locomotion", 0, 0), 0);
    assert_eq!(
        forward_report.diagnostics,
        [BehaviorPatchDiagnostic::Conflict {
            target: "machine:Locomotion:state:0:transition:0:destination".to_string(),
            previous_patch: "a_early".to_string(),
            winning_patch: "z_late".to_string(),
        }]
    );
}

#[test]
fn edt_21_invalid_late_transition_edit_rejects_the_entire_patch() {
    let mut graph = graph();
    let before = graph.clone();
    let patch = BehaviorPatch {
        name: "atomic-transition".to_string(),
        edits: vec![
            BehaviorPatchEdit::SetTransitionDestination {
                machine: "Locomotion".to_string(),
                from_state_id: 0,
                transition_index: 0,
                to_state_id: 0,
            },
            BehaviorPatchEdit::SetTransitionDestination {
                machine: "Locomotion".to_string(),
                from_state_id: 0,
                transition_index: 9,
                to_state_id: 1,
            },
        ],
    };

    let report = graph.apply_patches(&[patch]);

    assert_eq!(graph, before);
    assert!(report.applied.is_empty());
    assert!(matches!(
        report.diagnostics.as_slice(),
        [BehaviorPatchDiagnostic::Rejected { patch, reason }]
            if patch == "atomic-transition" && reason == "unsupported: transition index is out of range"
    ));
}

#[test]
fn edt_21_reapplying_same_named_patch_is_idempotent() {
    let patch = transition_patch("one", 0);
    let mut graph = graph();

    let first = graph.apply_patches(std::slice::from_ref(&patch));
    let after_first = graph.clone();
    let second = graph.apply_patches(&[patch]);

    assert_eq!(graph, after_first);
    assert_eq!(first, second);
}
