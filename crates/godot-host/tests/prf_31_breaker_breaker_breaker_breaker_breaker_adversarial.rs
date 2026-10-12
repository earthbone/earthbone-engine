// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_godot_host::detection::light_level;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    for cell in [1_u32, 2] {
        plugin.push_top(Record::new(Tag(*b"CELL"), cell));
    }
    plugin.push_top(Record::new(Tag(*b"LIGH"), 3).sub(b"DATA", {
        let mut data = vec![0_u8; 48];
        data[4..8].copy_from_slice(&100_u32.to_le_bytes());
        data[8..11].fill(255);
        data
    }));
    plugin.push_top(Record::new(Tag(*b"STAT"), 4));
    let mut reference = Record::new(Tag(*b"REFR"), 5)
        .sub(b"NAME", 3_u32.to_le_bytes())
        .sub(b"DATA", [0_u8; 24]);
    reference.placement.cell = Some(1);
    plugin.push_top(reference);
    let mut order = LoadOrder::new();
    order.add("LightMutation.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn prf_31_light_index_applies_combined_base_and_cell_mutations_before_sampling() {
    let mut world = world();
    assert!(light_level(&world, id(1), [0.0; 3]) > 0.0);

    let reference = world.reference(id(5)).unwrap();
    reference.base = Some(id(4));
    reference.cell = Some(id(2));
    assert_eq!(light_level(&world, id(1), [0.0; 3]), 0.0);
    assert_eq!(light_level(&world, id(2), [0.0; 3]), 0.0);

    let reference = world.reference(id(5)).unwrap();
    reference.base = Some(id(3));
    assert_eq!(light_level(&world, id(1), [0.0; 3]), 0.0);
    assert!(light_level(&world, id(2), [0.0; 3]) > 0.0);
}

#[test]
fn prf_31_light_index_repeated_changes_collapse_to_latest_reference_state() {
    let mut world = world();
    assert!(light_level(&world, id(1), [0.0; 3]) > 0.0);

    world.reference(id(5)).unwrap().cell = Some(id(2));
    world.reference(id(5)).unwrap().base = Some(id(4));
    world.reference(id(5)).unwrap().base = Some(id(3));
    world.reference(id(5)).unwrap().cell = Some(id(1));

    assert!(light_level(&world, id(1), [0.0; 3]) > 0.0);
    assert_eq!(light_level(&world, id(2), [0.0; 3]), 0.0);
}
