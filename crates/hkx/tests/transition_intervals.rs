// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::behavior::{
    transition_flags as flags, BehaviorGraph, Condition, NodeKind, StateMachine, TimeInterval,
};
use soe_hkx::interp::{ClipLibrary, Interpreter};
use soe_hkx::Packfile;

fn machine(graph: &mut BehaviorGraph) -> &mut StateMachine {
    let root = graph.root.unwrap();
    let NodeKind::StateMachine(sm) = &mut graph.nodes[root.0].kind else {
        panic!("state machine fixture")
    };
    sm
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
        exit_time: -9.0,
    }
}

fn fixture() -> BehaviorGraph {
    let bytes = common::graph(&common::Root::Machine, "a", "b");
    let mut graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    let sm = machine(&mut graph);
    sm.wildcard_transitions.clear();
    sm.return_to_previous_state_event_id = -1;
    for state in &mut sm.states {
        state.enter_notify_events.clear();
        state.exit_notify_events.clear();
    }
    sm.event_to_send_when_state_or_transition_changes.id = -1;
    sm.states[1].transitions.clear();
    let t = &mut sm.states[0].transitions[0];
    t.event_id = 10;
    t.transition = None;
    t.condition = None;
    t.flags = 0;
    t.priority = 1;
    for node in &mut graph.nodes {
        if let NodeKind::Clip(clip) = &mut node.kind {
            clip.triggers.clear();
        }
    }
    graph
}

#[test]
fn anm_11_intervals_trigger_bounds_gate_receipt_without_buffering_rejected_events() {
    for (at, expected) in [(0.5, 0), (1.0, 1), (1.5, 1), (2.0, 0), (2.5, 0)] {
        let mut graph = fixture();
        let t = &mut machine(&mut graph).states[0].transitions[0];
        t.flags = flags::USE_TRIGGER_INTERVAL;
        t.trigger_interval = time(1.0, 2.0);
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
        if at == 0.5 {
            vm.update(1.0);
            vm.update(0.0);
            assert_eq!(
                vm.current_state(graph.root.unwrap()),
                Some(0),
                "rejected events are not queued"
            );
        }
    }
}

#[test]
fn anm_11_intervals_accepted_request_starts_after_trigger_window_closes() {
    let mut graph = fixture();
    let t = &mut machine(&mut graph).states[0].transitions[0];
    t.flags = flags::USE_TRIGGER_INTERVAL | flags::USE_INITIATE_INTERVAL;
    t.trigger_interval = time(0.0, 0.5);
    t.initiate_interval = time(1.0, 2.0);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.send_event(10);
    vm.update(0.5);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(0));
    vm.update(0.5);
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(1));
    assert_eq!(vm.changes().last().unwrap().event, Some(10));
}

#[test]
fn anm_11_intervals_event_bounds_override_time_fields_and_close_before_matching() {
    for event_sequence in [vec![10], vec![20, 21, 10], vec![20, 10]] {
        let mut graph = fixture();
        let t = &mut machine(&mut graph).states[0].transitions[0];
        t.flags = flags::USE_TRIGGER_INTERVAL;
        t.trigger_interval = events(20, 21);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, vec![]);
        for event in &event_sequence {
            vm.send_event(*event);
        }
        vm.update(0.0);
        assert_eq!(
            vm.current_state(graph.root.unwrap()),
            Some(i32::from(event_sequence == [20, 10]))
        );
    }
    for (bounds, expected) in [
        (events(10, 21), 1),
        (events(-1, 10), 0),
        (events(10, 10), 0),
    ] {
        let mut graph = fixture();
        let t = &mut machine(&mut graph).states[0].transitions[0];
        t.flags = flags::USE_TRIGGER_INTERVAL;
        t.trigger_interval = bounds;
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, vec![]);
        vm.send_event(10);
        vm.update(0.0);
        assert_eq!(vm.current_state(graph.root.unwrap()), Some(expected));
    }
}

#[test]
fn anm_11_intervals_event_initiation_needs_no_repeated_trigger() {
    let mut graph = fixture();
    let t = &mut machine(&mut graph).states[0].transitions[0];
    t.flags = flags::USE_INITIATE_INTERVAL;
    t.initiate_interval = events(20, 21);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.send_event(10);
    vm.update(5.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(0));
    vm.send_event(20);
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(1));
    assert_eq!(vm.changes().last().unwrap().event, Some(10));
}

#[test]
fn anm_11_intervals_zero_exit_is_unbounded_and_disabled_interval_flags_ignore_fields() {
    for flags in [0, flags::USE_TRIGGER_INTERVAL] {
        let mut graph = fixture();
        let t = &mut machine(&mut graph).states[0].transitions[0];
        t.flags = flags;
        t.trigger_interval = if flags == 0 {
            time(f32::NAN, -1.0)
        } else {
            time(1.0, 0.0)
        };
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, vec![]);
        vm.update(1000.0);
        vm.send_event(10);
        vm.update(0.0);
        assert_eq!(vm.current_state(graph.root.unwrap()), Some(1));
    }
}

