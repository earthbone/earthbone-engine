// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_commonlib_shim::quick_loot::ItemStack;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::loot::LootState;
use soe_game::Game;
use soe_godot_host::inventory_menu::Category;
use soe_godot_host::quick_loot::{handle, QuickLoot, Row};

const PLAYER: u32 = 0x800;
const CHEST_BASE: u32 = 0x801;
const GOLD: u32 = 0x802;
const CHEST: u32 = 0x803;
const OWNER: u32 = 0x804;
const CORPSE_BASE: u32 = 0x805;
const CORPSE: u32 = 0x806;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game(count: i32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), PLAYER).zsub(Tag(*b"EDID"), "Player"));
    plugin.push_top(Record::new(Tag(*b"MISC"), GOLD).zsub(Tag(*b"FULL"), "Gold"));
    plugin.push_top(
        Record::new(Tag(*b"CONT"), CHEST_BASE)
            .zsub(Tag(*b"FULL"), "Treasure Chest")
            .sub(
                Tag(*b"CNTO"),
                [GOLD.to_le_bytes(), count.to_le_bytes()].concat(),
            ),
    );
    plugin.push_top(Record::new(Tag(*b"FACT"), OWNER).zsub(Tag(*b"FULL"), "Other Faction"));
    plugin.push_top(
        Record::new(Tag(*b"NPC_"), CORPSE_BASE)
            .zsub(Tag(*b"EDID"), "LootableCorpse")
            .sub(
                Tag(*b"CNTO"),
                [GOLD.to_le_bytes(), count.to_le_bytes()].concat(),
            ),
    );
    plugin.push_top(Record::new(Tag(*b"REFR"), CHEST).sub(Tag(*b"NAME"), CHEST_BASE.to_le_bytes()));
    plugin
        .push_top(Record::new(Tag(*b"ACHR"), CORPSE).sub(Tag(*b"NAME"), CORPSE_BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("QuickLootHost.esp", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().reference(id(CORPSE)).unwrap().is_actor = true;
    game.world_mut().actor(id(CORPSE)).unwrap().dead = true;
    game
}

fn quick_loot(count: i32) -> (Game, QuickLoot) {
    let mut game = game(count);
    let player = game.player();
    assert!(game.activate(id(CHEST), player));
    let mut loot = QuickLoot::default();
    loot.holder = Some(id(CHEST));
    loot.rows.push(Row {
        stack: ItemStack {
            item: handle(id(GOLD)),
            count,
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
fn ui_26_take_one_refreshes_after_one_successful_transfer() {
    let (mut game, mut loot) = quick_loot(3);
    assert_eq!(loot.take(&mut game, false), 1);
    let player = game.player();
    assert_eq!(game.world_mut().item_count(id(CHEST), id(GOLD)), 2);
    assert_eq!(game.world_mut().item_count(player, id(GOLD)), 1);
}

#[test]
fn ui_26_take_all_moves_every_available_stack_quantity() {
    let (mut game, mut loot) = quick_loot(4);
    assert_eq!(loot.take(&mut game, true), 4);
    let player = game.player();
    assert_eq!(game.world_mut().item_count(id(CHEST), id(GOLD)), 0);
    assert_eq!(game.world_mut().item_count(player, id(GOLD)), 4);
}

#[test]
fn ui_26_empty_and_locked_targets_keep_their_distinct_view_states() {
    let game = game(0);
    assert_eq!(game.world_mut().loot_state(id(CHEST)), LootState::Ready);
    let mut loot = QuickLoot::default();
    loot.refresh(&game, Some(id(CHEST)), false);
    assert_eq!(loot.holder, Some(id(CHEST)));
    assert!(loot.rows.is_empty());

    game.world_mut().reference(id(CHEST)).unwrap().locked = true;
    loot.refresh(&game, Some(id(CHEST)), false);
    assert!(loot.locked);
    assert!(loot.rows.is_empty());
}

#[test]
fn ui_26_owned_item_rows_are_flagged_for_stealing() {
    let (game, mut loot) = quick_loot(1);
    game.world_mut().reference(id(CHEST)).unwrap().owner = Some(id(OWNER));
    loot.refresh(&game, Some(id(CHEST)), false);
    assert!(loot.rows[0].stealing);
}

#[test]
fn ui_26_crosshair_container_opens_the_nonmodal_loot_list() {
    let (game, mut loot) = quick_loot(2);
    game.world_mut().set_crosshair_reference(Some(id(CHEST)));
    let focused = game.world().crosshair_reference();
    loot.refresh(&game, focused, false);
    assert_eq!(loot.holder, Some(id(CHEST)));
    assert_eq!(loot.rows.len(), 1);
    assert_eq!(loot.rows[0].stack.count, 2);
}

#[test]
fn ui_26_crosshair_dead_actor_opens_the_loot_list() {
    let (game, mut loot) = quick_loot(2);
    game.world_mut().set_crosshair_reference(Some(id(CORPSE)));
    let focused = game.world().crosshair_reference();
    loot.refresh(&game, focused, false);
    assert_eq!(loot.holder, Some(id(CORPSE)));
    assert_eq!(loot.rows.len(), 1);
    assert!(!loot.rows[0].stealing);
}
