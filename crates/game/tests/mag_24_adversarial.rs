// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn mag_24_rejects_rfct_data_with_trailing_bytes() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"ARTO"), 1));
    plugin.push_top(Record::new(Tag::new(b"ACHR"), 2));
    let mut data = Vec::new();
    data.extend_from_slice(&1_u32.to_le_bytes());
    data.extend_from_slice(&0_u32.to_le_bytes());
    data.extend_from_slice(&1_u32.to_le_bytes());
    data.push(0xff);
    plugin.push_top(Record::new(Tag::new(b"RFCT"), 3).sub(Tag::new(b"DATA"), data));
    let mut order = LoadOrder::new();
    order.add("BadEffect.esm", plugin).unwrap();
    let mut world = World::new(order);
    assert!(world.visual_effect_data(id(3)).is_none());
    assert!(!world.play_visual_effect(id(3), id(2)));
    assert_eq!(world.visual_effects().count(), 0);
}

#[test]
fn mag_24_stop_is_idempotent_and_only_removes_the_requested_pair() {
    let mut plugin = Plugin::new(FileHeader::default());
    for target in [2, 4] {
        plugin.push_top(Record::new(Tag::new(b"ACHR"), target));
    }
    let mut data = Vec::new();
    data.extend_from_slice(&0_u32.to_le_bytes());
    data.extend_from_slice(&0_u32.to_le_bytes());
    data.extend_from_slice(&0_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"RFCT"), 3).sub(Tag::new(b"DATA"), data));
    let mut order = LoadOrder::new();
    order.add("Effect.esm", plugin).unwrap();
    let mut world = World::new(order);
    assert!(world.play_visual_effect(id(3), id(2)));
    assert!(world.play_visual_effect(id(3), id(4)));
    assert!(world.stop_visual_effect(id(3), id(2)));
    assert!(!world.stop_visual_effect(id(3), id(2)));
    assert_eq!(world.visual_effects().next().unwrap().target, id(4));
}
