// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_commonlib_shim::quick_loot::{ApiVersion, EventKind, HandleResult, ItemStack};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::inventory_menu::Category;
use soe_godot_host::quick_loot::{handle, QuickLoot, Row};

const PLAYER: u32 = 0x800;
const CHEST_BASE: u32 = 0x801;
const GOLD: u32 = 0x802;
const CHEST: u32 = 0x803;
const MOD_ITEM: u32 = 0x804;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> (Game, QuickLoot) {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), PLAYER).zsub(Tag(*b"EDID"), "Player"));
    plugin.push_top(Record::new(Tag(*b"MISC"), GOLD).zsub(Tag(*b"FULL"), "Gold"));
    plugin.push_top(Record::new(Tag(*b"MISC"), MOD_ITEM).zsub(Tag(*b"FULL"), "Added by mod"));
    plugin.push_top(Record::new(Tag(*b"CONT"), CHEST_BASE).sub(
        Tag(*b"CNTO"),
        [GOLD.to_le_bytes(), 2_i32.to_le_bytes()].concat(),
    ));
    plugin.push_top(Record::new(Tag(*b"REFR"), CHEST).sub(Tag(*b"NAME"), CHEST_BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("QuickLootHostBreaker.esp", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    assert!(game.activate(id(CHEST), player));
    let mut loot = QuickLoot::default();
    loot.api.init("breaker", ApiVersion::V21).unwrap();
    loot.holder = Some(id(CHEST));
    loot.rows.push(Row {
        stack: ItemStack {
            item: handle(id(GOLD)),
            count: 2,
            extra_index: None,
            drop_ref: None,
        },
        name: "Gold".into(),
        category: Category::Misc,
        stealing: false,
        value: 1,
        weight: 0.0,
    });
    (game, loot)
}

#[test]
fn ui_26_taking_item_cancellation_leaves_inventory_unchanged() {
    let (mut game, mut loot) = fixture();
    loot.api
        .register_handler(
            "breaker",
            EventKind::TakingItem,
            Box::new(|event| {
                event.result = HandleResult::Stop;
            }),
        )
        .unwrap();
    assert_eq!(loot.take(&mut game, false), 0);
    let player = game.player();
    assert_eq!(game.world_mut().item_count(id(CHEST), id(GOLD)), 2);
    assert_eq!(game.world_mut().item_count(player, id(GOLD)), 0);
}

#[test]
fn ui_26_modify_inventory_can_add_a_valid_item_row() {
    let (game, mut loot) = fixture();
    loot.holder = None;
    loot.rows.clear();
    loot.api
        .register_handler(
            "breaker",
            EventKind::ModifyInventory,
            Box::new(|event| {
                event.inventory.push(ItemStack {
                    item: handle(id(MOD_ITEM)),
                    count: 1,
                    extra_index: None,
                    drop_ref: None,
                });
            }),
        )
        .unwrap();
    loot.refresh(&game, Some(id(CHEST)), false);
    assert!(loot
        .rows
        .iter()
        .any(|row| row.stack.item == handle(id(MOD_ITEM))));
}
