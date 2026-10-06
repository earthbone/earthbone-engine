// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::inventory::StackSource;
use soe_game::loot::LootState;
use soe_game::Game;

const PLAYER: u32 = 0x800;
const CHEST_BASE: u32 = 0x801;
const GOLD: u32 = 0x802;
const CHEST: u32 = 0x803;
const CORPSE_BASE: u32 = 0x804;
const CORPSE: u32 = 0x805;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), PLAYER).zsub(Tag(*b"EDID"), "Player"));
    plugin.push_top(
        Record::new(Tag(*b"NPC_"), CORPSE_BASE)
            .zsub(Tag(*b"EDID"), "CorpseBase")
            .sub(
                Tag(*b"CNTO"),
                [GOLD.to_le_bytes(), 2_i32.to_le_bytes()].concat(),
            ),
    );
    plugin.push_top(Record::new(Tag(*b"MISC"), GOLD).zsub(Tag(*b"FULL"), "Gold"));
    plugin.push_top(Record::new(Tag(*b"CONT"), CHEST_BASE).sub(
        Tag(*b"CNTO"),
        [GOLD.to_le_bytes(), 2_i32.to_le_bytes()].concat(),
    ));
    plugin.push_top(Record::new(Tag(*b"REFR"), CHEST).sub(Tag(*b"NAME"), CHEST_BASE.to_le_bytes()));
    plugin
        .push_top(Record::new(Tag(*b"REFR"), CORPSE).sub(Tag(*b"NAME"), CORPSE_BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("QuickLootBreaker.esp", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().reference(id(CORPSE)).unwrap().is_actor = true;
    game
}

#[test]
fn ui_26_dead_actor_contents_are_lootable_and_never_stolen() {
    let game = game();
    game.world_mut().actor(id(CORPSE)).unwrap().dead = true;
    assert_eq!(game.world_mut().loot_state(id(CORPSE)), LootState::Ready);
    let selection = game
        .world_mut()
        .reference_ref(id(CORPSE))
        .unwrap()
        .inventory
        .select(id(GOLD), StackSource::Plain, 1)
        .unwrap();
    assert!(!game
        .world_mut()
        .loot_is_stealing(id(CORPSE), id(PLAYER), &selection)
        .unwrap());
}

#[test]
fn ui_26_living_actor_is_not_a_loot_target() {
    let mut game = game();
    assert_eq!(game.world_mut().loot_state(id(CORPSE)), LootState::Hidden);
    let _ = game.world_mut().loot_state(id(CHEST));
    let selection = game
        .world_mut()
        .reference_ref(id(CHEST))
        .unwrap()
        .inventory
        .select(id(GOLD), StackSource::Plain, 1)
        .unwrap();
    game.world_mut()
        .reference(id(CHEST))
        .unwrap()
        .activation_blocked = true;
    assert!(game.loot_item(id(CHEST), &selection).is_err());
    assert_eq!(game.world_mut().item_count(id(PLAYER), id(GOLD)), 0);
    assert_eq!(game.world_mut().item_count(id(CHEST), id(GOLD)), 2);
}
