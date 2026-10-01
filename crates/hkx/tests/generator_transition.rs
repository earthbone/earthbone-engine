// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use soe_hkx::animation::SplineCompressedAnimation;
use soe_hkx::behavior::{BehaviorGraph, NodeId, NodeKind};
use soe_hkx::interp::{Clip, ClipLibrary, Interpreter};
use soe_hkx::{Packfile, QsTransform};

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
            extracted_motion: None,
        },
        &[0],
    )
    .unwrap()
}

fn graph() -> (BehaviorGraph, ClipLibrary, NodeId) {
    let bytes = common::graph(&common::Root::Machine, "a", "b");
    let mut graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    let transition_generator = NodeId(graph.nodes.len());
    let mut node = graph
        .nodes
        .iter()
        .find(|node| node.name == "WalkClip")
        .unwrap()
        .clone();
    node.id = transition_generator;
    node.name = "TransitionClip".into();
    node.bindings = None;
    let NodeKind::Clip(generator_clip) = &mut node.kind else {
        panic!("clip fixture")
    };
    generator_clip.animation_name = "transition".into();
    generator_clip.mode = 0;
    generator_clip.enforced_duration = 1.0;
    generator_clip.triggers.clear();
    graph.nodes.push(node);

    let effect = graph
        .nodes
        .iter_mut()
        .find(|node| matches!(node.kind, NodeKind::BlendingTransition(_)))
        .unwrap();
    effect.kind = NodeKind::GeneratorTransition(soe_hkx::behavior::GeneratorTransitionEffect {
        self_transition_mode: 0,
        event_mode: 0,
        transition_generator: Some(transition_generator),
        blend_in_duration: 0.5,
        blend_out_duration: 0.5,
        sync_to_generator_start_time: false,
    });

    let mut clips = ClipLibrary::new();
    clips.insert(clip("a", 0.0));
    clips.insert(clip("b", 10.0));
    clips.insert(clip("transition", 20.0));
    (graph, clips, transition_generator)
}

#[test]
fn anm_22_generator_transition_samples_authored_generator_during_transition_interval() {
    let (graph, clips, transition_generator) = graph();
    let machine = graph.root.unwrap();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    vm.update(0.0);
    vm.send_event(1);

    vm.update(0.25);
    assert_eq!(vm.clip_time(transition_generator), Some(0.25));
    assert!((vm.pose()[0].translation[0] - 10.0).abs() < 1e-5);
    assert!(vm.in_transition(machine));

    vm.update(0.25);
    assert_eq!(vm.clip_time(transition_generator), Some(0.5));
    assert!((vm.pose()[0].translation[0] - 20.0).abs() < 1e-5);

    vm.update(0.5);
    assert_eq!(vm.clip_time(transition_generator), Some(1.0));
    vm.update(0.25);
    assert!((vm.pose()[0].translation[0] - 15.0).abs() < 1e-5);
    vm.update(0.25);
    assert_eq!(vm.pose()[0].translation[0], 10.0);
    assert!(!vm.in_transition(machine));
}
