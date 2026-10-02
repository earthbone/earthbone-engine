// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! ANM-56 records current interpreter choices, not captured Skyrim semantics.
//! Event replay uses the installed 1HM_Behavior machine and TOOL-04 protocol.

mod common;

use std::{collections::BTreeMap, path::Path};

use soe_hkx::behavior::{
    transition_flags as flags, BehaviorGraph, Condition, NodeKind, StateMachine, TimeInterval,
};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::Packfile;
use soe_probe::{compare, parse_log, Observation, Probe, Step};

const SEQUENCE: &str = include_str!("../../probe/probes/anm_56.toml");
const EXPECTED: &str = include_str!("../../probe/probes/anm_56.expected.json");

fn machine(graph: &mut BehaviorGraph) -> &mut StateMachine {
    let root = graph.root.unwrap();
    let NodeKind::StateMachine(sm) = &mut graph.nodes[root.0].kind else {
        panic!("state machine fixture")
    };
    sm
}

fn fixture() -> BehaviorGraph {
    let bytes = common::graph(&common::Root::Machine, "a", "b");
    let mut graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    let sm = machine(&mut graph);
    sm.wildcard_transitions.clear();
    sm.return_to_previous_state_event_id = -1;
    sm.event_to_send_when_state_or_transition_changes.id = -1;
    for state in &mut sm.states {
        state.enter_notify_events.clear();
        state.exit_notify_events.clear();
        state.generator = None;
    }
    sm.states[1].transitions.clear();
    let t = &mut sm.states[0].transitions[0];
    t.event_id = 10;
    t.transition = None;
    t.condition = None;
    t.flags = 0;
    graph
}

fn time(enter: f32, exit: f32) -> TimeInterval {
    TimeInterval {
        enter_event_id: -1,
        exit_event_id: -1,
        enter_time: enter,
        exit_time: exit,
    }
}

fn events(enter: i32, exit: i32) -> TimeInterval {
    TimeInterval {
        enter_event_id: enter,
        exit_event_id: exit,
        enter_time: f32::NAN,
        exit_time: -1.0,
    }
}

#[test]
fn anm_56_trigger_time_edges_and_zero_exit() {
    for (exit, at, expected) in [
        (2.0, 0.5, 0),
        (2.0, 1.0, 1),
        (2.0, 1.5, 1),
        (2.0, 2.0, 0),
        (2.0, 2.5, 0),
        (0.0, 60.0, 1),
    ] {
        let mut graph = fixture();
        let t = &mut machine(&mut graph).states[0].transitions[0];
        t.flags = flags::USE_TRIGGER_INTERVAL;
        t.trigger_interval = time(1.0, exit);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, vec![]);
        vm.update(at);
        vm.send_event(10);
        vm.update(0.0);
        assert_eq!(
            vm.current_state(graph.root.unwrap()),
            Some(expected),
            "receipt at {at}"
        );
        if expected == 0 {
            vm.update(60.0);
            vm.update(0.0);
            assert_eq!(
                vm.current_state(graph.root.unwrap()),
                Some(0),
                "rejected request is discarded"
            );
        }
    }
}

#[test]
fn anm_56_initiate_time_edges_and_missed_window() {
    for (at, expected) in [(0.5, 0), (1.0, 1), (1.5, 1), (2.0, 0), (2.5, 0)] {
        let mut graph = fixture();
        let t = &mut machine(&mut graph).states[0].transitions[0];
        t.flags = flags::USE_INITIATE_INTERVAL;
        t.initiate_interval = time(1.0, 2.0);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, vec![]);
        vm.update(at);
        vm.send_event(10);
        vm.update(0.0);
        assert_eq!(
            vm.current_state(graph.root.unwrap()),
            Some(expected),
            "receipt at {at}"
        );
        if at > 2.0 {
            vm.update(60.0);
            vm.update(0.0);
            assert_eq!(
                vm.current_state(graph.root.unwrap()),
                Some(0),
                "closed window has no timeout fallback"
            );
        }
    }
    let mut graph = fixture();
    let t = &mut machine(&mut graph).states[0].transitions[0];
    t.flags = flags::USE_INITIATE_INTERVAL;
    t.initiate_interval = time(1.0, 2.0);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.send_event(10);
    vm.update(2.0);
    vm.update(0.0);
    assert_eq!(
        vm.current_state(graph.root.unwrap()),
        Some(0),
        "one update skips the complete window"
    );
}

