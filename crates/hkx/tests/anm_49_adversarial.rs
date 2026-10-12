// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::behavior::{Condition, NodeId, NodeKind};
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

fn current(vm: &Interpreter<'_>) -> i32 {
    vm.current_state(vm.active_states()[0].0).unwrap()
}

#[test]
fn anm_49_authored_functions_match_independent_scalar_oracle() {
    // The reference values use f64 standard math, independent of the hkReal
    // evaluator. The operator names and hkReal representation come from the
    // local hkbCompiledExpressionSetToken class definition.
    let degrees = std::f64::consts::PI / 180.0;
    let clerp_delta = (-3.0_f64 - 3.0 + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
        - std::f64::consts::PI;
    let cases = [
        ("sin(0.25)", 0.25_f64.sin()),
        ("cos(0.25)", 0.25_f64.cos()),
        ("asin(0.5)", 0.5_f64.asin()),
        ("acos(0.5)", 0.5_f64.acos()),
        ("sqrt(9)", 9.0_f64.sqrt()),
        ("fabs(-3)", 3.0),
        ("ceil(2.1)", 3.0),
        ("floor(2.9)", 2.0),
        ("sqrtinv(4)", 4.0_f64.sqrt().recip()),
        ("pow(2, 3)", 2.0_f64.powf(3.0)),
        ("max(-2, 3)", (-2.0_f64).max(3.0)),
        ("min(-2, 3)", (-2.0_f64).min(3.0)),
        ("atan2approx(1, 1)", 1.0_f64.atan2(1.0)),
        ("clamp(4, 0, 3)", 4.0_f64.clamp(0.0, 3.0)),
        ("mod(-5, 2)", -5.0_f64 % 2.0),
        ("deg2rad(180)", 180.0 * degrees),
        ("rad2deg(3.141592653589793)", std::f64::consts::PI / degrees),
        ("cosd(60)", (60.0 * degrees).cos()),
        ("sind(30)", (30.0 * degrees).sin()),
        ("acosd(0.5)", 0.5_f64.acos() / degrees),
        ("asind(0.5)", 0.5_f64.asin() / degrees),
        ("atan2approxd(1, 1)", 1.0_f64.atan2(1.0) / degrees),
        ("sign(-2)", -1.0),
        ("lerp(2, 6, 0.25)", 2.0 + (6.0 - 2.0) * 0.25),
        ("clerp(3, -3, 0.5)", 3.0 + clerp_delta * 0.5),
        ("cond(0, 2, 3)", 3.0),
    ];
    let clips = ClipLibrary::new();

    for (function, expected) in cases {
        let guard = format!(
            "({function}) > {:.12} && ({function}) < {:.12}",
            expected - 0.0001,
            expected + 0.0001
        );
        let graph = guarded(&guard);
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.send_event(1);
        vm.update(0.0);
        assert_eq!(
            current(&vm),
            1,
            "{function} should match scalar result {expected}"
        );
        assert!(vm.unsupported().conditions.is_empty(), "{function}");
    }
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
fn anm_49_inverse_trig_guards_follow_havok_domain_tolerance() {
    let clips = ClipLibrary::new();
    for source in [
        "asin(1.0005) > 1.57",
        "asin(-1.0005) < -1.57",
        "acos(1.0005) == 0",
        "acos(-1.0005) > 3.14",
        "asind(1.0005) == 90",
        "acosd(-1.0005) == 180",
    ] {
        let graph = guarded(source);
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.send_event(1);
        vm.update(0.0);
        assert_eq!(
            current(&vm),
            1,
            "expected Havok domain guard to pass: {source}"
        );
        assert!(vm.unsupported().conditions.is_empty(), "{source}");
    }
}

#[test]
fn anm_49_inverse_trig_outside_havok_domain_tolerance_rejects_guard() {
    let graph = guarded("asin(1.001) == 0");
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, Vec::new());
    vm.send_event(1);
    vm.update(0.0);
    assert_eq!(current(&vm), 0);
    assert_eq!(vm.unsupported().conditions.len(), 1);
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
