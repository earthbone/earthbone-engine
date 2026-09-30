// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::{graph_with_bound_mask, Root};
use soe_hkx::animation::SplineCompressedAnimation;
use soe_hkx::interp::{Clip, ClipLibrary, Interpreter};
use soe_hkx::spline::encode;
use soe_hkx::{Packfile, QsTransform};

fn clip(name: &str) -> Clip {
    let mut data = vec![1 | (2 << 2), 0b0001_0000, 0b0000_0001, 0];
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    let items = 30u16;
    data.extend_from_slice(&items.to_le_bytes());
    data.push(1);
    data.push(0);
    for k in 0..31u8 {
        data.push(k);
    }
    data.push(30);
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    for k in 0..31 {
        data.extend_from_slice(&encode::point_16(0.0, 1.0, k as f32 / 30.0).to_le_bytes());
    }
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    data.extend_from_slice(&encode::quat_48([0.0, 0.0, 0.0, 1.0]));
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    let anim = SplineCompressedAnimation {
        duration: 1.0,
        num_transform_tracks: 1,
        num_float_tracks: 0,
        annotation_tracks: vec![],
        num_frames: 31,
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
    Clip::from_parts(name, anim, &[1]).unwrap()
}

#[test]
fn typed_mask_pointer_changes_the_selected_mask_live_and_null_means_unmasked() {
    let bytes = graph_with_bound_mask(
        &Root::Blender { parametric: false },
        "mt_idle",
        "mt_walkforward",
        [Some(&[1.0, 0.0][..]), Some(&[0.0, 1.0][..])],
    );
    let graph =
        soe_hkx::behavior::BehaviorGraph::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
    let blender = graph
        .nodes
        .iter()
        .find_map(|n| {
            if let soe_hkx::behavior::NodeKind::Blender(b) = &n.kind {
                Some(b)
            } else {
                None
            }
        })
        .unwrap();
    let second = blender.children[1].bone_weights_pointer.unwrap();
    let mut clips = ClipLibrary::new();
    clips.insert(clip("mt_idle"));
    clips.insert(clip("mt_walkforward"));
    let mut vm = Interpreter::new(&graph, &clips, vec![QsTransform::IDENTITY; 2]);
    assert!(vm.variables.set_pointer(0, Some(second)));
    assert_eq!(vm.update(0.0).bone_weights, [0.0, 1.0]);
    assert!(vm.variables.set_pointer(0, None));
    assert_eq!(vm.update(0.0).bone_weights, [0.25, 1.0]);
}
