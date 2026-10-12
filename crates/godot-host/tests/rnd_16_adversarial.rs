// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for the RND-16 LOD rebuild contract.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

use soe_godot_host::lod_rebuild::{select_far_level, LoadOrderHash, LodBuildCache};
use soe_godot_host::terrain_lod::LodLevel;

#[test]
fn rnd_16_far_level_must_stay_strictly_under_one_pixel() {
    assert_eq!(
        select_far_level([0.25, 0.5, 0.75, 1.0], 1.0),
        Some(LodLevel::L16)
    );
}

#[test]
fn rnd_16_matching_pending_requests_share_one_build() {
    let key =
        LoadOrderHash::from_content_fingerprint([0x5a; 32]).with_block(0x0100_0001, 8, -4, 12);
    let builds = Arc::new(AtomicUsize::new(0));
    let (release, wait) = mpsc::channel();
    let mut cache = LodBuildCache::default();
    let builds_for_worker = Arc::clone(&builds);
    let first = cache.request(key, move || {
        builds_for_worker.fetch_add(1, Ordering::SeqCst);
        wait.recv().unwrap();
        String::from("built")
    });
    let second = cache.request(key, || panic!("a pending key must reuse its worker"));

    release.send(()).unwrap();
    let first = first.finish(&mut cache).unwrap();
    let second = second.finish(&mut cache).unwrap();

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(builds.load(Ordering::SeqCst), 1);
}
