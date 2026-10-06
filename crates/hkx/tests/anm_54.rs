// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Provisional transition ties, following the ANM-54 vanilla probe inputs.

mod common;

use soe_hkx::behavior::{
    transition_flags as flags, BehaviorGraph, NodeId, NodeKind, StateMachine, TimeInterval,
};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::Packfile;

fn machine(graph: &mut BehaviorGraph) -> &mut StateMachine {
    let root = graph.root.unwrap();
    let NodeKind::StateMachine(machine) = &mut graph.nodes[root.0].kind else {
        panic!("machine");
    };
    machine
}

fn fixture() -> BehaviorGraph {
    let mut graph = BehaviorGraph::from_packfile(
        &Packfile::parse(&common::graph(&common::Root::Machine, "a", "b")).unwrap(),
    )
    .unwrap();
    let b = graph.find_by_name("WalkClip").unwrap().id;
    let mut c = graph.nodes[b.0].clone();
    c.id = NodeId(graph.nodes.len());
    c.bindings = None;
    let c_id = c.id;
    graph.nodes.push(c);
    for node in &mut graph.nodes {
        match &mut node.kind {
            NodeKind::Clip(clip) => clip.triggers.clear(),
            NodeKind::BlendingTransition(effect) => {
                effect.duration = 1.0;
                effect.event_mode = 1;
            }
            _ => {}
        }
    }
    let sm = machine(&mut graph);
    sm.max_simultaneous_transitions = 3;
    sm.wildcard_transitions.clear();
    sm.event_to_send_when_state_or_transition_changes.id = -1;
    sm.return_to_previous_state_event_id = -1;
    let mut third = sm.states[1].clone();
    third.state_id = 2;
    third.generator = Some(c_id);
    third.transitions.clear();
    sm.states.push(third);
    for state in &mut sm.states {
        state.enter_notify_events.clear();
        state.exit_notify_events.clear();
    }
    sm.states[1].transitions.clear();
    let t = &mut sm.states[0].transitions[0];
    t.event_id = 10;
    t.priority = 0;
    t.flags = 0;
    t.condition = None;
    graph
}

#[test]
fn anm_54_local_candidate_precedes_equal_priority_wildcard() {
    let mut graph = fixture();
    let sm = machine(&mut graph);
    let mut wildcard = sm.states[0].transitions[0].clone();
    wildcard.to_state_id = 2;
    wildcard.flags = flags::IS_LOCAL_WILDCARD;
    sm.wildcard_transitions.push(wildcard);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.send_event(10);
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(1));
}

#[test]
fn anm_54_equal_priority_request_interrupts_active_blend() {
    let mut graph = fixture();
    let sm = machine(&mut graph);
    let mut next = sm.states[0].transitions[0].clone();
    next.event_id = 11;
    next.to_state_id = 2;
    sm.states[1].transitions.push(next);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.send_event(10);
    vm.update(0.25);
    assert_eq!(vm.active_transition_count(graph.root.unwrap()), 1);
    vm.send_event(11);
    vm.update(0.25);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(2));
    assert_eq!(vm.active_transition_count(graph.root.unwrap()), 2);
}

#[test]
fn anm_54_equal_priority_delayed_requests_keep_last_in_both_orders() {
    for (events, target) in [([10, 11], 2), ([11, 10], 1)] {
        let mut graph = fixture();
        let sm = machine(&mut graph);
        let first = &mut sm.states[0].transitions[0];
        first.transition = None;
        first.flags = flags::USE_INITIATE_INTERVAL;
        first.initiate_interval = TimeInterval {
            enter_event_id: 20,
            exit_event_id: 21,
            enter_time: 0.0,
            exit_time: 0.0,
        };
        let mut second = first.clone();
        second.event_id = 11;
        second.to_state_id = 2;
        sm.states[0].transitions.push(second);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, vec![]);
        vm.send_event(21);
        for event in events {
            vm.send_event(event);
        }
        vm.update(0.0);
        assert_eq!(vm.current_state(graph.root.unwrap()), Some(0));
        vm.send_event(20);
        vm.update(0.0);
        assert_eq!(
            vm.current_state(graph.root.unwrap()),
            Some(target),
            "order {events:?}"
        );
    }
}

