// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-16 tests for order-keyed background generation and projected LOD error.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use soe_godot_host::lod_rebuild::{
    select_far_level, silhouette_error_pixels, LoadOrderHash, LodBuildCache,
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
fn rnd_16_load_order_hash_tracks_winner_order_and_is_case_insensitive() {
    let original = LoadOrderHash::of(&order(&["Skyrim.esm", "Trees.esp", "World.esp"]));
    let case_only = LoadOrderHash::of(&order(&["skyrim.ESM", "TREES.ESP", "world.ESP"]));
    let changed = LoadOrderHash::of(&order(&["Skyrim.esm", "World.esp", "Trees.esp"]));
    assert_eq!(original, case_only);
    assert_ne!(original, changed);
}

#[test]
fn rnd_16_order_change_starts_background_rebuild_and_reuses_hash_cache() {
    let rebuilds = Arc::new(AtomicUsize::new(0));
    let mut cache = LodBuildCache::default();
    let first_key = LoadOrderHash::of(&order(&["Skyrim.esm", "Trees.esp"]));
    let calling_thread = std::thread::current().id();
    let worker_thread = Arc::new(std::sync::Mutex::new(None));
    let rebuilds_for_worker = Arc::clone(&rebuilds);
    let thread_for_worker = Arc::clone(&worker_thread);
    let first = cache
        .request(first_key, move || {
            *thread_for_worker.lock().unwrap() = Some(std::thread::current().id());
            rebuilds_for_worker.fetch_add(1, Ordering::SeqCst);
            (vec!["object lod"], vec!["terrain lod"])
        })
        .finish(&mut cache)
        .unwrap();
    assert_ne!(*worker_thread.lock().unwrap(), Some(calling_thread));
    assert_eq!(first.0, ["object lod"]);
    assert_eq!(first.1, ["terrain lod"]);

    let second_key = LoadOrderHash::of(&order(&["Skyrim.esm", "Trees.esp", "Patch.esp"]));
    let rebuilds_for_second = Arc::clone(&rebuilds);
    let second = cache
        .request(second_key, move || {
            rebuilds_for_second.fetch_add(1, Ordering::SeqCst);
            (vec!["updated object lod"], vec!["updated terrain lod"])
        })
        .finish(&mut cache)
        .unwrap();
    assert_eq!(second.0, ["updated object lod"]);
    assert_eq!(rebuilds.load(Ordering::SeqCst), 2);

    let cached = cache
        .request(second_key, || panic!("cache hit must not start a rebuild"))
        .finish(&mut cache)
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(&second, &cached));
    assert_eq!(rebuilds.load(Ordering::SeqCst), 2);
}

#[test]
fn rnd_16_far_level_uses_coarsest_level_under_one_pixel() {
    let errors = [0.0, 0.35, 0.8, 1.0];
    assert_eq!(select_far_level(errors, 0.99), LodLevel::L16);
    assert_eq!(select_far_level(errors, 0.79), LodLevel::L8);
    assert_eq!(select_far_level(errors, 0.0), LodLevel::L4);
    assert_eq!(
        select_far_level([0.1, f32::NAN, 0.6, f32::INFINITY], 0.99),
        LodLevel::L16
    );
}

#[test]
fn rnd_16_silhouette_edge_displacement_is_measured_in_pixels() {
    let full = [10.0, 20.0, 110.0, 120.0];
    let under_pixel = [10.4, 20.2, 110.7, 119.5];
    let over_pixel = [10.4, 20.2, 111.1, 119.5];
    assert!(silhouette_error_pixels(full, under_pixel) < 1.0);
    assert!(silhouette_error_pixels(full, over_pixel) >= 1.0);
}
