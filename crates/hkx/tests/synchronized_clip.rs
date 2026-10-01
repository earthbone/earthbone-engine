#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::interp::{SyncMarker, SynchronizedClipScene};

fn markers(values: &[(&str, f32)]) -> Vec<SyncMarker> {
    values
        .iter()
        .map(|(name, time)| SyncMarker {
            name: (*name).to_string(),
            time: *time,
        })
        .collect()
}

#[test]
fn anm_20_aligns_both_clips_at_shared_markers() {
    let mut scene = SynchronizedClipScene::new(
        10,
        markers(&[("start", 1.0), ("grip", 2.0), ("finish", 4.0)]),
        20,
        markers(&[("start", 0.5), ("grip", 1.5), ("finish", 2.5)]),
        10,
    )
    .unwrap();

    assert!(scene.advance(1.0));
    assert_eq!(scene.time_for(10), Some(2.0));
    assert_eq!(scene.time_for(20), Some(1.5));

    assert!(scene.advance(1.5));
    assert_eq!(scene.time_for(10), Some(3.5));
    assert_eq!(scene.time_for(20), Some(2.25));
    assert!(!scene.advance(0.5));
}

#[test]
fn anm_20_cancellation_releases_both_participants() {
    let mut scene = SynchronizedClipScene::new(
        7,
        markers(&[("start", 0.0), ("impact", 1.0)]),
        9,
        markers(&[("start", 0.25), ("impact", 1.25)]),
        7,
    )
    .unwrap();
    assert!(scene.is_active());

    assert_eq!(scene.cancel(), [7, 9]);
    assert!(!scene.is_active());
    assert_eq!(scene.time_for(7), None);
    assert_eq!(scene.time_for(9), None);
    assert_eq!(scene.cancel(), [7, 9]);
}

#[test]
fn anm_20_rejects_pairs_without_shared_markers() {
    let result = SynchronizedClipScene::new(
        1,
        markers(&[("start", 0.0)]),
        2,
        markers(&[("begin", 0.0)]),
        1,
    );
    assert!(result.is_err());
}

#[test]
fn anm_20_rejects_shared_markers_in_different_orders() {
    let result = SynchronizedClipScene::new(
        1,
        markers(&[("start", 0.0), ("impact", 1.0), ("end", 2.0)]),
        2,
        markers(&[("start", 0.0), ("end", 1.0), ("impact", 2.0)]),
        1,
    );
    assert!(result.is_err());
}
