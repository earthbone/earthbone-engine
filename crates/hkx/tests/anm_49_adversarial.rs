// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::behavior::{Condition, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::Packfile;

fn guarded(source: &str) -> soe_hkx::behavior::BehaviorGraph {
    let bytes = common::graph(&common::Root::Machine, "idle", "walk");
    let mut graph = soe_hkx::behavior::BehaviorGraph::from_packfile(
        &Packfile::parse(&bytes).unwrap(),
    )
    .unwrap();
    let NodeKind::StateMachine(machine) = &mut graph.nodes[graph.root.unwrap().0].kind else {
        panic!("fixture root is not a state machine");
    };
    machine.states[0].transitions[0].condition = Some(Condition::Expression(source.into()));
    machine.states[0].transitions[0].event_id = 1;
    machine.states[0].transitions[0].transition = None;
    graph
}

fn current(vm: &Interpreter<'_>) -> i32 {
    vm.current_state(vm.active_states()[0].0).unwrap()
}

#[test]
fn scalar_function_guards_cover_signed_modulo_clamp_edges_and_angle_wrap() {
    let clips = ClipLibrary::new();
    for (source, expected) in [
        ("mod(-5, 2) == -1", 1),
        ("clamp(3, 0, 3) == 3", 1),
        ("clerp(3, -3, .5) > 3.14", 1),
        ("atan2approxd(1, 0) == 90", 1),
    ] {
        let graph = guarded(source);
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.send_event(1);
        vm.update(0.0);
        assert_eq!(current(&vm), expected, "{source}");
        assert!(vm.unsupported().conditions.is_empty(), "{source}");
    }
}

#[test]
fn non_finite_function_result_blocks_guard_without_mutating_state() {
    let graph = guarded("asin(2) == 0");
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, Vec::new());
    for _ in 0..3 {
        vm.send_event(1);
        vm.update(0.0);
    }
    assert_eq!(current(&vm), 0);
    assert_eq!(vm.unsupported().conditions.len(), 1);
}

#[test]
fn random_guard_sequence_replays_across_interpreters_and_repeated_updates() {
    let graph = guarded("randrange(-2, 3) >= 0");
    let clips = ClipLibrary::new();
    let mut first = Interpreter::new(&graph, &clips, Vec::new());
    let mut replay = Interpreter::new(&graph, &clips, Vec::new());
    for _ in 0..16 {
        first.send_event(1);
        replay.send_event(1);
        first.update(0.0);
        replay.update(0.0);
        assert_eq!(current(&first), current(&replay));
    }
    assert!(first.unsupported().conditions.is_empty());
    assert!(replay.unsupported().conditions.is_empty());
}

#[test]
fn expression_guard_rejects_unknown_function_without_transition() {
    let graph = guarded("havok_missing(1) == 1");
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, Vec::new());
    vm.send_event(1);
    vm.update(0.0);
    assert_eq!(current(&vm), 0);
    assert_eq!(vm.unsupported().conditions.len(), 1);
    assert_eq!(vm.active_states()[0].0, NodeId(graph.root.unwrap().0));
}
