// SPDX-License-Identifier: GPL-3.0-or-later

//! Time-zero sound and later contact annotations on the same authored clip.

use super::*;
use soe_hkx::animation::{Annotation, AnnotationTrack, SplineCompressedAnimation};
use soe_hkx::behavior::{BehaviorGraphData, ClipGenerator, Node, NodeId};
use soe_hkx::interp::{Clip, Interpreter};

fn animation() -> (BehaviorGraph, ClipLibrary) {
    let source = SplineCompressedAnimation {
        duration: 1.0,
        num_transform_tracks: 0,
        num_float_tracks: 0,
        annotation_tracks: vec![AnnotationTrack {
            track_name: "weapon".into(),
            annotations: vec![
                Annotation {
                    time: 0.0,
                    text: "SoundPlay.FixtureVocal".into(),
                },
                Annotation {
                    time: 0.2,
                    text: "preHitFrame".into(),
                },
                Annotation {
                    time: 0.5,
                    text: "HitFrame".into(),
                },
                Annotation {
                    time: 0.9,
                    text: "attackStop".into(),
                },
            ],
        }],
        num_frames: 0,
        num_blocks: 0,
        max_frames_per_block: 0,
        mask_and_quantization_size: 0,
        block_duration: 1.0,
        block_inverse_duration: 1.0,
        frame_duration: 0.0,
        block_offsets: vec![],
        float_block_offsets: vec![],
        transform_offsets: vec![],
        float_offsets: vec![],
        data: vec![],
        endian: 0,
        extracted_motion: None,
    };
    let mut clips = ClipLibrary::new();
    clips.insert(Clip::from_parts("swing", source, &[]).unwrap());
    let mut graph = BehaviorGraph {
        name: "contact".into(),
        variable_mode: 0,
        root: Some(NodeId(0)),
        data: BehaviorGraphData::default(),
        nodes: vec![Node {
            id: NodeId(0),
            offset: 0,
            class: "hkbClipGenerator".into(),
            name: "swing".into(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::Clip(ClipGenerator {
                animation_name: "swing".into(),
                playback_speed: 1.0,
                ..ClipGenerator::default()
            }),
        }],
    };
    contact_annotation_triggers(&mut graph, &clips);
    (graph, clips)
}

#[test]
fn r4_sound_annotation_fires_at_clip_start_and_contact_keeps_its_time() {
    let (graph, clips) = animation();
    let mut vm = Interpreter::new(&graph, &clips, vec![]);
    vm.update(0.016);
    let first: Vec<_> = vm
        .take_events()
        .into_iter()
        .map(|event| graph.data.event_names[event.id as usize].as_str())
        .collect();
    assert_eq!(
        first
            .iter()
            .filter(|name| **name == "SoundPlay.FixtureVocal")
            .count(),
        1
    );
    assert!(!first.contains(&"HitFrame"));
    vm.update(0.484);
    let second: Vec<_> = vm
        .take_events()
        .into_iter()
        .map(|event| graph.data.event_names[event.id as usize].as_str())
        .collect();
    assert_eq!(second.iter().filter(|name| **name == "HitFrame").count(), 1);
    assert!(!second.contains(&"SoundPlay.FixtureVocal"));
}