#[test]
fn anm_56_event_edges_override_time_fields() {
    for (bounds, input, expected) in [
        (events(20, 21), vec![10], 0),
        (events(20, 21), vec![20, 10], 1),
        (events(20, 21), vec![20, 21, 10], 0),
        (events(10, 21), vec![10], 1),
        (events(-1, 10), vec![10], 0),
        (events(10, 10), vec![10], 0),
    ] {
        let mut graph = fixture();
        let t = &mut machine(&mut graph).states[0].transitions[0];
        t.flags = flags::USE_TRIGGER_INTERVAL;
        t.trigger_interval = bounds;
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, vec![]);
        for id in input {
            vm.send_event(id);
        }
        vm.update(0.0);
        assert_eq!(vm.current_state(graph.root.unwrap()), Some(expected));
    }
}

#[test]
fn anm_56_delayed_timeout_and_condition_recheck_baseline() {
    let mut graph = fixture();
    let t = &mut machine(&mut graph).states[0].transitions[0];
    t.flags = flags::USE_INITIATE_INTERVAL;
    t.initiate_interval = events(20, 21);
    t.condition = Some(Condition::Expression("Speed > 0".into()));
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    let speed = graph.data.variable_index("Speed").unwrap() as usize;
    vm.variables.set_real(speed, 1.0);
    vm.send_event(10);
    vm.update(0.0);
    vm.variables.set_real(speed, 0.0);
    for dt in [1.0, 4.0, 25.0, 30.0] {
        vm.update(dt);
        assert_eq!(vm.current_state(graph.root.unwrap()), Some(0));
    }
    vm.send_event(20);
    vm.update(0.0);
    assert_eq!(
        vm.current_state(graph.root.unwrap()),
        Some(1),
        "admission condition is latched for 60 seconds"
    );
    assert_eq!(vm.changes().last().unwrap().event, Some(10));
}

#[test]
fn anm_56_state_reentry_cancels_delayed_request() {
    let mut graph = fixture();
    let sm = machine(&mut graph);
    let mut reset = sm.states[0].transitions[0].clone();
    reset.event_id = 11;
    reset.to_state_id = 0;
    sm.states[0].transitions.push(reset);
    let t = &mut sm.states[0].transitions[0];
    t.flags = flags::USE_INITIATE_INTERVAL;
    t.initiate_interval = events(20, 21);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    for id in [10, 11, 20] {
        vm.send_event(id);
    }
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(0));
    vm.send_event(10);
    vm.update(0.0);
    assert_eq!(
        vm.current_state(graph.root.unwrap()),
        Some(1),
        "fresh request uses the newly opened interval"
    );
}

#[test]
fn anm_56_deactivation_cancels_delayed_request() {
    let mut graph = fixture();
    let inner = graph.root.unwrap();
    let mut parent = graph.nodes[inner.0].clone();
    let NodeKind::StateMachine(outer) = &mut parent.kind else {
        panic!("machine fixture")
    };
    outer.states[0].generator = Some(inner);
    outer.states[0].transitions[0].event_id = 12;
    let mut back = outer.states[0].transitions[0].clone();
    back.event_id = 13;
    back.to_state_id = 0;
    outer.states[1].transitions.push(back);
    let t = &mut machine(&mut graph).states[0].transitions[0];
    t.flags = flags::USE_INITIATE_INTERVAL;
    t.initiate_interval = events(20, 21);
    parent.id = soe_hkx::behavior::NodeId(graph.nodes.len());
    graph.root = Some(parent.id);
    graph.nodes.push(parent);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    for id in [10, 12] {
        vm.send_event(id);
        vm.update(0.0);
    }
    assert!(!vm.is_active(inner));
    for id in [13, 20] {
        vm.send_event(id);
    }
    vm.update(0.0);
    assert_eq!(vm.current_state(inner), Some(0));
}

