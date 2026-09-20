// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Container activation loads authored `CNTO` stacks before the shared
//! inventory transaction moves an item to the player.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const PLAYER: u32 = 0x800;
const CHEST_BASE: u32 = 0x801;
const GOLD: u32 = 0x802;
const CHEST: u32 = 0x803;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), PLAYER).zsub(Tag(*b"EDID"), "Player"));
    plugin.push_top(Record::new(Tag(*b"MISC"), GOLD).zsub(Tag(*b"FULL"), "Gold"));
    plugin.push_top(
        Record::new(Tag(*b"CONT"), CHEST_BASE)
            .zsub(Tag(*b"FULL"), "Treasure Chest")
            .sub(
                Tag(*b"CNTO"),
                [GOLD.to_le_bytes(), 3_i32.to_le_bytes()].concat(),
            ),
    );
    plugin.push_top(Record::new(Tag(*b"REFR"), CHEST).sub(Tag(*b"NAME"), CHEST_BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Containers.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn activating_a_container_loads_contents_and_transfer_updates_both_holders() {
    let mut game = game();
    let player = game.player();
    let chest = id(CHEST);
    let gold = id(GOLD);

    assert!(game.activate(chest, player));
    assert!(game.world_mut().reference_ref(chest).unwrap().opened);
    assert_eq!(game.world_mut().item_count(chest, gold), 3);

    assert_eq!(game.transfer_item(chest, gold, 2, player), 2);
    assert_eq!(game.world_mut().item_count(chest, gold), 1);
    assert_eq!(game.world_mut().item_count(player, gold), 2);
}

#[test]
fn rejected_container_transfer_keeps_both_inventories_intact() {
    let mut game = game();
    let player = game.player();
    let chest = id(CHEST);
    let gold = id(GOLD);
    assert!(game.activate(chest, player));
    game.world_mut().add_item(player, gold, i32::MAX);

    assert_eq!(game.transfer_item(chest, gold, 1, player), 0);
    assert_eq!(game.world_mut().item_count(chest, gold), 3);
    assert_eq!(game.world_mut().item_count(player, gold), i32::MAX);
}

#[test]
fn locked_or_blocked_container_activation_does_not_open_contents() {
    for blocked in [false, true] {
        let mut game = game();
        let player = game.player();
        let chest = id(CHEST);
        {
            let mut world = game.world_mut();
            let state = world.reference(chest).unwrap();
            state.locked = !blocked;
            state.activation_blocked = blocked;
        }

        assert!(!game.activate(chest, player));
        assert!(!game.world_mut().reference_ref(chest).unwrap().opened);
    }
}
