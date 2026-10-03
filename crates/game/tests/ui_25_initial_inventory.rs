// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! UI-25 resolves authored CNTO lists through the existing LVLI roller once.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture(cyclic: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"MISC"), 1).zsub(Tag(*b"FULL"), "Apple"));
    plugin.push_top(
        Record::new(Tag(*b"LVLI"), 2)
            .sub(Tag(*b"LVLD"), [0])
            .sub(Tag(*b"LVLF"), [0])
            .sub(Tag(*b"LLCT"), [1])
            .sub(
                Tag(*b"LVLO"),
                [
                    1_u32.to_le_bytes(),
                    (if cyclic { 2_u32 } else { 8_u32 }).to_le_bytes(),
                    3_u32.to_le_bytes(),
                ]
                .concat(),
            ),
    );
    plugin.push_top(
        Record::new(Tag(*b"CONT"), 3)
            .zsub(Tag(*b"FULL"), "Food barrel")
            .sub(
                Tag(*b"CNTO"),
                [2_u32.to_le_bytes(), 2_i32.to_le_bytes()].concat(),
            ),
    );
    plugin.push_top(Record::new(Tag(*b"REFR"), 4).sub(Tag(*b"NAME"), 3_u32.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"LVLI"), 8)
            .sub(Tag(*b"LVLD"), [0])
            .sub(Tag(*b"LVLF"), [0])
            .sub(Tag(*b"LLCT"), [1])
            .sub(
                Tag(*b"LVLO"),
                [
                    1_u32.to_le_bytes(),
                    1_u32.to_le_bytes(),
                    1_u32.to_le_bytes(),
                ]
                .concat(),
            ),
    );
    plugin.push_top(Record::new(Tag(*b"FLST"), 5).sub(Tag(*b"LNAM"), 2_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 6).sub(Tag(*b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), 7)
            .sub(Tag(*b"NAME"), 6_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]),
    );
    let mut vendor = [0_u16.to_le_bytes(), 23_u16.to_le_bytes()].concat();
    vendor.extend_from_slice(&120_u32.to_le_bytes());
    vendor.extend_from_slice(&[0; 4]);
    plugin.push_top(
        Record::new(Tag(*b"FACT"), 9)
            .sub(Tag(*b"DATA"), (1_u32 << 14).to_le_bytes())
            .sub(Tag(*b"VEND"), 5_u32.to_le_bytes())
            .sub(Tag(*b"VENC"), 4_u32.to_le_bytes())
            .sub(Tag(*b"VENV"), vendor),
    );
    let mut order = LoadOrder::new();
    order.add("Inventory.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ui_25_authored_inventory_lists_expand_once_and_survive_save_reload() {
    let game = fixture(false);
    let mut world = game.world_mut();
    let inventory = &world.reference(id(4)).unwrap().inventory;
    assert_eq!(inventory.count(id(1)), 6);
    assert_eq!(inventory.count(id(2)), 0, "LVLI is never a displayed item");
    let saved = world.save_state();
    world.reference(id(4)).unwrap();
    assert_eq!(
        world.save_state(),
        saved,
        "a repeated lookup must not roll again"
    );
    let restored = fixture(false);
    let mut loaded = restored.world_mut();
    loaded.load_state(&saved).unwrap();
    assert_eq!(loaded.reference(id(4)).unwrap().inventory.count(id(1)), 6);
    assert_eq!(
        loaded.save_state(),
        saved,
        "reload must retain selected stacks and RNG"
    );
}

#[test]
fn ui_25_dynamic_containers_store_concrete_items_and_reject_invalid_lists() {
    let game = fixture(false);
    let mut world = game.world_mut();
    let placed = world.place_at(id(3), id(4), false);
    assert_eq!(
        world.reference_ref(placed).unwrap().inventory.count(id(1)),
        6
    );
    assert_eq!(
        world.reference_ref(placed).unwrap().inventory.count(id(2)),
        0
    );
    let invalid = fixture(true);
    let mut world = invalid.world_mut();
    assert!(
        world.reference(id(4)).is_none(),
        "cyclic initial list rejects reference construction"
    );
    let placed = world.place_at(id(3), id(99), false);
    let reference = world.reference_ref(placed).unwrap();
    assert!(!reference.enabled);
    assert_eq!(
        reference.inventory.len(),
        0,
        "invalid lists must not appear as unnamed items"
    );
}

#[test]
fn ui_25_vendor_list_filters_materialized_nested_stock_without_rerolling() {
    let mut game = fixture(false);
    assert!(game.world_mut().drive_actor(id(7)));
    let stock = game.resolve_vendor_service(id(9), id(7)).unwrap().unwrap();
    assert!(stock.available);
    assert_eq!(stock.merchandise.count(id(1)), 6);
    assert_eq!(game.world().name_of(id(1)).as_deref(), Some("Apple"));
    let before = game.world().save_state();
    let again = game.resolve_vendor_service(id(9), id(7)).unwrap().unwrap();
    assert_eq!(again.merchandise.count(id(1)), 6);
    assert_eq!(
        game.world().save_state(),
        before,
        "vendor refresh must not roll stock again"
    );
}