#[test]
fn anm_11_intervals_invalid_fixed_bounds_fail_closed() {
    for bounds in [
        time(f32::NAN, 0.0),
        time(0.0, f32::INFINITY),
        time(-1.0, 1.0),
        time(2.0, 1.0),
    ] {
        let mut graph = fixture();
        let t = &mut machine(&mut graph).states[0].transitions[0];
        t.flags = flags::USE_TRIGGER_INTERVAL;
        t.trigger_interval = bounds;
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, vec![]);
        vm.update(0.5);
        vm.send_event(10);
        vm.update(0.0);
        assert_eq!(vm.current_state(graph.root.unwrap()), Some(0));
    }
}

#[test]
fn anm_11_intervals_state_reentry_resets_event_bounds() {
    let mut graph = fixture();
    let sm = machine(&mut graph);
    let mut back = sm.states[0].transitions[0].clone();
    back.event_id = 11;
    back.to_state_id = 0;
    sm.states[1].transitions.push(back);
    let t = &mut sm.states[0].transitions[0];
    t.flags = flags::USE_TRIGGER_INTERVAL;
    t.trigger_interval = events(20, -1);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    for event in [20, 10, 11, 10] {
        vm.send_event(event);
    }
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(0));
    vm.send_event(20);
    vm.send_event(10);
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(1));
}

#[test]
fn anm_11_intervals_delayed_priority_and_uninterruptible_flags_protect_request() {
    for (priority, locked, expected) in [(0, false, 1), (2, false, 0), (2, true, 1)] {
        let mut graph = fixture();
        let sm = machine(&mut graph);
        let mut competitor = sm.states[0].transitions[0].clone();
        competitor.event_id = 11;
        competitor.to_state_id = 0;
        competitor.priority = priority;
        let t = &mut sm.states[0].transitions[0];
        t.flags = flags::USE_INITIATE_INTERVAL
            | if locked {
                flags::UNINTERRUPTIBLE_WHILE_DELAYED
            } else {
                0
            };
        t.initiate_interval = time(1.0, 0.0);
        sm.states[0].transitions.push(competitor);
        let clips = ClipLibrary::new();
        let mut vm = Interpreter::new(&graph, &clips, vec![]);
        vm.send_event(10);
        vm.update(0.0);
        vm.send_event(11);
        vm.update(0.0);
        vm.update(1.0);
        vm.update(0.0);
        assert_eq!(vm.current_state(graph.root.unwrap()), Some(expected));
    }
}

#[test]
fn anm_11_intervals_condition_only_transition_obeys_trigger_window() {
    let mut graph = fixture();
    let t = &mut machine(&mut graph).states[0].transitions[0];
    t.event_id = -1;
    t.condition = Some(Condition::Expression("true".into()));
    t.flags = flags::USE_TRIGGER_INTERVAL;
    t.trigger_interval = time(1.0, 2.0);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.update(0.5);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(0));
    vm.update(0.5);
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(1));
}

#[test]
fn anm_11_intervals_wildcard_bounds_reset_when_the_state_changes() {
    let mut graph = fixture();
    let sm = machine(&mut graph);
    let mut wildcard = sm.states[0].transitions[0].clone();
    wildcard.flags = flags::IS_LOCAL_WILDCARD | flags::USE_TRIGGER_INTERVAL;
    wildcard.trigger_interval = events(20, -1);
    wildcard.to_state_id = 0;
    wildcard.event_id = 11;
    sm.wildcard_transitions.push(wildcard);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    for event in [20, 10, 11] {
        vm.send_event(event);
    }
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(1));
    vm.send_event(20);
    vm.send_event(11);
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(0));
}

#[test]
fn anm_11_intervals_deactivation_cancels_the_request() {
    let mut graph = fixture();
    let inner = graph.root.unwrap();
    let mut parent = graph.nodes[inner.0].clone();
    let NodeKind::StateMachine(outer) = &mut parent.kind else {
        panic!("machine")
    };
    outer.states[0].generator = Some(inner);
    outer.states[0].transitions[0].event_id = 12;
    outer.states[1].generator = None;
    let mut back = outer.states[0].transitions[0].clone();
    back.event_id = 13;
    back.to_state_id = 0;
    outer.states[1].transitions.push(back);
    let t = &mut machine(&mut graph).states[0].transitions[0];
    t.flags = flags::USE_INITIATE_INTERVAL;
    t.initiate_interval = events(20, -1);
    parent.id = soe_hkx::behavior::NodeId(graph.nodes.len());
    graph.root = Some(parent.id);
    graph.nodes.push(parent);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.send_event(10);
    vm.update(0.0);
    vm.send_event(12);
    vm.update(0.0);
    assert!(!vm.is_active(inner));
    vm.send_event(13);
    vm.send_event(20);
    vm.update(0.0);
    assert_eq!(vm.current_state(inner), Some(0));
}

