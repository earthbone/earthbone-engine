// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use soe_godot_host::lod_rebuild::{
    select_far_level, silhouette_error_pixels, LoadOrderHash, LodBuildCache,
};
use soe_godot_host::terrain_lod::LodLevel;
use soe_profile::{Entry, LoadOrder, Slot};

fn order() -> LoadOrder {
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

#[test]
fn rnd_16_does_not_select_a_far_level_when_every_level_exceeds_the_pixel_limit() {
    let errors = [1.0, 1.2, 1.5, 2.0];
    let selected = select_far_level(errors, 1.0);
    let selected_error = match selected {
        LodLevel::L4 => errors[0],
        LodLevel::L8 => errors[1],
        LodLevel::L16 => errors[2],
        LodLevel::L32 => errors[3],
    };
    assert!(selected_error < 1.0);
}

#[test]
fn rnd_16_non_finite_silhouette_bounds_do_not_look_like_zero_error() {
    let invalid = silhouette_error_pixels([f32::NAN, 0.0, 10.0, 10.0], [0.0, 0.0, 10.0, 10.0]);
    assert!(!invalid.is_finite());
    assert_eq!(select_far_level([invalid, 0.5, 0.75, 0.9], 1.0), LodLevel::L32);
}

#[test]
fn rnd_16_concurrent_requests_for_one_load_order_build_only_once() {
    let cache_key = LoadOrderHash::of(&order());
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

    first.finish(&mut cache).unwrap();
    second.finish(&mut cache).unwrap();
    assert_eq!(builds.load(Ordering::SeqCst), 1);
}
