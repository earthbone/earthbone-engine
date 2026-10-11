// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::behavior::{BehaviorGraph, BehaviorGraphData, ClipGenerator, Node, NodeId, NodeKind};
use soe_hkx::interp::{ClipLibrary, Interpreter, SyncMarker, SynchronizedClipScene};

fn markers(values: &[(&str, f32)]) -> Vec<SyncMarker> {
    values
        .iter()
        .map(|(name, time)| SyncMarker {
            name: (*name).into(),
            time: *time,
        })
        .collect()
}

#[test]
fn both_clips_reach_each_shared_marker_at_the_same_scene_position() {
    let mut scene = SynchronizedClipScene::new(
        12,
        markers(&[("start", 0.0), ("impact", 2.0), ("end", 5.0)]),
        28,
        markers(&[("start", 1.0), ("impact", 3.0), ("end", 9.0)]),
        12,
    )
    .unwrap();

    assert_eq!(scene.time_for(12), Some(0.0));
    assert_eq!(scene.time_for(28), Some(1.0));
    assert!(scene.advance(2.0));
    assert_eq!(scene.time_for(12), Some(2.0));
    assert_eq!(scene.time_for(28), Some(3.0));
    let advanced = scene.advance(3.0);
    let final_times = [scene.time_for(12), scene.time_for(28)];
    assert_eq!((advanced, final_times), (true, [Some(5.0), Some(9.0)]));
}

#[test]
fn non_lead_clock_maps_linearly_between_uneven_shared_markers() {
    let mut scene = SynchronizedClipScene::new(
        12,
        markers(&[("start", 0.0), ("middle", 2.0), ("end", 6.0)]),
        28,
        markers(&[("start", 1.0), ("middle", 5.0), ("end", 7.0)]),
        28,
    )
    .unwrap();

    assert!(scene.advance(1.0));
    assert_eq!(scene.time_for(28), Some(2.0));
    assert_eq!(scene.time_for(12), Some(0.5));
    assert!(scene.advance(2.0));
    assert_eq!(scene.time_for(28), Some(4.0));
    assert_eq!(scene.time_for(12), Some(1.5));
}

#[test]
fn missing_or_reordered_shared_markers_reject_the_pair() {
    assert!(SynchronizedClipScene::new(
        12,
        markers(&[("start", 0.0), ("impact", 2.0)]),
        28,
        markers(&[("start", 1.0), ("other", 2.0)]),
        12,
    )
    .is_err());

    assert!(SynchronizedClipScene::new(
        12,
        markers(&[("start", 0.0), ("impact", 2.0), ("end", 4.0)]),
        28,
        markers(&[("start", 1.0), ("end", 3.0), ("impact", 4.0)]),
        12,
    )
    .is_err());
}

#[test]
fn cancellation_calls_host_release_for_both_interpreter_participants() {
    let graph = || {
        let clip = Node {
            id: NodeId(0),
            offset: 0,
            class: "hkbClipGenerator".into(),
            name: "paired clip".into(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::Clip(ClipGenerator::default()),
        };
        let synchronized = Node {
            id: NodeId(1),
            offset: 0,
            class: "BSSynchronizedClipGenerator".into(),
            name: "paired scene".into(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::SynchronizedClip {
                clip_generator: Some(NodeId(0)),
                sync_anim_prefix: String::new(),
                sync_clip_ignore_mark_placement: false,
                get_to_mark_time: 0.0,
                mark_error_threshold: 0.0,
                lead_character: false,
                reorient_support_char: false,
                apply_motion_from_root: false,
                animation_binding_index: -1,
            },
        };
        BehaviorGraph {
            name: "paired scene test".into(),
            variable_mode: 0,
            root: Some(NodeId(1)),
            data: BehaviorGraphData::default(),
            nodes: vec![clip, synchronized],
        }
    };
    let first_graph = graph();
    let second_graph = graph();
    let clips = ClipLibrary::default();
    let mut first = Interpreter::new(&first_graph, &clips, Vec::new());
    let mut second = Interpreter::new(&second_graph, &clips, Vec::new());
    assert!(first.set_synchronized_clip_time(NodeId(1), 0.75));
    assert!(second.set_synchronized_clip_time(NodeId(1), 0.5));
    let mut scene = SynchronizedClipScene::new(
        12,
        markers(&[("start", 0.0), ("impact", 1.0)]),
        28,
        markers(&[("start", 0.25), ("impact", 1.25)]),
        28,
    )
    .unwrap();
    let mut released = Vec::new();

    let actors = scene.cancel_clear_and_release_host_locks(
        &mut first,
        NodeId(1),
        &mut second,
        NodeId(1),
        |actor| released.push(actor),
    );

    assert_eq!(actors, [12, 28]);
    assert_eq!(released, [12, 28]);
    assert!(!scene.is_active());
}
