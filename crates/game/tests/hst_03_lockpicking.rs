// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const DOOR: u32 = 0x100;
const LOCKPICK: u32 = 0xA;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn order(level: u8) -> LoadOrder {
    let mut lock = vec![0; 20];
    lock[0] = level;
    let mut plugin = Plugin::new(FileHeader::default());
    for record in [
        Record::new(T(b"NPC_"), 7).zsub(T(b"EDID"), "Player"),
        Record::new(T(b"DOOR"), 0x10),
        Record::new(T(b"MISC"), LOCKPICK).zsub(T(b"EDID"), "Lockpick"),
        Record::new(T(b"REFR"), DOOR)
            .sub(T(b"NAME"), 0x10_u32.to_le_bytes())
            .sub(T(b"XLOC"), lock),
    ] {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

#[test]
fn hst_03_keyless_locked_door_opens_after_a_successful_pick() {
    let mut game = Game::new(order(50));
    let player = game.player();
    game.world_mut().add_item(player, id(LOCKPICK), 1);

    assert!(!game.activate_no_settle(id(DOOR), player));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert!(game.resolve_lockpick(id(DOOR), player, true));
    assert!(!game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(game.world_mut().item_count(player, id(LOCKPICK)), 1);
    assert!(game.activate_no_settle(id(DOOR), player));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().opened);
}

#[test]
fn hst_03_failed_pick_consumes_a_pick_and_keeps_the_door_locked() {
    let mut game = Game::new(order(25));
    let player = game.player();
    game.world_mut().add_item(player, id(LOCKPICK), 1);

    assert!(!game.resolve_lockpick(id(DOOR), player, false));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(game.world_mut().item_count(player, id(LOCKPICK)), 0);
}

#[test]
fn hst_03_key_only_lock_cannot_be_picked() {
    let mut game = Game::new(order(255));
    let player = game.player();
    game.world_mut().add_item(player, id(LOCKPICK), 1);

    assert!(!game.resolve_lockpick(id(DOOR), player, true));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(game.world_mut().item_count(player, id(LOCKPICK)), 1);
}

#[test]
fn hst_03_level_zero_xloc_is_a_pickable_locked_door() {
    let mut game = Game::new(order(0));
    let player = game.player();
    game.world_mut().add_item(player, id(LOCKPICK), 1);

    assert!(game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert!(game.resolve_lockpick(id(DOOR), player, true));
    assert!(!game.world_mut().reference(id(DOOR)).unwrap().locked);
}
