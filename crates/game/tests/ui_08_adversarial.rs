// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial UI-08 lockpick transaction checks.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const DOOR: u32 = 0x100;
const PICK: u32 = 0x200;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game_with_lock(xloc: Option<Vec<u8>>, picks: i32) -> (Game, GlobalFormId, GlobalFormId) {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut records = vec![
        Record::new(T(b"NPC_"), 7).zsub(T(b"EDID"), "Player"),
        Record::new(T(b"DOOR"), 0x10),
        Record::new(T(b"MISC"), PICK).zsub(T(b"EDID"), "Lockpick"),
        Record::new(T(b"REFR"), DOOR).sub(T(b"NAME"), 0x10_u32.to_le_bytes()),
    ];
    if let Some(xloc) = xloc {
        records[3] = records[3].clone().sub(T(b"XLOC"), xloc);
    }
    for record in records {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let pick = game.world().form("Lockpick").unwrap();
    if picks > 0 {
        game.world_mut().add_item(player, pick, picks);
    }
    (game, player, pick)
}

fn xloc(level: u8, len: usize) -> Vec<u8> {
    let mut bytes = vec![0; len];
    bytes[0] = level;
    bytes
}

#[test]
fn ui_08_success_does_not_consume_a_pick_and_cannot_unlock_twice() {
    let (mut game, player, pick) = game_with_lock(Some(xloc(25, 9)), 2);
    let target = id(DOOR);
    let level = game.world_mut().reference(target).unwrap().lock_level;
    let skill = game.world_mut().actor_value(player, "lockpicking");
    let angle = (u64::from(target.object) * 37 + u64::from(target.file) * 101) % 901;
    let angle = angle as f32 / 10.0;

    assert!(soe_game::lockpicking::angle_succeeds(target, level, skill, angle));
    assert!(game.attempt_lockpick(target, player, pick, angle));
    assert_eq!(game.world_mut().item_count(player, pick), 2);
    assert!(!game.attempt_lockpick(target, player, pick, angle));
    assert_eq!(game.world_mut().item_count(player, pick), 2);
    assert!(!game.world_mut().reference(target).unwrap().locked);
}

#[test]
fn ui_08_repeated_misses_spend_one_pick_per_turn_until_empty() {
    let (mut game, player, pick) = game_with_lock(Some(xloc(25, 20)), 2);
    let target = id(DOOR);
    let spot = ((u64::from(target.object) * 37 + u64::from(target.file) * 101) % 901) as f32 / 10.0;
    let miss = if spot > 45.0 { 0.0 } else { 90.0 };

    assert!(!game.attempt_lockpick(target, player, pick, miss));
    assert_eq!(game.world_mut().item_count(player, pick), 1);
    assert!(!game.attempt_lockpick(target, player, pick, miss));
    assert_eq!(game.world_mut().item_count(player, pick), 0);
    assert!(!game.attempt_lockpick(target, player, pick, miss));
    assert_eq!(game.world_mut().item_count(player, pick), 0);
    assert!(game.world_mut().reference(target).unwrap().locked);
}

#[test]
fn ui_08_unlocked_missing_and_unlocked_target_inputs_preserve_inventory() {
    let (mut game, player, pick) = game_with_lock(Some(xloc(25, 20)), 1);
    let target = id(DOOR);
    game.world_mut().reference(target).unwrap().locked = false;

    assert!(!game.attempt_lockpick(target, player, pick, 46.2));
    assert!(!game.attempt_lockpick(id(0xdead), player, pick, 46.2));
    assert_eq!(game.world_mut().item_count(player, pick), 1);
}

#[test]
fn ui_08_valid_truncated_xloc_level_255_is_key_only_and_preserves_pick() {
    let (mut game, player, pick) = game_with_lock(Some(xloc(255, 9)), 1);
    let target = id(DOOR);

    assert_eq!(game.world_mut().reference(target).unwrap().lock_level, 255);
    assert!(!game.attempt_lockpick(target, player, pick, 46.2));
    assert_eq!(game.world_mut().item_count(player, pick), 1);
    assert!(game.world_mut().reference(target).unwrap().locked);
}