#[test]
fn anm_54_same_event_delayed_candidates_retain_authored_first() {
    let mut graph = fixture();
    let sm = machine(&mut graph);
    let first = &mut sm.states[0].transitions[0];
    first.transition = None;
    first.flags = flags::USE_INITIATE_INTERVAL;
    first.initiate_interval = TimeInterval {
        enter_event_id: 20,
        exit_event_id: -1,
        enter_time: 0.0,
        exit_time: 0.0,
    };
    let mut second = first.clone();
    second.to_state_id = 2;
    second.initiate_interval.enter_event_id = 21;
    sm.states[0].transitions.push(second);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.send_event(10);
    vm.update(0.0);
    vm.send_event(21);
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(0));
    vm.send_event(20);
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(1));
}

#[test]
fn anm_54_current_state_output_binding_publishes_transition_destination() {
    use soe_hkx::behavior::{Binding, VariableBindingSet};
    let mut graph = fixture();
    let root = graph.root.unwrap();
    graph.nodes[root.0].bindings = Some(VariableBindingSet {
        bindings: vec![Binding {
            member_path: "currentStateId".into(),
            variable_index: 1,
            bit_index: -1,
            binding_type: 0,
        }],
        index_of_binding_to_enable: -1,
    });
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.variables.set_word(1, 99);
    vm.update(0.0);
    assert_eq!(vm.variables.word(1), Some(0));
    vm.send_event(10);
    vm.update(0.25);
    assert!(vm.in_transition(root));
    assert_eq!(vm.variables.word(1), Some(1));
}

const SEQUENCE: &str = include_str!("../../probe/probes/anm_54.toml");
const EXPECTED: &str = include_str!("../../probe/probes/anm_54.expected.json");

#[test]
fn anm_54_installed_replay_matches_reviewed_predictions() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping ANM-54 installed replay");
        return;
    };
    let probe = soe_probe::Probe::parse(SEQUENCE).unwrap();
    let expected: Vec<soe_probe::Observation> = serde_json::from_str(EXPECTED).unwrap();
    let replay = soe_probe::run_installed(&probe, std::path::Path::new(&data)).unwrap();
    let comparison = soe_probe::compare(&probe, &expected, &replay).unwrap();
    assert!(comparison.matched, "{:?}", comparison.differences);
    let values: Vec<_> = replay
        .iter()
        .filter_map(|observation| {
            observation
                .result
                .get("value")
                .map(|value| (observation.request_id.as_str(), *value))
        })
        .collect();
    assert_eq!(
        values,
        vec![
            ("ready_state", 0.0),
            ("local_wildcard_result", 10.0),
            ("local_reset_state", 0.0),
            ("equal_priority_result", 8.0),
            ("interrupt_reset_state", 0.0),
            ("delayed_last_attack_pending", 1.0),
            ("delayed_last_attack_result", 10.0),
            ("delayed_last_attack_reset_state", 0.0),
            ("delayed_last_stop_pending", 1.0),
            ("delayed_last_stop_result", 0.0),
        ]
    );
}

#[test]
fn anm_54_original_runtime_capture_matches_hkx_replay() {
    let Some(capture) = std::env::var_os("SOE_PROBE_CAPTURE") else {
        eprintln!("SOE_PROBE_CAPTURE not set, skipping vanilla comparison");
        return;
    };
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping ANM-54 installed replay");
        return;
    };
    let probe = soe_probe::Probe::parse(SEQUENCE).unwrap();
    let log =
        std::fs::read_to_string(std::path::PathBuf::from(capture).join("Papyrus.0.log")).unwrap();
    let observed = soe_probe::parse_log(&log).unwrap();
    let replay = soe_probe::run_installed(&probe, std::path::Path::new(&data)).unwrap();
    let comparison = soe_probe::compare(&probe, &replay, &observed).unwrap();
    assert!(comparison.matched, "{:?}", comparison.differences);
}
