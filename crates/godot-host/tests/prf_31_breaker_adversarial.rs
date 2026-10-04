// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_godot_host::detection::light_level;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn light_world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 1));
    let mut data = vec![0_u8; 48];
    data[4..8].copy_from_slice(&100_u32.to_le_bytes());
    data[8..11].fill(255);
    plugin.push_top(Record::new(Tag(*b"LIGH"), 2).sub(b"DATA", data));
    let mut reference = Record::new(Tag(*b"REFR"), 3)
        .sub(Tag(*b"NAME"), 2_u32.to_le_bytes())
        .sub(Tag(*b"DATA"), [0_u8; 24]);
    reference.placement.cell = Some(1);
    plugin.push_top(reference);
    let mut order = LoadOrder::new();
    order.add("DetectionLight.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn prf_31_destroyed_runtime_light_stops_contributing() {
    let mut world = light_world();
    assert!(light_level(&world, id(1), [0.0; 3]) > 0.0);
    world.reference(id(3)).unwrap().destroyed = true;
    assert_eq!(light_level(&world, id(1), [0.0; 3]), 0.0);
}
