// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

const CELL: u32 = 0x100;
const ITEM: u32 = 0x300;
const BASE: u32 = 0x400;
const REFERENCE: u32 = 0x500;
const PATCH_ITEM: u32 = 0x900;

fn id(file: u8, object: u32) -> GlobalFormId {
    GlobalFormId { file, object }
}

fn order() -> Arc<LoadOrder> {
    let mut master = Plugin::new(FileHeader::default());
    master.push_top(Record::new(Tag(*b"CELL"), CELL));
    master.push_top(Record::new(Tag(*b"MISC"), ITEM));
    master.push_top(Record::new(Tag(*b"CONT"), BASE).sub(
        Tag(*b"CNTO"),
        [ITEM.to_le_bytes(), 2_i32.to_le_bytes()].concat(),
    ));
    let mut reference = Record::new(Tag(*b"REFR"), REFERENCE)
        .sub(Tag(*b"NAME"), BASE.to_le_bytes())
        .sub(Tag(*b"DATA"), vec![0; 24]);
    reference.placement.cell = Some(CELL);
    master.push_top(reference);

    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Reset.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(Record::new(Tag(*b"MISC"), PATCH_ITEM));
    patch.push_top(Record::new(Tag(*b"CONT"), BASE).sub(
        Tag(*b"CNTO"),
        [PATCH_ITEM.to_le_bytes(), 5_i32.to_le_bytes()].concat(),
    ));

    let mut load_order = LoadOrder::new();
    load_order.add("Reset.esm", master).unwrap();
    load_order.add("ResetPatch.esp", patch).unwrap();
    Arc::new(load_order)
}

#[test]
fn sav_71_patch_added_container_item_survives_loaded_reset() {
    let order = order();
    let mut world = World::from_shared_order(order.clone());
    assert_eq!(
        world
            .reference(id(0, REFERENCE))
            .unwrap()
            .inventory
            .count(id(1, PATCH_ITEM)),
        5
    );
    {
        let changed = world.reference(id(0, REFERENCE)).unwrap();
        changed.inventory.take(id(1, PATCH_ITEM), 5);
        changed.inventory.add(id(0, ITEM), 99).unwrap();
    }

    let mut loaded = World::from_shared_order(order.clone());
    loaded.load_state(&world.save_state()).unwrap();
    assert!(loaded.reset_reference(id(0, REFERENCE), None));
    let reset = loaded.reference_ref(id(0, REFERENCE)).unwrap();
    assert_eq!(reset.inventory.count(id(1, PATCH_ITEM)), 5);
    assert_eq!(reset.inventory.count(id(0, ITEM)), 0);

    let saved = loaded.save_state();
    let mut restored = World::from_shared_order(order);
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.reference_ref(id(0, REFERENCE)), Some(reset));
}

#[test]
fn sav_71_unresolved_optional_target_still_resets_editor_state() {
    let order = order();
    let mut world = World::from_shared_order(order.clone());
    {
        let changed = world.reference(id(0, REFERENCE)).unwrap();
        changed.position = [32.0, 64.0, 128.0];
        changed.opened = true;
        changed.inventory.take(id(1, PATCH_ITEM), 5);
    }
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&world.save_state()).unwrap();

    assert!(loaded.reset_reference(id(0, REFERENCE), Some(id(0, 0xDEAD))));
    let reset = loaded.reference_ref(id(0, REFERENCE)).unwrap();
    assert_eq!(reset.position, [0.0; 3]);
    assert!(!reset.opened);
    assert_eq!(reset.inventory.count(id(1, PATCH_ITEM)), 5);
}

#[test]
fn sav_71_missing_reference_reset_returns_false_without_state_change() {
    let mut world = World::from_shared_order(order());
    let reference = id(0, REFERENCE);
    world.reference(reference).unwrap().opened = true;
    let before = world.save_state();

    assert!(!world.reset_reference(id(0, 0xDEAD), None));
    assert_eq!(world.save_state(), before);
    assert!(world.reference_ref(reference).unwrap().opened);
}

#[test]
fn sav_71_second_loaded_reset_clears_changes_made_after_first_reset() {
    let order = order();
    let mut world = World::from_shared_order(order.clone());
    assert!(world.reset_reference(id(0, REFERENCE), None));
    {
        let changed = world.reference(id(0, REFERENCE)).unwrap();
        changed.opened = true;
        changed.deleted = true;
        changed.position = [8.0, 9.0, 10.0];
        changed.inventory.add(id(1, PATCH_ITEM), 11).unwrap();
    }
    let mut loaded = World::from_shared_order(order.clone());
    loaded.load_state(&world.save_state()).unwrap();
    assert!(loaded.reset_reference(id(0, REFERENCE), None));
    assert_eq!(
        loaded.reference_ref(id(0, REFERENCE)).unwrap().position,
        [0.0; 3]
    );

    let mut restored = World::from_shared_order(order);
    let saved = loaded.save_state();
    restored.load_state(&saved).unwrap();
    let reset = restored.reference_ref(id(0, REFERENCE)).unwrap();
    assert!(!reset.opened);
    assert!(!reset.deleted);
    assert_eq!(reset.inventory.count(id(1, PATCH_ITEM)), 5);
}