#[test]
fn anm_56_installed_interval_modes_have_no_finite_time_windows() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping ANM-56 interval survey");
        return;
    };
    let archive =
        soe_bsa::Archive::open(&Path::new(&data).join("Skyrim - Animations.bsa")).unwrap();
    let mut graphs = 0;
    let mut intervals = 0;
    for path in archive.files().iter().filter(|path| path.ends_with(".hkx")) {
        let bytes = archive.read(path).unwrap();
        let pf = Packfile::parse(&bytes).unwrap();
        if pf.objects_of("hkbBehaviorGraph").is_empty() {
            continue;
        }
        let graph = BehaviorGraph::from_packfile(&pf).unwrap();
        graphs += 1;
        for node in &graph.nodes {
            let NodeKind::StateMachine(sm) = &node.kind else {
                continue;
            };
            for t in sm
                .states
                .iter()
                .flat_map(|s| &s.transitions)
                .chain(&sm.wildcard_transitions)
            {
                for (flag, bounds) in [
                    (flags::USE_TRIGGER_INTERVAL, t.trigger_interval),
                    (flags::USE_INITIATE_INTERVAL, t.initiate_interval),
                ] {
                    if t.flags & flag == 0 {
                        continue;
                    }
                    intervals += 1;
                    if bounds.enter_event_id < 0 && bounds.exit_event_id < 0 {
                        assert_eq!(
                            (bounds.enter_time, bounds.exit_time),
                            (0.0, 0.0),
                            "{path}: {} finite window needs a capture case",
                            node.name
                        );
                    }
                }
            }
        }
    }
    assert!(graphs >= 428, "vanilla survey visited {graphs} graphs");
    assert!(intervals > 0, "survey found no enabled intervals");
    println!("{graphs} graphs, {intervals} enabled intervals, no finite time windows");
}

fn vanilla_graph(data: &Path) -> BehaviorGraph {
    let archive = soe_bsa::Archive::open(&data.join("Skyrim - Animations.bsa")).unwrap();
    let bytes = archive
        .read("meshes/actors/character/behaviors/1hm_behavior.hkx")
        .unwrap();
    let mut graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    graph.root = Some(graph.find_by_name("1HM_Behavior").unwrap().id);
    graph
}

fn form_identities(probe: &Probe, data: &Path) -> BTreeMap<String, i32> {
    let mut identity_probe = probe.clone();
    identity_probe.startup_wait_seconds = 0.0;
    identity_probe.steps = vec![Step::QuestStage {
        id: "identity".into(),
        quest: "runner".into(),
    }];
    soe_probe::run_installed(&identity_probe, data)
        .unwrap()
        .remove(0)
        .forms
}

