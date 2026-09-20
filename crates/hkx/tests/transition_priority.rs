// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::animation::SplineCompressedAnimation;
use soe_hkx::behavior::{
    transition_flags, BehaviorGraph, Condition, NodeId, NodeKind, StateMachine,
};
use soe_hkx::interp::{Clip, ClipLibrary, Interpreter};
use soe_hkx::{Packfile, QsTransform};

fn machine(graph: &mut BehaviorGraph) -> &mut StateMachine {
    let root = graph.root.unwrap();
    let NodeKind::StateMachine(machine) = &mut graph.nodes[root.0].kind else {
        panic!("machine")
    };
    machine
}

fn clip(name: &str, x: f32) -> Clip {
    let mut data = vec![0, 1, 0, 0];
    data.extend_from_slice(&x.to_le_bytes());
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
    .unwrap()
}

fn fixture(capacity: i8) -> (BehaviorGraph, ClipLibrary, [NodeId; 3]) {
    let bytes = common::graph(&common::Root::Machine, "a", "b");
    let mut graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    let a = graph.find_by_name("IdleClip").unwrap().id;
    let b = graph.find_by_name("WalkClip").unwrap().id;
    let c = NodeId(graph.nodes.len());
    let mut node = graph.nodes[b.0].clone();
    node.id = c;
    node.name = "ThirdClip".into();
    node.bindings = None;
    if let NodeKind::Clip(clip) = &mut node.kind {
        clip.animation_name = "c".into();
    }
    graph.nodes.push(node);
    for node in &mut graph.nodes {
        match &mut node.kind {
            NodeKind::Clip(clip) => clip.triggers.clear(),
            NodeKind::BlendingTransition(effect) => {
                effect.duration = 1.0;
                effect.blend_curve = 1;
            }
            _ => {}
        }
    }
    let sm = machine(&mut graph);
    sm.max_simultaneous_transitions = capacity;
    sm.wildcard_transitions.clear();
    let mut third = sm.states[1].clone();
    third.state_id = 2;
    third.name = "Third".into();
    third.generator = Some(c);
    third.transitions.clear();
    sm.states.push(third);
    sm.states[0].transitions[0].event_id = 10;
    sm.states[0].transitions[0].priority = 1;
    let mut next = sm.states[0].transitions[0].clone();
    next.event_id = 11;
    next.to_state_id = 2;
    next.priority = 2;
    sm.states[1].transitions.push(next);
    let mut clips = ClipLibrary::new();
    clips.insert(clip("a", 0.0));
    clips.insert(clip("b", 10.0));
    clips.insert(clip("c", 20.0));
    (graph, clips, [a, b, c])
}

#[test]
fn anm_02_priority_and_capacity_retains_interrupted_blends() {
    let (graph, clips, [a, b, c]) = fixture(2);
    let root = graph.root.unwrap();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    vm.update(0.0);
    vm.send_event(10);
    vm.update(0.25);
    assert_eq!(vm.pose()[0].translation[0], 2.5);
    vm.send_event(11);
    vm.update(0.25);
    assert_eq!(vm.active_transition_count(root), 2);
    assert_eq!(vm.pose()[0].translation[0], 8.75);
    assert!(vm.is_active(a) && vm.is_active(b) && vm.is_active(c));
    assert_eq!(vm.clip_time(a), Some(0.5));
    assert_eq!(vm.clip_time(b), Some(0.5));
    assert_eq!(vm.clip_time(c), Some(0.25));
    vm.update(0.5);
    assert_eq!(vm.active_transition_count(root), 1);
    assert!(!vm.is_active(a));
    assert!(vm.is_active(b));
    vm.update(0.25);
    assert_eq!(vm.active_transition_count(root), 0);
    assert!(!vm.is_active(b));
    assert!(vm.is_active(c));
    assert_eq!(vm.pose()[0].translation[0], 20.0);
}

#[test]
fn anm_02_priority_and_capacity_evicts_oldest_instead_of_rejecting_new_transition() {
    let (graph, clips, [a, b, c]) = fixture(1);
    let root = graph.root.unwrap();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    vm.update(0.0);
    vm.send_event(10);
    vm.update(0.25);
    vm.send_event(11);
    vm.update(0.25);
    assert_eq!(vm.current_state(root), Some(2));
    assert_eq!(vm.active_transition_count(root), 1);
    assert_eq!(vm.pose()[0].translation[0], 12.5);
    assert!(!vm.is_active(a));
    assert!(vm.is_active(b) && vm.is_active(c));
}

#[test]
fn anm_02_priority_and_capacity_chooses_highest_enabled_passing_candidate() {
    for wildcard in [false, true] {
        for false_guard in [false, true] {
            let (mut graph, clips, _) = fixture(2);
            let sm = machine(&mut graph);
            let mut high = sm.states[0].transitions[0].clone();
            high.priority = 8;
            high.to_state_id = 2;
            if false_guard {
                high.condition = Some(Condition::Expression("false".into()));
            }
            if wildcard {
                high.flags |= transition_flags::IS_LOCAL_WILDCARD;
                sm.wildcard_transitions.push(high);
            } else {
                sm.states[0].transitions.push(high);
            }
            let root = graph.root.unwrap();
            let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
            vm.update(0.0);
            vm.send_event(10);
            vm.update(0.1);
            assert_eq!(
                vm.current_state(root),
                Some(if false_guard { 1 } else { 2 })
            );
        }
    }
}

