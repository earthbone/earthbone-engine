// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_godot_host::detection::light_level;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn off_by_default_placed_light_does_not_add_light_until_enabled() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 1).sub(b"XCLL", [60, 60, 60]));

    let mut data = Vec::new();
    data.extend_from_slice(&(-1_i32).to_le_bytes());
    data.extend_from_slice(&100_u32.to_le_bytes());
    data.extend_from_slice(&[255, 255, 255, 0]);
    data.extend_from_slice(&(1_u32 << 5).to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"LIGH"), 2).sub(b"DATA", data));

    let mut reference = Record::new(Tag(*b"REFR"), 3)
        .sub(b"NAME", 2_u32.to_le_bytes())
        .sub(b"DATA", [0_u8; 24]);
    reference.placement.cell = Some(1);
    plugin.push_top(reference);

    let mut order = LoadOrder::new();
    order.add("OffByDefault.esm", plugin).unwrap();
    let world = World::new(order);

    let expected_ambient = 60.0 * (100.0 / 255.0);
    assert_eq!(light_level(&world, id(1), [0.0; 3]), expected_ambient);
}
