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
fn second_lead_drives_peer_through_unmatched_markers() {
    let mut scene = SynchronizedClipScene::new(
        11,
        markers(&[
            ("start", 2.0),
            ("private", 2.5),
            ("impact", 4.0),
            ("end", 5.0),
        ]),
        29,
        markers(&[
            ("start", 1.0),
            ("impact", 3.0),
            ("other", 3.5),
            ("end", 4.0),
        ]),
        29,
    )
    .unwrap();

    assert!(scene.advance(1.0));
    assert_eq!(scene.time_for(29), Some(2.0));
    assert_eq!(scene.time_for(11), Some(3.0));
    assert!(scene.advance(1.0));
    assert_eq!(scene.time_for(29), Some(3.0));
    assert_eq!(scene.time_for(11), Some(4.0));
}

#[test]
fn constructor_rejects_duplicate_names_and_nonfinite_or_reversed_markers() {
    for first in [
        markers(&[("start", 0.0), ("start", 1.0)]),
        markers(&[("start", 0.0), ("impact", f32::NAN)]),
        markers(&[("start", 1.0), ("impact", 0.5)]),
    ] {
        assert!(SynchronizedClipScene::new(
            1,
            first,
            2,
            markers(&[("start", 0.0), ("impact", 1.0)]),
            1,
        )
        .is_err());
    }
}

#[test]
fn cancellation_returns_both_actor_ids_even_when_peer_is_lead() {
    let mut scene = SynchronizedClipScene::new(
        101,
        markers(&[("start", 0.0), ("impact", 2.0)]),
        303,
        markers(&[("start", 1.0), ("impact", 4.0)]),
        303,
    )
    .unwrap();
    assert!(scene.advance(0.5));

    assert_eq!(scene.cancel(), [101, 303]);
    assert!(!scene.is_active());
    assert_eq!(scene.time_for(101), None);
    assert_eq!(scene.time_for(303), None);
}

#[test]
fn invalid_lead_and_duplicate_actor_identity_are_rejected() {
    let pair = markers(&[("start", 0.0), ("impact", 1.0)]);
    assert!(SynchronizedClipScene::new(1, pair.clone(), 1, pair.clone(), 1).is_err());
    assert!(SynchronizedClipScene::new(1, pair.clone(), 2, pair, 3).is_err());
}