#[test]
fn anm_11_intervals_delayed_request_latches_its_admission_condition() {
    let mut graph = fixture();
    let t = &mut machine(&mut graph).states[0].transitions[0];
    t.condition = Some(Condition::Expression("Speed > 0".into()));
    t.flags = flags::USE_INITIATE_INTERVAL;
    t.initiate_interval = events(20, -1);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    let speed = usize::try_from(graph.data.variable_index("Speed").unwrap()).unwrap();
    vm.variables.set_real(speed, 1.0);
    vm.send_event(10);
    vm.update(0.0);
    vm.variables.set_real(speed, 0.0);
    vm.send_event(20);
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(1));
}

#[test]
fn anm_11_intervals_higher_priority_event_beats_a_ready_delayed_request() {
    let mut graph = fixture();
    let sm = machine(&mut graph);
    let mut competitor = sm.states[0].transitions[0].clone();
    competitor.event_id = 20;
    competitor.to_state_id = 0;
    competitor.priority = 2;
    let t = &mut sm.states[0].transitions[0];
    t.flags = flags::USE_INITIATE_INTERVAL;
    t.initiate_interval = events(20, -1);
    sm.states[0].transitions.push(competitor);
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.send_event(10);
    vm.update(0.0);
    vm.send_event(20);
    vm.update(0.0);
    assert_eq!(vm.current_state(graph.root.unwrap()), Some(0));
    assert_eq!(vm.changes().last().unwrap().event, Some(20));
}

#[test]
fn anm_11_intervals_delayed_blend_keeps_target_state_clip_and_effect_clocks_aligned() {
    use soe_hkx::animation::SplineCompressedAnimation;
    use soe_hkx::interp::Clip;
    use soe_hkx::QsTransform;

    let mut graph = fixture();
    let effect = graph
        .nodes
        .iter_mut()
        .find(|n| matches!(n.kind, NodeKind::BlendingTransition(_)))
        .unwrap();
    let effect_id = effect.id;
    if let NodeKind::BlendingTransition(effect) = &mut effect.kind {
        effect.duration = 0.25;
        effect.blend_curve = 1;
    }
    let sm = machine(&mut graph);
    let mut back = sm.states[0].transitions[0].clone();
    back.event_id = 11;
    back.to_state_id = 0;
    back.flags = flags::USE_TRIGGER_INTERVAL;
    back.trigger_interval = time(0.25, 0.0);
    sm.states[1].transitions.push(back);
    let t = &mut sm.states[0].transitions[0];
    t.flags = flags::USE_INITIATE_INTERVAL;
    t.initiate_interval = time(1.0, 0.0);
    t.transition = Some(effect_id);
    let mut clips = ClipLibrary::new();
    for (name, x) in [("a", 0.0_f32), ("b", 10.0)] {
        let mut data = vec![0, 1, 0, 0];
        data.extend_from_slice(&x.to_le_bytes());
        clips.insert(
            Clip::from_parts(
                name,
                SplineCompressedAnimation {
                    duration: 10.0,
                    num_transform_tracks: 1,
                    num_float_tracks: 0,
                    annotation_tracks: vec![],
                    num_frames: 2,
                    num_blocks: 1,
                    max_frames_per_block: 2,
                    mask_and_quantization_size: 4,
                    block_duration: 10.0,
                    block_inverse_duration: 0.1,
                    frame_duration: 10.0,
                    block_offsets: vec![0],
                    float_block_offsets: vec![8],
                    transform_offsets: vec![],
                    float_offsets: vec![],
                    data,
                    endian: 0,
                    has_extracted_motion: false,
                },
                &[0],
            )
            .unwrap(),
        );
    }
    let target = graph.find_by_name("WalkClip").unwrap().id;
    let root = graph.root.unwrap();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    vm.send_event(10);
    vm.update(1.0);
    assert_eq!(vm.current_state(root), Some(0));
    vm.update(0.125);
    assert_eq!(vm.current_state(root), Some(1));
    assert_eq!(vm.clip_time(target), Some(0.125));
    assert!(vm.in_transition(root));
    assert_eq!(vm.pose()[0].translation[0], 5.0);
    vm.send_event(11);
    vm.update(0.125);
    assert_eq!(
        vm.current_state(root),
        Some(1),
        "target state time has not reached 0.25 at receipt"
    );
    assert_eq!(vm.clip_time(target), Some(0.25));
    assert!(!vm.in_transition(root));
    assert_eq!(vm.pose()[0].translation[0], 10.0);
    vm.send_event(11);
    vm.update(0.0);
    assert_eq!(vm.current_state(root), Some(0));
}
