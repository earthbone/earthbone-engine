// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for RND-16 load-order cache identity and failure handling.

use std::path::PathBuf;

use soe_godot_host::lod_rebuild::{select_far_level, LoadOrderHash, LodBuildCache};
use soe_godot_host::terrain_lod::LodLevel;
use soe_profile::{Entry, LoadOrder, Slot};

fn order(enabled: bool, slot: Slot) -> LoadOrder {
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
fn rnd_16_hash_invalidates_enabled_state_and_slot_kind() {
    let regular = LoadOrderHash::of(&order(true, Slot::Regular(1)));
    let disabled = LoadOrderHash::of(&order(false, Slot::Regular(1)));
    let light = LoadOrderHash::of(&order(true, Slot::Light(1)));

    assert_ne!(regular, disabled);
    assert_ne!(regular, light);
}

#[test]
fn rnd_16_worker_panic_does_not_cache_partial_lod_and_allows_retry() {
    let key = LoadOrderHash::of(&order(true, Slot::Regular(1)));
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
        LodLevel::L16
    );
}
