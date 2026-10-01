// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{graph as graph_bytes, Root};
use soe_hkx::animation::SplineCompressedAnimation;
use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::behavior::{
    Binding, Node, NodeId, NodeKind, VariableBindingSet, VariableInfo, VariableType,
};
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
fn offset_range_endpoints_apply_zero_and_full_authored_offset() {
    let bytes = graph_bytes(&Root::Blender { parametric: false }, "base", "offset");
    let mut graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    let offset_variable_index = graph.data.variable_names.len() as i32;
    graph.data.variable_names.push("OffsetValue".into());
    graph.data.variable_infos.push(VariableInfo {
        role: 0,
        role_flags: 0,
        ty: VariableType::Real,
    });
    let base = graph
        .nodes
        .iter()
        .find(|node| node.name == "IdleClip")
        .unwrap()
        .id;
    let offset = graph
        .nodes
        .iter()
        .find(|node| node.name == "WalkClip")
        .unwrap()
        .id;
    let wrapper = NodeId(graph.nodes.len());
    graph.nodes.push(Node {
        id: wrapper,
        offset: 0,
        class: "BSOffsetAnimationGenerator".into(),
        name: "offset wrapper".into(),
        user_data: 0,
        bindings: Some(VariableBindingSet {
            bindings: vec![Binding {
                member_path: "fOffsetVariable".into(),
                variable_index: offset_variable_index,
                bit_index: -1,
                binding_type: 0,
            }],
            index_of_binding_to_enable: -1,
        }),
        kind: NodeKind::OffsetAnimation {
            default_generator: Some(base),
            offset_clip_generator: Some(offset),
            offset_variable: 0.0,
            offset_range_start: 0.0,
            offset_range_end: 1.0,
        },
    });
    graph.root = Some(wrapper);
    let mut clips = ClipLibrary::new();
    clips.insert(linear_clip("base", 10.0, 20.0));
    clips.insert(linear_clip("offset", 2.0, 6.0));
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY; 2]);
    assert!(vm.variables.set_by_name("OffsetValue", 0.0));
    let at_start = vm.update(0.0).pose[1].translation[0];
    assert!(vm.variables.set_by_name("OffsetValue", 1.0));
    let at_end = vm.update(0.0).pose[1].translation[0];
    assert!((at_start - 10.0).abs() < 0.02);
    assert!((at_end - 12.0).abs() < 0.02);
}
