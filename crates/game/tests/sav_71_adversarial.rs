// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{inventory::{Extra, Field, StackSource}, World};

const CELL: u32 = 0x100;
const TARGET_CELL: u32 = 0x101;
const ITEM: u32 = 0x300;
const BASE: u32 = 0x400;
const REFERENCE: u32 = 0x500;
const TARGET: u32 = 0x501;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn placement(object: u32, cell: u32, position: [f32; 3]) -> Record {
    let mut record = Record::new(Tag(*b"REFR"), object)
        .sub(Tag(*b"NAME"), BASE.to_le_bytes())
        .sub(
            Tag(*b"DATA"),
            position
                .into_iter()
                .chain([0.25_f32, 0.5, 0.75])
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
    record.placement.cell = Some(cell);
    record
}

fn order() -> Arc<LoadOrder> {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), CELL));
    plugin.push_top(Record::new(Tag(*b"CELL"), TARGET_CELL));
    plugin.push_top(Record::new(Tag(*b"MISC"), ITEM));
    plugin.push_top(Record::new(Tag(*b"ENCH"), 0x302));
    plugin.push_top(Record::new(Tag(*b"CONT"), BASE).sub(
        Tag(*b"CNTO"),
        [ITEM.to_le_bytes(), 2_i32.to_le_bytes()].concat(),
    ));
    plugin.push_top(placement(REFERENCE, CELL, [10.0, 20.0, 30.0]));
    plugin.push_top(placement(TARGET, TARGET_CELL, [100.0, 200.0, 300.0]));
    let mut load_order = LoadOrder::new();
    load_order.add("ResetAdversarial.esm", plugin).unwrap();
    Arc::new(load_order)
}

#[test]
fn sav_71_loaded_reset_clears_runtime_instance_extras() {
    let order = order();
    let mut world = World::from_shared_order(order.clone());
    let selected = world
        .select_item(id(REFERENCE), id(ITEM), StackSource::Plain, 1)
        .unwrap();
    world
        .update_item(
            id(REFERENCE),
            &selected,
            vec![Extra {
                kind: 155,
                fields: vec![Field::Form(Some(id(0x302))), Field::Short(87)],
            }],
        )
        .unwrap();

    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&world.save_state()).unwrap();
    assert!(loaded.reset_reference(id(REFERENCE), None));
    let inventory = &loaded.reference_ref(id(REFERENCE)).unwrap().inventory;
    assert_eq!(inventory.count(id(ITEM)), 2);
    assert!(inventory.stacks(id(ITEM)).is_empty());
}

#[test]
fn sav_71_loaded_reset_ignores_target_when_source_cell_is_unloaded() {
    let order = order();
    let mut world = World::from_shared_order(order.clone());
    world.reference(id(REFERENCE)).unwrap().position = [1.0, 2.0, 3.0];
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&world.save_state()).unwrap();
    loaded
        .update_streaming_cells(id(TARGET_CELL), &[id(TARGET_CELL)], &[])
        .unwrap();

    assert!(loaded.reset_reference(id(REFERENCE), Some(id(TARGET))));
    let reset = loaded.reference_ref(id(REFERENCE)).unwrap();
    assert_eq!(reset.position, [10.0, 20.0, 30.0]);
    assert_eq!(reset.cell, Some(id(CELL)));
}
