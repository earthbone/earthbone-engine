// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{graph as graph_bytes, Root};
use soe_hkx::animation::SplineCompressedAnimation;
use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::behavior::{EventProperty, Node, NodeId, NodeKind};
use soe_hkx::interp::{Clip, ClipLibrary, Interpreter};
use soe_hkx::spline::encode;
use soe_hkx::Packfile;
use soe_hkx::QsTransform;

fn linear_clip(name: &str, lo: f32, hi: f32) -> Clip {
    let mut data = vec![1 | (2 << 2), 0b0001_0000, 0b0000_0001, 0];
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    data.extend_from_slice(&29u16.to_le_bytes());
    data.extend_from_slice(&[1, 0]);
    data.extend(0..30);
    data.push(29);
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    data.extend_from_slice(&lo.to_le_bytes());
    data.extend_from_slice(&hi.to_le_bytes());
    for i in 0..30 {
        data.extend_from_slice(
            &encode::point_16(lo, hi, lo + (hi - lo) * i as f32 / 29.0).to_le_bytes(),
        );
    }
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    data.extend_from_slice(&encode::quat_48([0.0, 0.0, 0.0, 1.0]));
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    let animation = SplineCompressedAnimation {
        duration: 29.0 / 30.0,
        num_transform_tracks: 1,
        num_float_tracks: 0,
        annotation_tracks: vec![],
        num_frames: 30,
        num_blocks: 1,
        max_frames_per_block: 256,
        mask_and_quantization_size: 4,
        block_duration: 256.0 / 30.0,
        block_inverse_duration: 30.0 / 256.0,
        frame_duration: 1.0 / 30.0,
        block_offsets: vec![0],
        float_block_offsets: vec![0],
        transform_offsets: vec![4],
        float_offsets: vec![],
        data,
        endian: 0,
        extracted_motion: None,
    };
    Clip::from_parts(name, animation, &[1]).unwrap()
}

#[test]
fn cross_blend_reaches_authored_curve_midpoint_and_exact_duration_endpoint() {
    let bytes = graph_bytes(
        &Root::Blender { parametric: true },
        "mt_idle",
        "mt_walkforward",
    );
    let mut graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    let blender = graph.root.unwrap();
    let wrapper = NodeId(graph.nodes.len());
    graph.nodes.push(Node {
        id: wrapper,
        offset: 0,
        class: "BSCyclicBlendTransitionGenerator".into(),
        name: "cyclic".into(),
        user_data: 0,
        bindings: None,
        kind: NodeKind::CyclicBlendTransition {
            blender_generator: Some(blender),
            event_to_freeze_blend_value: EventProperty {
                id: 10,
                has_payload: false,
            },
            event_to_cross_blend: EventProperty {
                id: 11,
                has_payload: false,
            },
            blend_parameter: 0.0,
            transition_duration: 0.5,
            blend_curve: 2,
        },
    });
    graph.root = Some(wrapper);
    let mut clips = ClipLibrary::new();
    clips.insert(linear_clip("mt_idle", 0.0, 20.0));
    clips.insert(linear_clip("mt_walkforward", 100.0, 120.0));
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY; 2]);
    vm.variables.set_by_name("Speed", 1.0);
    vm.update(0.0);
    vm.send_event(10);
    vm.update(0.0);
    vm.variables.set_by_name("Speed", 0.0);
    vm.update(0.0);
    vm.send_event(11);
    vm.update(0.0);
    let halfway = vm.update(0.25).pose[1].translation[0];
    assert!(
        (25.0..35.0).contains(&halfway),
        "curve 2 midpoint must be near the 25 percent target weight, got {halfway}"
    );
    let endpoint = vm.update(0.25).pose[1].translation[0];
    assert!(
        (9.5..10.5).contains(&endpoint),
        "completed blend must use the target parameter, got {endpoint}"
    );
}
