// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

#[path = "common/installed.rs"]
mod installed;

const CELL: u32 = 0x100;
const ITEM: u32 = 0x300;
const OVERRIDE_ITEM: u32 = 0x301;
const BASE: u32 = 0x400;
const REFERENCE: u32 = 0x500;
const ANCHOR: u32 = 0x501;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn order() -> Arc<LoadOrder> {
    let mut master = Plugin::new(FileHeader::default());
    master.push_top(Record::new(Tag(*b"CELL"), CELL));
    master.push_top(Record::new(Tag(*b"MISC"), ITEM));
    master.push_top(Record::new(Tag(*b"MISC"), OVERRIDE_ITEM));
    master.push_top(Record::new(Tag(*b"CONT"), BASE).sub(
        Tag(*b"CNTO"),
        [ITEM.to_le_bytes(), 2_i32.to_le_bytes()].concat(),
    ));
    let mut reference = Record::new(Tag(*b"REFR"), REFERENCE)
        .sub(Tag(*b"NAME"), BASE.to_le_bytes())
        .sub(Tag(*b"DATA"), vec![0; 24]);
    reference.placement.cell = Some(CELL);
    master.push_top(reference);
    let mut anchor = Record::new(Tag(*b"REFR"), ANCHOR)
        .sub(Tag(*b"NAME"), BASE.to_le_bytes())
        .sub(Tag(*b"DATA"), vec![0; 24]);
    anchor.placement.cell = Some(CELL);
    master.push_top(anchor);

    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Reset.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(Record::new(Tag(*b"CONT"), BASE).sub(
        Tag(*b"CNTO"),
        [ITEM.to_le_bytes(), 7_i32.to_le_bytes()].concat(),
    ));
    let mut load_order = LoadOrder::new();
    load_order.add("Reset.esm", master).unwrap();
    load_order.add("ResetPatch.esp", patch).unwrap();
    Arc::new(load_order)
}

#[test]
fn sav_71_loaded_reset_uses_only_the_winning_container_inventory() {
    let order = order();
    let mut world = World::from_shared_order(order.clone());
    let state = world.reference(id(REFERENCE)).unwrap();
    assert_eq!(state.inventory.count(id(ITEM)), 7);
    state.inventory.take(id(ITEM), 7);
    state.inventory.add(id(OVERRIDE_ITEM), 23).unwrap();
    let mut loaded = World::from_shared_order(order.clone());
    loaded.load_state(&world.save_state()).unwrap();

    assert!(loaded.reset_reference(id(REFERENCE), None));
    let reset = loaded.reference_ref(id(REFERENCE)).unwrap();
    assert_eq!(reset.inventory.count(id(ITEM)), 7);
    assert_eq!(reset.inventory.count(id(OVERRIDE_ITEM)), 0);
    let saved = loaded.save_state();
    let mut again = World::from_shared_order(order);
    again.load_state(&saved).unwrap();
    assert_eq!(again.reference_ref(id(REFERENCE)), Some(reset));
}

#[test]
fn sav_71_repeated_loaded_reset_rebuilds_winning_inventory() {
    let order = order();
    let mut changed = World::from_shared_order(order.clone());
    changed
        .reference(id(REFERENCE))
        .unwrap()
        .inventory
        .add(id(OVERRIDE_ITEM), 23)
        .unwrap();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&changed.save_state()).unwrap();

    assert!(loaded.reset_reference(id(REFERENCE), None));
    assert_eq!(
        loaded
            .reference_ref(id(REFERENCE))
            .unwrap()
            .inventory
            .count(id(ITEM)),
        7
    );
    loaded
        .reference(id(REFERENCE))
        .unwrap()
        .inventory
        .add(id(OVERRIDE_ITEM), 11)
        .unwrap();

    assert!(loaded.reset_reference(id(REFERENCE), None));
    let reset = loaded.reference_ref(id(REFERENCE)).unwrap();
    assert_eq!(reset.inventory.count(id(ITEM)), 7);
    assert_eq!(reset.inventory.count(id(OVERRIDE_ITEM)), 0);
}

#[test]
fn sav_71_runtime_only_reference_cannot_reset_or_change_saved_state() {
    let order = order();
    let mut world = World::from_shared_order(order);
    let dynamic = world.place_at(id(BASE), id(ANCHOR), false);
    let before = world.save_state();

    assert!(!world.reset_reference(dynamic, None));
    assert_eq!(world.save_state(), before);
    assert!(world.reference_ref(dynamic).is_some());
}

#[test]
fn sav_71_installed_vanilla_container_resets_to_authored_state_after_load() {
    let Some(order) = installed::load_order(&["Skyrim.esm"]) else {
        eprintln!("SKYRIM_DATA not set, skipping installed SAV-71 reset check");
        return;
    };
    let reference = installed::fixed_container_references(&order)
        .next()
        .expect("Skyrim.esm has a fixed-inventory placed container");
    let mut world = World::from_shared_order(order.clone());
    let authored = world.reference(reference).unwrap().clone();
    let extra = world.form("Gold001").unwrap();
    {
        let changed = world.reference(reference).unwrap();
        changed.position[0] += 256.0;
        changed.enabled = !changed.enabled;
        changed.opened = true;
        changed.inventory.add(extra, 1_000_000).unwrap();
    }
    let mut loaded = World::from_shared_order(order.clone());
    loaded.load_state(&world.save_state()).unwrap();
    assert!(loaded.reset_reference(reference, None));
    let reset = loaded.reference_ref(reference).unwrap();
    assert_eq!(reset.position, authored.position);
    assert_eq!(reset.enabled, authored.enabled);
    assert_eq!(reset.opened, authored.opened);
    assert_eq!(reset.inventory, authored.inventory);
    let saved = loaded.save_state();
    let mut again = World::from_shared_order(order);
    again.load_state(&saved).unwrap();
    assert_eq!(again.reference_ref(reference), Some(reset));
}