#[test]
fn anm_02_priority_and_capacity_protects_active_priority_and_uninterruptible_flags() {
    for uninterruptible in [false, true] {
        let (mut graph, clips, _) = fixture(2);
        let sm = machine(&mut graph);
        if uninterruptible {
            sm.states[0].transitions[0].flags |= transition_flags::UNINTERRUPTIBLE_WHILE_PLAYING;
        } else {
            sm.states[1].transitions[0].priority = 0;
        }
        let root = graph.root.unwrap();
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
        vm.update(0.0);
        vm.send_event(10);
        vm.update(0.25);
        vm.send_event(11);
        vm.update(0.25);
        assert_eq!(vm.current_state(root), Some(1));
        assert_eq!(vm.active_transition_count(root), 1);
        vm.update(0.5);
        vm.send_event(11);
        vm.update(0.25);
        assert_eq!(vm.current_state(root), Some(2));
    }
}

#[test]
fn a_later_completed_effect_releases_all_predecessors() {
    let (mut graph, clips, [a, b, c]) = fixture(3);
    let effect = machine(&mut graph).states[1].transitions[0]
        .transition
        .unwrap();
    let short = NodeId(graph.nodes.len());
    let mut node = graph.nodes[effect.0].clone();
    node.id = short;
    if let NodeKind::BlendingTransition(effect) = &mut node.kind {
        effect.duration = 0.25;
    }
    graph.nodes.push(node);
    machine(&mut graph).states[1].transitions[0].transition = Some(short);
    let root = graph.root.unwrap();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    vm.update(0.0);
    vm.send_event(10);
    vm.update(0.25);
    vm.send_event(11);
    vm.update(0.25);
    assert_eq!(vm.active_transition_count(root), 0);
    assert!(!vm.is_active(a) && !vm.is_active(b) && vm.is_active(c));
    assert_eq!(vm.pose()[0].translation[0], 20.0);
}

#[test]
fn a_shared_generator_advances_only_once_within_a_transition_chain() {
    let (mut graph, clips, [a, b, _]) = fixture(3);
    machine(&mut graph).states[1].transitions[0].to_state_id = 0;
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    vm.update(0.0);
    vm.send_event(10);
    vm.update(0.25);
    vm.send_event(11);
    vm.update(0.25);
    assert_eq!(vm.clip_time(a), Some(0.5));
    assert_eq!(vm.clip_time(b), Some(0.5));
    assert_eq!(vm.pose()[0].translation[0], 3.75);
}

#[test]
fn an_instant_transition_releases_all_fading_generators() {
    let (mut graph, clips, [a, b, c]) = fixture(3);
    machine(&mut graph).states[1].transitions[0].transition = None;
    let root = graph.root.unwrap();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    vm.update(0.0);
    vm.send_event(10);
    vm.update(0.25);
    vm.send_event(11);
    vm.update(0.25);
    assert_eq!(vm.active_transition_count(root), 0);
    assert!(!vm.is_active(a) && !vm.is_active(b) && vm.is_active(c));
    assert_eq!(vm.pose()[0].translation[0], 20.0);
}

#[test]
fn a_disabled_high_priority_candidate_does_not_mask_the_next_candidate() {
    let (mut graph, clips, _) = fixture(2);
    let sm = machine(&mut graph);
    let mut high = sm.states[0].transitions[0].clone();
    high.priority = 8;
    high.to_state_id = 2;
    high.flags |= transition_flags::DISABLED;
    sm.states[0].transitions.push(high);
    let root = graph.root.unwrap();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    vm.update(0.0);
    vm.send_event(10);
    vm.update(0.25);
    assert_eq!(vm.current_state(root), Some(1));
}

#[test]
fn nonpositive_capacity_changes_state_without_retaining_effects() {
    for capacity in [0, -1] {
        let (graph, clips, [a, b, c]) = fixture(capacity);
        let root = graph.root.unwrap();
        let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
        vm.update(0.0);
        vm.send_event(10);
        vm.update(0.25);
        assert_eq!(vm.active_transition_count(root), 0);
        assert_eq!(vm.pose()[0].translation[0], 10.0);
        assert!(!vm.is_active(a) && vm.is_active(b));
        vm.send_event(11);
        vm.update(0.25);
        assert_eq!(vm.active_transition_count(root), 0);
        assert_eq!(vm.pose()[0].translation[0], 20.0);
        assert!(!vm.is_active(b) && vm.is_active(c));
    }
}

#[test]
fn full_capacity_above_one_evicts_only_the_oldest_effect() {
    let (mut graph, clips, _) = fixture(2);
    let sm = machine(&mut graph);
    let mut back = sm.states[1].transitions[0].clone();
    back.priority = 3;
    back.to_state_id = 0;
    back.event_id = 12;
    sm.states[2].transitions.push(back);
    let root = graph.root.unwrap();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    vm.update(0.0);
    vm.send_event(10);
    vm.update(0.25);
    vm.send_event(11);
    vm.update(0.25);
    vm.send_event(12);
    vm.update(0.25);
    assert_eq!(vm.active_transition_count(root), 2);
    assert_eq!(vm.current_state(root), Some(0));
    assert_eq!(vm.pose()[0].translation[0], 11.25);
}
