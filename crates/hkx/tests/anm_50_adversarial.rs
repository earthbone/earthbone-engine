// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::behavior::{BehaviorGraph, Condition, NodeKind, VariableInfo, VariableType};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::Packfile;

fn guarded(source: &str) -> soe_hkx::behavior::BehaviorGraph {
    let bytes = common::graph(&common::Root::Machine, "idle", "stagger");
    let mut graph =
        soe_hkx::behavior::BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
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
    machine.states[0].transitions[0].condition = Some(Condition::Expression(source.into()));
    machine.states[0].transitions[0].event_id = 1;
    machine.states[0].transitions[0].transition = None;
    graph
}

fn current(vm: &Interpreter<'_>) -> i32 {
    vm.current_state(vm.active_states()[0].0).unwrap()
}

#[test]
fn anm_50_shipped_troll_guard_respects_strict_magnitude_and_direction_bounds() {
    let source =
        "(staggerDirection < .25) || (staggerDirection > .75) || (staggerMagnitude > .3 , < .7)";
    let graph = guarded(source);
    let clips = ClipLibrary::new();
    for (direction, magnitude, expected) in [
        (0.25, 0.2, 0),
        (0.2499, 0.2, 1),
        (0.75, 0.2, 0),
        (0.7501, 0.2, 1),
        (0.5, 0.3, 0),
        (0.5, 0.3001, 1),
        (0.5, 0.6999, 1),
        (0.5, 0.7, 0),
    ] {
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.variables.set_by_name("staggerDirection", direction);
        vm.variables.set_by_name("staggerMagnitude", magnitude);
        vm.send_event(1);
        vm.update(0.0);
        assert_eq!(
            current(&vm),
            expected,
            "direction={direction}, magnitude={magnitude}"
        );
        assert!(vm.unsupported().conditions.is_empty());
    }
}

#[test]
fn anm_50_comma_comparison_can_be_nested_and_or_composed_without_widening_bounds() {
    let graph = guarded(
        "(staggerMagnitude > .3 , <= .7) && (staggerDirection < .25 || staggerDirection > .75)",
    );
    let clips = ClipLibrary::new();
    for (direction, magnitude, expected) in [
        (0.1, 0.7, 1),
        (0.1, 0.7001, 0),
        (0.25, 0.5, 0),
        (0.9, 0.3, 0),
    ] {
        let mut vm = Interpreter::new(&graph, &clips, Vec::new());
        vm.variables.set_by_name("staggerDirection", direction);
        vm.variables.set_by_name("staggerMagnitude", magnitude);
        vm.send_event(1);
        vm.update(0.0);
        assert_eq!(current(&vm), expected);
        assert!(vm.unsupported().conditions.is_empty());
    }
}

#[test]
fn anm_50_malformed_comma_comparison_is_reported_and_does_not_transition() {
    let graph = guarded("Magnitude > .3 , nonsense .7");
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, Vec::new());
    vm.send_event(1);
    vm.update(0.0);
    assert_eq!(current(&vm), 0);
    assert_eq!(vm.unsupported().conditions.len(), 1);
}

#[test]
fn anm_50_installed_troll_runtime_trace_covers_both_magnitude_bounds() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping installed troll behavior check");
        return;
    };
    let archive =
        soe_bsa::Archive::open(&std::path::Path::new(&data).join("Skyrim - Animations.bsa"))
            .expect("animation archive opens");
    let bytes = archive
        .read("meshes/actors/troll/behaviors/trollbehavior.hkx")
        .expect("troll behavior is present");
    let packfile = Packfile::parse(&bytes).expect("troll behavior parses");
    let installed = BehaviorGraph::from_packfile(&packfile).expect("troll graph decodes");
    let source =
        "(staggerDirection < .25) || (staggerDirection > .75) || (staggerMagnitude > .3 , < .7)";
    let mut target = None;
    for node in &installed.nodes {
        let NodeKind::StateMachine(machine) = &node.kind else {
            continue;
        };
        for state in &machine.states {
            for transition in &state.transitions {
                if transition.condition.as_ref().is_some_and(
                    |condition| matches!(condition, Condition::Expression(text) if text == source),
                ) {
                    target = Some((
                        node.id,
                        machine.start_state_id,
                        transition.event_id,
                        transition.to_state_id,
                    ));
                }
            }
        }
    }
    let (machine, start_state, event, target_state) =
        target.expect("authored troll guard transition found");
    let clips = ClipLibrary::new();
    let mut trace = Vec::new();
    for (magnitude, expected_state) in [
        (0.3, start_state),
        (0.3001, target_state),
        (0.6999, target_state),
        (0.7, start_state),
    ] {
        let mut vm = Interpreter::new(&installed, &clips, Vec::new());
        // Keep the other direction and magnitude guards on this event false.
        vm.variables.set_by_name("staggerDirection", 0.25);
        vm.variables.set_by_name("staggerMagnitude", magnitude);
        vm.send_event(event);
        vm.update(0.0);
        let state = vm.current_state(machine).expect("troll machine is active");
        trace.push((magnitude, state));
        assert_eq!(state, expected_state, "magnitude={magnitude}");
    }
    assert_eq!(
        trace,
        [
            (0.3, start_state),
            (0.3001, target_state),
            (0.6999, target_state),
            (0.7, start_state)
        ]
    );
}
