// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! QuickLoot transactions preserve the shared container inventory contract.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::inventory::{Field, StackSource};
use soe_game::loot::LootState;
use soe_game::Game;

const PLAYER: u32 = 0x800;
const CHEST_BASE: u32 = 0x801;
const GOLD: u32 = 0x802;
const CHEST: u32 = 0x803;
const OWNER: u32 = 0x804;

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
            .sub(Tag(*b"CNTO"), [GOLD.to_le_bytes(), count.to_le_bytes()].concat()),
    );
    plugin.push_top(Record::new(Tag(*b"FACT"), OWNER).zsub(Tag(*b"FULL"), "Other Faction"));
    plugin.push_top(Record::new(Tag(*b"REFR"), CHEST).sub(Tag(*b"NAME"), CHEST_BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("QuickLoot.esp", plugin).unwrap();
    Game::new(order)
}

fn select(game: &mut Game, count: i32) -> soe_game::inventory::Selection {
    let player = game.player();
    assert!(game.activate(id(CHEST), player));
    let chest = game
        .world_mut()
        .reference_ref(id(CHEST))
        .unwrap()
        .inventory
        .select(id(GOLD), StackSource::Plain, count)
        .unwrap();
    chest
}

#[test]
fn ui_26_authored_contents_are_available_before_activation() {
    let mut game = game(3);
    assert_eq!(game.world_mut().loot_state(id(CHEST)), LootState::Ready);
    let world = game.world();
    let reference = world.reference_ref(id(CHEST)).unwrap();
    assert_eq!(reference.inventory.iter().map(|(_, count)| count).sum::<i32>(), 3);
    assert!(!reference.opened);
}

#[test]
fn ui_26_take_one_moves_only_one_and_invalidates_the_old_selection() {
    let mut game = game(3);
    let player = game.player();
    let selection = select(&mut game, 1);
    assert_eq!(game.loot_item(id(CHEST), &selection).unwrap(), 1);
    assert_eq!(game.world_mut().item_count(id(CHEST), id(GOLD)), 2);
    assert_eq!(game.world_mut().item_count(player, id(GOLD)), 1);
    assert!(game.loot_item(id(CHEST), &selection).is_err());
    assert_eq!(game.world_mut().item_count(player, id(GOLD)), 1);
}

#[test]
fn ui_26_take_all_selection_moves_the_full_stack_once() {
    let mut game = game(4);
    let player = game.player();
    let selection = select(&mut game, 4);
    assert_eq!(game.loot_item(id(CHEST), &selection).unwrap(), 4);
    assert_eq!(game.world_mut().item_count(id(CHEST), id(GOLD)), 0);
    assert_eq!(game.world_mut().item_count(player, id(GOLD)), 4);
}

#[test]
fn ui_26_owned_container_marks_taken_stack_with_its_owner() {
    let mut game = game(2);
    let player = game.player();
    game.world_mut().reference(id(CHEST)).unwrap().owner = Some(id(OWNER));
    let selection = select(&mut game, 1);
    assert!(game
        .world_mut()
        .loot_is_stealing(id(CHEST), player, &selection)
        .unwrap());
    assert_eq!(game.loot_item(id(CHEST), &selection).unwrap(), 1);
    let stacks = game.world_mut().reference_ref(player).unwrap().inventory.stacks(id(GOLD)).to_vec();
    assert_eq!(stacks.len(), 1);
    assert_eq!(stacks[0].extra(33).unwrap().fields, vec![Field::Form(Some(id(OWNER)))]);
}

#[test]
fn ui_26_empty_and_locked_containers_have_distinct_states_without_activation() {
    let game = game(0);
    assert_eq!(game.world_mut().loot_state(id(CHEST)), LootState::Ready);
    assert!(!game.world_mut().reference_ref(id(CHEST)).unwrap().opened);
    game.world_mut().reference(id(CHEST)).unwrap().locked = true;
    assert_eq!(game.world_mut().loot_state(id(CHEST)), LootState::Locked);
    assert!(!game.world_mut().reference_ref(id(CHEST)).unwrap().opened);
}
