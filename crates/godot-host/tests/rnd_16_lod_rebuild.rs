// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use soe_godot_host::lod_rebuild::{
    select_far_level, silhouette_error_pixels, GeneratedLod, LoadOrderHash, LodBuildCache,
};
use soe_godot_host::terrain_lod::LodLevel;
use soe_profile::{Entry, LoadOrder, Slot};

fn order(names: &[&str]) -> LoadOrder {
    LoadOrder {
        entries: names
            .iter()
            .enumerate()
            .map(|(slot, name)| Entry {
                name: (*name).to_string(),
                path: PathBuf::from(name),
                enabled: true,
                master: false,
                light: false,
                early: false,
                masters: Vec::new(),
                slot: Some(Slot::Regular(slot as u8)),
            })
            .collect(),
        problems: Vec::new(),
    }
}

#[test]
fn rnd_16_cache_identity_tracks_profile_order_content_and_name_case() {
    let first =
        LoadOrderHash::of(&order(&["Skyrim.esm", "Trees.esp"])).with_content_fingerprint([1; 32]);
    let case_only =
        LoadOrderHash::of(&order(&["skyrim.ESM", "TREES.ESP"])).with_content_fingerprint([1; 32]);
    let reordered =
        LoadOrderHash::of(&order(&["Trees.esp", "Skyrim.esm"])).with_content_fingerprint([1; 32]);
    let changed_content =
        LoadOrderHash::of(&order(&["Skyrim.esm", "Trees.esp"])).with_content_fingerprint([2; 32]);
    assert_eq!(first, case_only);
    assert_ne!(first, reordered);
    assert_ne!(first, changed_content);
}

#[test]
fn rnd_16_order_change_rebuilds_object_and_terrain_on_worker_and_reuses_cache() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut cache = LodBuildCache::default();
    let key_a = LoadOrderHash::of(&order(&["Skyrim.esm", "Trees.esp"]));
    let key_b = LoadOrderHash::of(&order(&["Skyrim.esm", "Trees.esp", "Patch.esp"]));
    let caller = std::thread::current().id();
    let worker_id = Arc::new(std::sync::Mutex::new(None));
    let calls_a = Arc::clone(&calls);
    let worker_a = Arc::clone(&worker_id);
    let first = cache
        .request(key_a, move || {
            *worker_a.lock().unwrap() = Some(std::thread::current().id());
            calls_a.fetch_add(1, Ordering::SeqCst);
            GeneratedLod {
                object: vec!["objects-a"],
                terrain: vec!["terrain-a"],
            }
        })
        .finish(&mut cache)
        .unwrap();
    assert_ne!(*worker_id.lock().unwrap(), Some(caller));
    assert_eq!(first.object, ["objects-a"]);
    assert_eq!(first.terrain, ["terrain-a"]);

    let calls_b = Arc::clone(&calls);
    let second = cache
        .request(key_b, move || {
            calls_b.fetch_add(1, Ordering::SeqCst);
            GeneratedLod {
                object: vec!["objects-b"],
                terrain: vec!["terrain-b"],
            }
        })
        .finish(&mut cache)
        .unwrap();
    assert_eq!(second.object, ["objects-b"]);
    assert_eq!(second.terrain, ["terrain-b"]);
    let cached = cache
        .request(key_b, || panic!("cache hit must not rebuild"))
        .finish(&mut cache)
        .unwrap();
    assert!(Arc::ptr_eq(&second, &cached));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn rnd_16_reference_switches_to_coarsest_far_level_under_one_pixel() {
    let reference = [10.0, 20.0, 110.0, 120.0];
    let errors = [0.0, 0.35, 0.8, 1.0];
    assert_eq!(select_far_level(errors, 1.0), LodLevel::L16);
    let under_pixel = [10.4, 20.2, 110.7, 119.5];
    let over_pixel = [10.4, 20.2, 111.1, 119.5];
    assert!(silhouette_error_pixels(reference, under_pixel) < 1.0);
    assert!(silhouette_error_pixels(reference, over_pixel) >= 1.0);
    assert_eq!(
        select_far_level(
            [
                0.0,
                0.35,
                silhouette_error_pixels(reference, over_pixel),
                1.2
            ],
            1.0
        ),
        LodLevel::L8
    );
}
