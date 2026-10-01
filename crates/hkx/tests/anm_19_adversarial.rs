// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{graph as graph_bytes, Root};
use soe_hkx::animation::SplineCompressedAnimation;
use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::behavior::{BoneSwitchChild, Node, NodeId, NodeKind};
use soe_hkx::interp::{Clip, ClipLibrary, Interpreter};
use soe_hkx::spline::encode;
use soe_hkx::Packfile;
use soe_hkx::QsTransform;

fn clip(name: &str, value: f32) -> Clip {
    let mut data = vec![1 | (2 << 2), 0b0001_0000, 0b0000_0001, 0];
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&[1, 0]);
    data.extend([0, 1]);
    data.push(1);
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    data.extend_from_slice(&value.to_le_bytes());
    data.extend_from_slice(&(value + 1.0).to_le_bytes());
    data.extend_from_slice(&encode::point_16(value, value + 1.0, value).to_le_bytes());
    data.extend_from_slice(&encode::point_16(value, value + 1.0, value + 1.0).to_le_bytes());
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    data.extend_from_slice(&encode::quat_48([0.0, 0.0, 0.0, 1.0]));
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    let animation = SplineCompressedAnimation {
        duration: 1.0 / 30.0,
        num_transform_tracks: 1,
        num_float_tracks: 0,
        annotation_tracks: vec![],
        num_frames: 2,
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
fn sparse_bone_mask_keeps_default_pose_on_unselected_bone() {
    let bytes = graph_bytes(&Root::Blender { parametric: false }, "base", "overlay");
    let mut graph = BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    let base = graph
        .nodes
        .iter()
        .find(|node| node.name == "IdleClip")
        .unwrap()
        .id;
    let overlay = graph
        .nodes
        .iter()
        .find(|node| node.name == "WalkClip")
        .unwrap()
        .id;
    let root = NodeId(graph.nodes.len());
    graph.nodes.push(Node {
        id: root,
        offset: 0,
        class: "BSBoneSwitchGenerator".into(),
        name: "bone switch".into(),
        user_data: 0,
        bindings: None,
        kind: NodeKind::BoneSwitch {
            default_generator: Some(base),
            children: vec![BoneSwitchChild {
                generator: Some(overlay),
                bone_weights: Some(vec![0.0, 1.0]),
            }],
        },
    });
    graph.root = Some(root);
    let mut clips = ClipLibrary::new();
    clips.insert(clip("base", 10.0));
    clips.insert(clip("overlay", 20.0));
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY; 2]);
    let pose = vm.update(0.0).pose.clone();
    assert!((pose[1].translation[0] - 20.0).abs() < 0.02);
    assert_eq!(pose[0], QsTransform::IDENTITY);
}
