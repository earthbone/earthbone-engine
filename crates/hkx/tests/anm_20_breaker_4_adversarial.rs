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

fn graph() -> BehaviorGraph {
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
}

#[test]
fn completed_pair_exposes_endpoint_then_stops_and_returns_both_actor_ids() {
    let mut scene = SynchronizedClipScene::new(
        4,
        markers(&[("start", 0.0), ("impact", 2.0), ("end", 5.0)]),
        8,
        markers(&[("start", 1.0), ("impact", 3.0), ("end", 9.0)]),
        4,
    )
    .unwrap();

    assert!(!scene.advance(5.0));
    assert_eq!([scene.time_for(4), scene.time_for(8)], [None, None]);
    assert_eq!(scene.cancel(), [4, 8]);
    assert_eq!([scene.time_for(4), scene.time_for(8)], [None, None]);
}

#[test]
fn invalid_actor_pairing_is_rejected_without_creating_a_scene() {
    let marks = || markers(&[("start", 0.0), ("end", 1.0)]);
    assert!(SynchronizedClipScene::new(4, marks(), 4, marks(), 4).is_err());
    assert!(SynchronizedClipScene::new(4, marks(), 8, marks(), 12).is_err());
}

#[test]
fn invalid_marker_data_is_rejected() {
    let valid = || markers(&[("start", 0.0), ("end", 1.0)]);
    for bad in [
        markers(&[("", 0.0), ("end", 1.0)]),
        markers(&[("start", -0.1), ("end", 1.0)]),
        markers(&[("start", 0.0), ("end", f32::INFINITY)]),
        markers(&[("start", 0.0), ("start", 1.0)]),
        markers(&[("start", 1.0), ("end", 0.5)]),
    ] {
        assert!(SynchronizedClipScene::new(4, bad, 8, valid(), 4).is_err());
    }
}

#[test]
fn cancellation_after_end_releases_each_participant_once() {
    let first_graph = graph();
    let second_graph = graph();
    let clips = ClipLibrary::default();
    let mut first = Interpreter::new(&first_graph, &clips, Vec::new());
    let mut second = Interpreter::new(&second_graph, &clips, Vec::new());
    assert!(first.set_synchronized_clip_time(NodeId(1), 0.4));
    assert!(second.set_synchronized_clip_time(NodeId(1), 0.8));
    let mut scene = SynchronizedClipScene::new(
        4,
        markers(&[("start", 0.0), ("end", 1.0)]),
        8,
        markers(&[("start", 2.0), ("end", 4.0)]),
        8,
    )
    .unwrap();
    assert!(!scene.advance(1.0));
    let mut released = Vec::new();

    let actors = scene.cancel_clear_and_release_host_locks(
        &mut first,
        NodeId(1),
        &mut second,
        NodeId(1),
        |actor| released.push(actor),
    );

    assert_eq!(actors, [4, 8]);
    assert_eq!(released, [4, 8]);
    assert!(!scene.is_active());
}
