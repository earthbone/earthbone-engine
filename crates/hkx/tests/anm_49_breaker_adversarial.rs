// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::behavior::{Condition, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::Packfile;

fn guarded(source: &str) -> soe_hkx::behavior::BehaviorGraph {
    let bytes = common::graph(&common::Root::Machine, "idle", "walk");
    let mut graph =
        soe_hkx::behavior::BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    let NodeKind::StateMachine(machine) = &mut graph.nodes[graph.root.unwrap().0].kind else {
        panic!("fixture root is not a state machine");
    };
    machine.states[0].transitions[0].condition = Some(Condition::Expression(source.into()));
    machine.states[0].transitions[0].event_id = 1;
    machine.states[0].transitions[0].transition = None;
    graph
}

fn evaluate_guard(source: &str) -> (i32, usize) {
    let graph = guarded(source);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, Vec::new());
    vm.send_event(1);
    vm.update(0.0);
    (
        vm.current_state(vm.active_states()[0].0).unwrap(),
        vm.unsupported().conditions.len(),
    )
}

#[test]
fn expression_operator_boundaries_keep_scalar_contracts() {
    for source in [
        "clamp(2, 2, 2) == 2",
        "sign(0) == 0",
        "mod(-5, 2) == -1",
        "atan2approxd(1, 0) == 90",
        "randrange(4, 4) == 4",
    ] {
        let (state, unsupported) = evaluate_guard(source);
        assert_eq!(state, 1, "expected guard to pass: {source}");
        assert_eq!(unsupported, 0, "unexpected unsupported guard: {source}");
    }
}

#[test]
fn undefined_real_results_reject_guards_without_transition() {
    for source in ["mod(1, 0) == 0", "sqrtinv(0) == 0", "sqrt(-1) == 0"] {
        let (state, unsupported) = evaluate_guard(source);
        assert_eq!(state, 0, "undefined scalar result must not pass: {source}");
        assert_eq!(
            unsupported, 1,
            "invalid scalar result should be reported: {source}"
        );
    }
}

#[test]
fn seeded_random_functions_stay_within_authored_bounds() {
    let (state, unsupported) = evaluate_guard(
        "rand01() >= 0 && rand01() < 1 && randrange(-2, 3) >= -2 && randrange(-2, 3) < 3",
    );
    assert_eq!(
        state, 1,
        "random outputs must stay within their stated bounds"
    );
    assert_eq!(
        unsupported, 0,
        "valid bounded random guards should be supported"
    );
}

#[test]
fn inverse_trig_clamps_both_signs_at_and_just_inside_havok_domain_limit() {
    let clips = ClipLibrary::new();
    for source in [
        "asin(1) > 1.57",
        "asin(-1) < -1.57",
        "acos(1) == 0",
        "acos(-1) > 3.14",
        "asind(1.00099) == 90",
        "asind(-1.00099) == -90",
        "acosd(1.00099) == 0",
        "acosd(-1.00099) == 180",
    ] {
        let graph = guarded(source);
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.send_event(1);
        vm.update(0.0);
        assert_eq!(
            vm.current_state(vm.active_states()[0].0).unwrap(),
            1,
            "expected the Havok inverse-trig clamp: {source}"
        );
        assert!(vm.unsupported().conditions.is_empty(), "{source}");
    }
}
