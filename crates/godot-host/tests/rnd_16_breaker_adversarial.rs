// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for RND-16 load-order cache identity and failure handling.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use soe_godot_host::lod_rebuild::{
    select_far_level, silhouette_error_pixels, LoadOrderHash, LodBuildCache,
};
use soe_godot_host::terrain_lod::LodLevel;
use soe_profile::{Entry, LoadOrder, Slot};

fn single_order() -> LoadOrder {
    LoadOrder {
        entries: vec![Entry {
            name: "Skyrim.esm".to_string(),
            path: PathBuf::from("Skyrim.esm"),
            enabled: true,
            master: false,
            light: false,
            early: false,
            masters: Vec::new(),
            slot: Some(Slot::Regular(0)),
        }],
        problems: Vec::new(),
    }
}

fn plugin_order(enabled: bool, slot: Slot) -> LoadOrder {
    LoadOrder {
        entries: vec![Entry {
            name: "Patch.esp".to_string(),
            path: PathBuf::from("Patch.esp"),
            enabled,
            master: false,
            light: matches!(slot, Slot::Light(_)),
            early: false,
            masters: vec!["Skyrim.esm".to_string()],
            slot: Some(slot),
        }],
        problems: Vec::new(),
    }
}

#[test]
fn rnd_16_does_not_select_a_far_level_when_every_level_exceeds_the_pixel_limit() {
    let errors = [1.1, 1.2, 1.5, 2.0];
    let selected = select_far_level(errors, 1.0);
    assert_eq!(selected, None);
}

#[test]
fn rnd_16_non_finite_silhouette_bounds_do_not_look_like_zero_error() {
    let invalid = silhouette_error_pixels([f32::NAN, 0.0, 10.0, 10.0], [0.0, 0.0, 10.0, 10.0]);
    assert!(!invalid.is_finite());
    assert_eq!(
        select_far_level([invalid, 0.5, 0.75, 0.9], 1.0),
        Some(LodLevel::L32)
    );
}

#[test]
fn rnd_16_concurrent_requests_for_one_load_order_build_only_once() {
    let cache_key = LoadOrderHash::of(&single_order());
    let builds = Arc::new(AtomicUsize::new(0));
    let mut cache = LodBuildCache::default();
    let first_builds = Arc::clone(&builds);
    let first = cache.request(cache_key, move || {
        first_builds.fetch_add(1, Ordering::SeqCst);
        vec!["objects", "terrain"]
    });
    let second_builds = Arc::clone(&builds);
    let second = cache.request(cache_key, move || {
        second_builds.fetch_add(1, Ordering::SeqCst);
        vec!["objects", "terrain"]
    });

    assert!(cache.get(cache_key).is_none());
    let second = second.finish(&mut cache).unwrap();
    let first = first.finish(&mut cache).unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert!(Arc::ptr_eq(&first, &cache.get(cache_key).unwrap()));
    assert_eq!(builds.load(Ordering::SeqCst), 1);
}

#[test]
fn rnd_16_failed_request_does_not_discard_a_pending_retry() {
    let cache_key = LoadOrderHash::of(&single_order());
    let mut cache = LodBuildCache::default();
    let failed = cache.request(cache_key, || panic!("failed build"));
    let stale = cache.request(cache_key, || panic!("duplicate build"));
    assert!(failed.finish(&mut cache).is_err());
    assert!(cache.get(cache_key).is_none());

    let retry = cache.request(cache_key, || vec!["objects", "terrain"]);
    assert!(stale.finish(&mut cache).is_err());
    let shared = cache
        .request(cache_key, || panic!("retry must be shared"))
        .finish(&mut cache)
        .unwrap();
    let retry = retry.finish(&mut cache).unwrap();
    assert_eq!(*retry, ["objects", "terrain"]);
    assert!(Arc::ptr_eq(&retry, &shared));
}

#[test]
fn rnd_16_hash_invalidates_enabled_state_and_slot_kind() {
    let regular = LoadOrderHash::of(&plugin_order(true, Slot::Regular(1)));
    let disabled = LoadOrderHash::of(&plugin_order(false, Slot::Regular(1)));
    let light = LoadOrderHash::of(&plugin_order(true, Slot::Light(1)));

    assert_ne!(regular, disabled);
    assert_ne!(regular, light);
}

#[test]
fn rnd_16_worker_panic_does_not_cache_partial_lod_and_allows_retry() {
    let key = LoadOrderHash::of(&plugin_order(true, Slot::Regular(1)));
    let mut cache = LodBuildCache::default();
    let failed = cache.request(key, || -> Vec<u8> { panic!("failed build") });
    assert!(failed.finish(&mut cache).is_err());
    assert!(cache.get(key).is_none());

    let retry = cache.request(key, || vec![1, 2, 3]);
    assert_eq!(&*retry.finish(&mut cache).unwrap(), &[1, 2, 3]);
}

#[test]
fn rnd_16_level_at_pixel_limit_is_accepted_and_next_level_is_rejected() {
    assert_eq!(
        select_far_level([0.2, 0.5, 1.0, 1.1], 1.0),
        Some(LodLevel::L16)
    );
}
