// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn light_world(reference_flags: u32) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    let cell_light = vec![0_u8; 92];
    plugin.push_top(Record::new(Tag(*b"CELL"), 1).sub(b"XCLL", cell_light));
    let mut light_data = Vec::new();
    light_data.extend_from_slice(&(-1_i32).to_le_bytes());
    light_data.extend_from_slice(&100_u32.to_le_bytes());
    light_data.extend_from_slice(&[255, 255, 255, 0]);
    light_data.extend_from_slice(&0_u32.to_le_bytes());
    light_data.resize(48, 0);
    plugin.push_top(Record::new(Tag(*b"LIGH"), 2).sub(b"DATA", light_data));
    let mut placement = Vec::new();
    for _ in 0..6 {
        placement.extend_from_slice(&0.0_f32.to_le_bytes());
    }
    let mut reference = Record::new(Tag(*b"REFR"), 3)
        .with_flags(reference_flags)
        .sub(b"NAME", 2_u32.to_le_bytes())
        .sub(b"DATA", placement);
    reference.placement.cell = Some(1);
    plugin.push_top(reference);
    let mut order = LoadOrder::new();
    order.add("Detection.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn cmb_32_initially_disabled_light_reference_emits_no_detection_light() {
    let world = light_world(flags::INITIALLY_DISABLED);
    assert_eq!(
        soe_godot_host::detection::light_level(&world, id(1), [0.0; 3]),
        0.0,
        "a record-disabled reference has no active placed light"
    );
}
