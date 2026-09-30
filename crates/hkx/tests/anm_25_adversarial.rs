// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::animation::SplineCompressedAnimation;
use soe_hkx::behavior::ClipGenerator;
use soe_hkx::behavior::{BehaviorGraph, BehaviorGraphData, Node, NodeId, NodeKind};
use soe_hkx::interp::{Clip, ClipLibrary, Interpreter};
use soe_hkx::QsTransform;

fn graph() -> BehaviorGraph {
    BehaviorGraph {
        name: "direct-at adversarial".into(),
        variable_mode: 0,
        root: Some(NodeId(0)),
        data: BehaviorGraphData::default(),
        nodes: vec![
            Node {
                id: NodeId(0),
                offset: 0,
                class: "hkbModifierGenerator".into(),
                name: "root".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::ModifierGenerator {
                    modifier: Some(NodeId(1)),
                    generator: None,
                },
            },
            Node {
                id: NodeId(1),
                offset: 1,
                class: "BSDirectAtModifier".into(),
                name: "aim".into(),
                user_data: 0,
                bindings: None,
                kind: NodeKind::DirectAt {
                    enable: true,
                    direct_at_target: true,
                    source_bone_index: 0,
                    start_bone_index: 0,
                    end_bone_index: 0,
                    limit_heading_degrees: 20.0,
                    limit_pitch_degrees: 15.0,
                    offset_heading_degrees: 0.0,
                    offset_pitch_degrees: 0.0,
                    on_gain: 1.0,
                    off_gain: 1.0,
                    target_location: [10.0, 0.0, 0.0, 0.0],
                    user_info: 0,
                    direct_at_camera: false,
                    direct_at_camera_location: [0.0; 3],
                    active: true,
                    current_heading_offset: 0.0,
                    current_pitch_offset: 0.0,
                },
            },
        ],
    }
}

fn pose_clip() -> Clip {
    let mut data = vec![0, 1, 0, 0];
    data.extend_from_slice(&0.0f32.to_le_bytes());
    Clip::from_parts(
        "pose",
        SplineCompressedAnimation {
            duration: 1.0,
            num_transform_tracks: 1,
            num_float_tracks: 0,
            annotation_tracks: vec![],
            num_frames: 2,
            num_blocks: 1,
            max_frames_per_block: 2,
            mask_and_quantization_size: 4,
            block_duration: 1.0,
            block_inverse_duration: 1.0,
            frame_duration: 1.0,
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

#[test]
fn anm_25_zero_pose_weight_cannot_rotate_chain() {
    let graph = graph();
    let clips = ClipLibrary::new();
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    let before = vm.pose().to_vec();
    vm.update(1.0);
    assert_eq!(vm.pose(), before);
}

#[test]
fn anm_25_target_rotates_the_declared_bone_within_heading_limit() {
    let mut graph = graph();
    let clip_id = NodeId(graph.nodes.len());
    graph.nodes[0].kind = NodeKind::ModifierGenerator {
        modifier: Some(NodeId(1)),
        generator: Some(clip_id),
    };
    graph.nodes.push(Node {
        id: clip_id,
        offset: 2,
        class: "hkbClipGenerator".into(),
        name: "pose source".into(),
        user_data: 0,
        bindings: None,
        kind: NodeKind::Clip(ClipGenerator {
            animation_name: "pose".into(),
            crop_start_amount_local_time: 0.0,
            crop_end_amount_local_time: 0.0,
            start_time: 0.0,
            playback_speed: 1.0,
            enforced_duration: 0.0,
            user_controlled_time_fraction: 0.0,
            animation_binding_index: -1,
            mode: 1,
            flags: 0,
            triggers: vec![],
        }),
    });
    let mut clips = ClipLibrary::new();
    clips.insert(pose_clip());
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY]);
    vm.update(1.0);

    let rotation = vm.pose()[0].rotation;
    let expected_half_angle = 10.0f32.to_radians();
    assert!((rotation[2] - expected_half_angle.sin()).abs() < 1e-5);
    assert!((rotation[3] - expected_half_angle.cos()).abs() < 1e-5);
}
