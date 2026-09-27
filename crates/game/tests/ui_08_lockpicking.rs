// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! UI-08 acceptance checks for lockpick success and failed-pick consumption.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const DOOR: u32 = 0x100;
const PICK: u32 = 0x200;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn load_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut lock = vec![0; 20];
    lock[0] = 25;
    for record in [
        Record::new(T(b"NPC_"), 7).zsub(T(b"EDID"), "Player"),
        Record::new(T(b"DOOR"), 0x10),
        Record::new(T(b"MISC"), PICK).zsub(T(b"EDID"), "Lockpick"),
        Record::new(T(b"REFR"), DOOR)
            .sub(T(b"NAME"), 0x10_u32.to_le_bytes())
            .sub(T(b"XLOC"), lock.clone()),
    ] {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn game_with_pick() -> (Game, GlobalFormId) {
    let mut game = Game::new(load_order());
    let player = game.player();
    let pick = game.world().form("Lockpick").unwrap();
    game.world_mut().add_item(player, pick, 1);
    (game, player)
}

#[test]
fn ui_08_successful_minigame_turn_unlocks_and_opens_the_door() {
    let (mut game, player) = game_with_pick();
    let pick = game.world().form("Lockpick").unwrap();
    let target = id(DOOR);
    let level = game.world_mut().reference(target).unwrap().lock_level;
    let skill = game.world_mut().actor_value(player, "lockpicking");
    let angle = 46.2;
    assert!(soe_game::lockpicking::angle_succeeds(
        target, level, skill, angle
    ));
    assert!(game.attempt_lockpick(target, player, pick, angle));
    assert!(!game.world_mut().reference(target).unwrap().locked);
    assert!(game.activate_deferred(target, player));
    assert!(game.world_mut().reference(target).unwrap().opened);
}

#[test]
fn ui_08_failed_minigame_turn_consumes_one_pick_and_keeps_the_lock() {
    let (mut game, player) = game_with_pick();
    let pick = game.world().form("Lockpick").unwrap();
    let target = id(DOOR);
    assert!(!game.attempt_lockpick(target, player, pick, 0.0));
    assert_eq!(game.world_mut().item_count(player, pick), 0);
    assert!(game.world_mut().reference(target).unwrap().locked);
    assert!(!game.world_mut().reference(target).unwrap().opened);
}

#[test]
fn ui_08_key_only_lock_rejects_pick_without_consuming_it() {
    let (mut game, player) = game_with_pick();
    let pick = game.world().form("Lockpick").unwrap();
    let target = id(DOOR);
    game.world_mut().reference(target).unwrap().lock_level = 255;
    assert!(!game.attempt_lockpick(target, player, pick, 46.2));
    assert_eq!(game.world_mut().item_count(player, pick), 1);
    assert!(game.world_mut().reference(target).unwrap().locked);
}
