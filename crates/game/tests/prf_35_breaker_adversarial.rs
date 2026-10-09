// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_trace::{Operation, WorkFrame};

#[test]
fn idle_magic_ticks_do_not_read_records_or_mutate_time_for_invalid_delta() {
    let mut plugin = Plugin::new(FileHeader::default());
    for object in 0x800..0x1800 {
        plugin.push_top(Record::new(Tag(*b"MGEF"), object).sub(Tag(*b"DATA"), [0; 152]));
    }
    let mut order = LoadOrder::new();
    order.add("Magic.esm", plugin).unwrap();
    let mut world = World::new(order);
    let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    let invalid = WorkFrame::begin();
    assert!(world.tick_magic(f64::NAN, &mut alloc).is_empty());
    assert_eq!(invalid.counts().get(Operation::RecordLookups), 0);
    assert_eq!(world.active_effects().count(), 0);
    let idle = WorkFrame::begin();
    assert!(world.tick_magic(1.0 / 60.0, &mut alloc).is_empty());
    assert_eq!(idle.counts().get(Operation::RecordLookups), 0);
    assert_eq!(world.active_effects().count(), 0);
}

#[test]
fn negative_magic_delta_leaves_empty_effect_state_unchanged_on_repetition() {
    let mut world = World::new(LoadOrder::new());
    let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    for _ in 0..8 {
        assert!(world.tick_magic(-0.5, &mut alloc).is_empty());
        assert_eq!(world.active_effects().count(), 0);
    }
}
