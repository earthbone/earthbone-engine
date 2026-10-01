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
fn cancellation_is_idempotent_and_invalid_advance_does_not_relock_pair() {
    let mut scene = SynchronizedClipScene::new(
        1,
        markers(&[("start", 2.0), ("impact", 3.0)]),
        2,
        markers(&[("start", 1.0), ("impact", 1.5)]),
        2,
    )
    .unwrap();
    assert_eq!(scene.time_for(1), Some(2.0));
    assert_eq!(scene.time_for(2), Some(1.0));
    assert!(!scene.advance(-0.1));
    assert_eq!(scene.time_for(1), Some(2.0));
    assert_eq!(scene.cancel(), [1, 2]);
    assert_eq!(scene.cancel(), [1, 2]);
    assert_eq!(scene.time_for(1), None);
}
