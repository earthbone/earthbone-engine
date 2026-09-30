// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};

fn order_with_value(name: &str, value: u32) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"GLOB"), 0x100).sub(b"FLTV", value.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add(name, plugin).unwrap();
    order
}

#[test]
fn rnd_16_cache_identity_changes_when_same_named_plugin_content_changes() {
    let original = order_with_value("Patch.esp", 1).fingerprint();
    let replacement = order_with_value("Patch.esp", 2).fingerprint();

    assert_ne!(original, replacement);
}

#[test]
fn rnd_16_cache_identity_is_case_insensitive_for_plugin_names() {
    let upper = order_with_value("Patch.esp", 1).fingerprint();
    let mixed = order_with_value("pAtCh.EsP", 1).fingerprint();

    assert_eq!(upper, mixed);
}