fn replay(probe: &Probe, graph: &BehaviorGraph, forms: &BTreeMap<String, i32>) -> Vec<Observation> {
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(graph, &clips, vec![]);
    for (name, value) in [
        ("IsNPC", 0),
        ("iRightHandType", 1),
        ("iLeftHandType", 0),
        ("iWantBlock", 0),
        ("i1stPerson", 0),
    ] {
        let index = graph.data.variable_index(name).unwrap() as usize;
        vm.variables.set_word(index, value);
    }
    vm.update(0.0);
    let mut elapsed = 0.0;
    let mut observations = Vec::new();
    let mut emit = |id: &str, phase: &str, result: BTreeMap<String, f64>, elapsed: f64| {
        observations.push(Observation {
            schema: 1,
            sequence: probe.name.clone(),
            request_id: id.into(),
            phase: phase.into(),
            runtime_version: format!("soe-hkx/{}", env!("CARGO_PKG_VERSION")),
            skse_version: None,
            load_order: probe.load_order.clone(),
            forms: forms.clone(),
            timescale: f64::from(probe.timescale),
            real_seconds: elapsed,
            game_seconds: elapsed * f64::from(probe.timescale),
            paused: 0,
            result,
        });
    };
    for step in &probe.steps {
        let value = match step {
            Step::WaitReal { seconds, .. } => {
                emit(step.id(), "begin", BTreeMap::new(), elapsed);
                let ticks = (f64::from(*seconds) / probe.tick_seconds).round() as usize;
                for _ in 0..ticks {
                    vm.update(probe.tick_seconds as f32);
                }
                elapsed += f64::from(*seconds);
                ("issued", 1.0)
            }
            Step::AnimationEvents {
                events, set_int, ..
            } => {
                for (name, value) in set_int {
                    let index = graph.data.variable_index(name).unwrap() as usize;
                    assert!(vm.variables.set_word(index, *value));
                }
                for event in events {
                    if step.id() == "draw_sword" {
                        assert_eq!(event, "WeapEquip");
                        continue;
                    }
                    let index = graph.data.event_id(event).unwrap();
                    vm.send_event(index);
                }
                vm.update(0.0);
                ("issued", 1.0)
            }
            Step::AnimationInt { variable, set, .. } => {
                let index = graph.data.variable_index(variable).unwrap() as usize;
                if let Some(value) = set {
                    vm.variables.set_word(index, *value);
                }
                let value = if variable == "i1HMState" {
                    vm.current_state(graph.root.unwrap()).unwrap()
                } else {
                    vm.variables.word(index).unwrap()
                };
                ("value", f64::from(value))
            }
            Step::AnimationFloat { variable, set, .. } => {
                let index = graph.data.variable_index(variable).unwrap() as usize;
                if let Some(value) = set {
                    vm.variables.set_real(index, *value);
                }
                ("value", f64::from(vm.variables.real(index).unwrap()))
            }
            Step::EquipItem { .. } | Step::ThirdPerson { .. } => ("issued", 1.0),
            _ => panic!("ANM-56 replay does not handle {step:?}"),
        };
        emit(
            step.id(),
            "end",
            BTreeMap::from([(value.0.into(), value.1)]),
            elapsed,
        );
    }
    observations
}

#[test]
fn anm_56_installed_sequence_replays_expected_cases() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping ANM-56 installed replay");
        return;
    };
    let probe = Probe::parse(SEQUENCE).unwrap();
    let actual = replay(
        &probe,
        &vanilla_graph(Path::new(&data)),
        &form_identities(&probe, Path::new(&data)),
    );
    let expected: Vec<Observation> = serde_json::from_str(EXPECTED).unwrap();
    assert_eq!(
        actual, expected,
        "engine replay drifted from the reviewed expectations"
    );
    for (id, state) in [
        ("ready_precondition", 0.0),
        ("trigger_before_enter_state", 10.0),
        ("trigger_at_enter_state", 1.0),
        ("trigger_at_exit_state", 10.0),
        ("timeout_1_state", 1.0),
        ("timeout_5_state", 1.0),
        ("timeout_30_state", 1.0),
        ("timeout_60_state", 1.0),
        ("initiate_at_enter_state", 0.0),
        ("reset_cancels_pending_state", 0.0),
        ("recheck_latched_state", 0.0),
        ("same_event_bounds_state", 1.0),
        ("finished_state", 0.0),
    ] {
        let observation = actual
            .iter()
            .find(|o| o.request_id == id && o.phase == "end")
            .unwrap();
        assert_eq!(observation.result["value"], state, "{id}");
    }
}

#[test]
fn anm_56_original_capture_matches_hkx_replay() {
    let Some(capture) = std::env::var_os("SOE_PROBE_CAPTURE") else {
        eprintln!("SOE_PROBE_CAPTURE not set, skipping ANM-56 original-runtime comparison");
        return;
    };
    let data = std::env::var_os("SKYRIM_DATA").expect("ANM-56 capture replay needs SKYRIM_DATA");
    let probe = Probe::parse(SEQUENCE).unwrap();
    let expected: Vec<Observation> = serde_json::from_str(EXPECTED).unwrap();
    let actual = replay(
        &probe,
        &vanilla_graph(Path::new(&data)),
        &form_identities(&probe, Path::new(&data)),
    );
    assert_eq!(
        actual, expected,
        "review engine expectations before capturing"
    );
    let log = std::fs::read_to_string(Path::new(&capture).join("Papyrus.0.log")).unwrap();
    let observed = parse_log(&log).unwrap();
    let report = compare(&probe, &actual, &observed).unwrap();
    assert!(
        report.matched,
        "{}",
        serde_json::to_string_pretty(&report).unwrap()
    );
}
