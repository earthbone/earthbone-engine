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
    let mut active = vec![0_u8; 48];
    active[4..8].copy_from_slice(&100_u32.to_le_bytes());
    active[8..11].fill(255);
    plugin.push_top(Record::new(Tag(*b"LIGH"), 2).sub(b"DATA", active));
    let mut off_by_default = vec![0_u8; 48];
    off_by_default[4..8].copy_from_slice(&100_u32.to_le_bytes());
    off_by_default[8..11].fill(255);
    off_by_default[12..16].copy_from_slice(&(1_u32 << 5).to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"LIGH"), 3).sub(b"DATA", off_by_default));
    let mut reference = Record::new(Tag(*b"REFR"), 4)
        .sub(b"NAME", 2_u32.to_le_bytes())
        .sub(b"DATA", [0_u8; 24]);
    reference.placement.cell = Some(1);
    plugin.push_top(reference);
    let mut order = LoadOrder::new();
    order.add("LightMembership.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn prf_31_breaker_light_index_tracks_runtime_base_changes_and_ligh_flags() {
    let mut world = light_world();
    assert!(light_level(&world, id(1), [0.0; 3]) > 0.0);

    world.reference(id(4)).unwrap().base = Some(id(3));
    assert_eq!(light_level(&world, id(1), [0.0; 3]), 0.0);

    world.reference(id(4)).unwrap().base = Some(id(2));
    assert!(light_level(&world, id(1), [0.0; 3]) > 0.0);
}
