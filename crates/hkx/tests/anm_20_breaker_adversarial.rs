// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::interp::{SyncMarker, SynchronizedClipScene};

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
fn second_actor_can_lead_across_unmatched_markers() {
    let mut scene = SynchronizedClipScene::new(
        10,
        markers(&[("start", 1.0), ("private", 1.5), ("impact", 3.0)]),
        20,
        markers(&[("start", 2.0), ("extra", 2.2), ("impact", 4.0)]),
        20,
    )
    .unwrap();

    assert!(scene.advance(1.0));
    assert_eq!(scene.time_for(20), Some(3.0));
    assert_eq!(scene.time_for(10), Some(2.0));
}

#[test]
fn invalid_advance_preserves_both_marker_aligned_times() {
    let mut scene = SynchronizedClipScene::new(
        10,
        markers(&[("start", 1.0), ("impact", 3.0)]),
        20,
        markers(&[("start", 2.0), ("impact", 4.0)]),
        10,
    )
    .unwrap();
    assert!(scene.advance(0.5));
    let before = [scene.time_for(10), scene.time_for(20)];

    assert!(!scene.advance(f32::NAN));
    assert_eq!([scene.time_for(10), scene.time_for(20)], before);
    assert!(scene.is_active());
}

#[test]
fn cancel_releases_exact_pair_and_remains_idempotent_after_progress() {
    let mut scene = SynchronizedClipScene::new(
        31,
        markers(&[("start", 0.0), ("impact", 1.0)]),
        47,
        markers(&[("start", 0.25), ("impact", 1.25)]),
        47,
    )
    .unwrap();
    assert!(scene.advance(0.2));

    assert_eq!(scene.cancel(), [31, 47]);
    assert!(!scene.is_active());
    assert_eq!(scene.time_for(31), None);
    assert_eq!(scene.time_for(47), None);
    assert_eq!(scene.cancel(), [31, 47]);
    assert!(!scene.advance(0.1));
}
