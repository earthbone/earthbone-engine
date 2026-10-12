// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for RND-16's strict pixel threshold and block identity.

use soe_godot_host::lod_rebuild::{select_far_level, LoadOrderHash, LodBuildCache};
use soe_godot_host::terrain_lod::LodLevel;

#[test]
fn rnd_16_rejects_a_far_level_at_exactly_one_pixel() {
    assert_eq!(
        select_far_level([0.25, 0.5, 0.75, 1.0], 1.0),
        Some(LodLevel::L16)
    );
}

#[test]
fn rnd_16_cache_separates_worlds_and_negative_block_coordinates() {
    let load_order = [9; 32];
    let keys = [
        LoadOrderHash::from_content_fingerprint(load_order).with_block(1, 4, -4, 8),
        LoadOrderHash::from_content_fingerprint(load_order).with_block(2, 4, -4, 8),
        LoadOrderHash::from_content_fingerprint(load_order).with_block(1, 4, -8, 8),
        LoadOrderHash::from_content_fingerprint(load_order).with_block(1, 4, -4, -8),
    ];
    let mut cache = LodBuildCache::default();

    for (index, key) in keys.into_iter().enumerate() {
        let value = cache
            .request(key, move || index)
            .finish(&mut cache)
            .unwrap();
        assert_eq!(*value, index);
    }

    assert_eq!(cache.get(keys[0]).as_deref(), Some(&0));
    assert_eq!(cache.get(keys[1]).as_deref(), Some(&1));
    assert_eq!(cache.get(keys[2]).as_deref(), Some(&2));
    assert_eq!(cache.get(keys[3]).as_deref(), Some(&3));
}
