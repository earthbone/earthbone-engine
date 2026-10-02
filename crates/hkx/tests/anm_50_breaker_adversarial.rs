// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::behavior::{BehaviorGraph, Condition, NodeKind, VariableInfo, VariableType};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::Packfile;

const TROLL_GUARD: &str =
    "(staggerDirection < .25) || (staggerDirection > .75) || (staggerMagnitude > .3 , < .7)";

fn guarded(source: &str) -> BehaviorGraph {
    let bytes = common::graph(&common::Root::Machine, "idle", "stagger");
    let mut graph =
        BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    graph.data.variable_names = vec!["staggerDirection".into(), "staggerMagnitude".into()];
    graph.data.variable_infos = vec![
        VariableInfo {
            ty: VariableType::Real,
            ..Default::default()
        };
        2
    ];
    let NodeKind::StateMachine(machine) = &mut graph.nodes[graph.root.unwrap().0].kind else {
        panic!("fixture root is not a state machine");
    };
    let mut back = machine.states[0].transitions[0].clone();
    machine.states[0].transitions[0].condition = Some(Condition::Expression(source.into()));
    machine.states[0].transitions[0].event_id = 1;
    machine.states[0].transitions[0].transition = None;
    back.condition = None;
    back.event_id = 2;
    back.to_state_id = 0;
    back.transition = None;
    machine.states[1].transitions.push(back);
    graph
}

fn current(vm: &Interpreter<'_>) -> i32 {
    vm.current_state(vm.active_states()[0].0).unwrap()
}

#[test]
fn anm_50_breaker_troll_guard_reevaluates_both_bounds_on_repeated_events() {
    let graph = guarded(TROLL_GUARD);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, Vec::new());
    vm.variables.set_by_name("staggerDirection", 0.25);

    for (event, magnitude, expected) in [
        (2, 0.4, 0),
        (1, 0.3, 0),
        (1, 0.3001, 1),
        (2, 0.3001, 0),
        (1, 0.6999, 1),
        (2, 0.6999, 0),
        (1, 0.7, 0),
    ] {
        vm.variables.set_by_name("staggerMagnitude", magnitude);
        vm.send_event(event);
        vm.update(0.0);
        assert_eq!(current(&vm), expected, "event={event}, magnitude={magnitude}");
    }
    assert!(vm.unsupported().conditions.is_empty());
}

#[test]
fn anm_50_breaker_malformed_trailing_comma_keeps_the_guard_closed() {
    let graph = guarded("staggerMagnitude > .3 ,");
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, Vec::new());
    vm.variables.set_by_name("staggerDirection", 0.25);
    vm.variables.set_by_name("staggerMagnitude", 0.5);
    for _ in 0..3 {
        vm.send_event(1);
        vm.update(0.0);
    }
    assert_eq!(current(&vm), 0);
    assert_eq!(vm.unsupported().conditions.len(), 1);
}

#[test]
fn anm_50_breaker_installed_guard_is_reached_from_its_authored_start_state() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping installed troll behavior check");
        return;
    };
    let archive = soe_bsa::Archive::open(
        &std::path::Path::new(&data).join("Skyrim - Animations.bsa"),
    )
    .expect("animation archive opens");
    let bytes = archive
        .read("meshes/actors/troll/behaviors/trollbehavior.hkx")
        .expect("troll behavior is present");
    let packfile = Packfile::parse(&bytes).expect("troll behavior parses");
    let graph = BehaviorGraph::from_packfile(&packfile).expect("troll graph decodes");
    let mut matches = Vec::new();
    for node in &graph.nodes {
        let NodeKind::StateMachine(machine) = &node.kind else {
            continue;
        };
        for state in &machine.states {
            for transition in &state.transitions {
                if transition.condition.as_ref().is_some_and(
                    |condition| matches!(condition, Condition::Expression(text) if text == TROLL_GUARD),
                ) {
                    matches.push((
                        node.id,
                        state.state_id,
                        machine.start_state_id,
                        transition.event_id,
                        transition.to_state_id,
                    ));
                }
            }
        }
    }
    assert_eq!(matches.len(), 1, "authored troll guard should be unique");
    let (machine, source_state, start_state, event, target_state) = matches[0];
    assert_eq!(source_state, start_state, "the guard must be reachable initially");

    for (magnitude, expected_state) in [
        (0.3, start_state),
        (0.3001, target_state),
        (0.6999, target_state),
        (0.7, start_state),
    ] {
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.variables.set_by_name("staggerDirection", 0.25);
        vm.variables.set_by_name("staggerMagnitude", magnitude);
        vm.send_event(event);
        vm.update(0.0);
        assert_eq!(
            vm.current_state(machine),
            Some(expected_state),
            "magnitude={magnitude}"
        );
    }
}
