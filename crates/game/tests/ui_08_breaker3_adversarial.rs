// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut lock = vec![0; 20];
    lock[0] = 25;
    for record in [
        Record::new(T(b"NPC_"), 7).zsub(T(b"EDID"), "Player"),
        Record::new(T(b"DOOR"), 0x10),
        Record::new(T(b"MISC"), 0x200).zsub(T(b"EDID"), "Lockpick"),
        Record::new(T(b"REFR"), 0x100)
            .sub(T(b"NAME"), 0x10_u32.to_le_bytes())
            .sub(T(b"XLOC"), lock),
    ] {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Ui08Retry.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let pick = game.world().form("Lockpick").unwrap();
    game.world_mut().add_item(player, pick, 1);
    game
}

#[test]
fn ui_08_retry_after_failure_can_open_same_door_once_and_preserve_inventory() {
    let mut game = fixture();
    let player = game.player();
    let pick = game.world().form("Lockpick").unwrap();
    let target = GlobalFormId { file: 0, object: 0x100 };
    assert!(!game.attempt_lockpick(target, player, pick, 0.0));
    assert_eq!(game.world_mut().item_count(player, pick), 0);
    game.world_mut().add_item(player, pick, 1);
    assert!(game.attempt_lockpick(target, player, pick, 46.2));
    assert!(!game.world_mut().reference(target).unwrap().locked);
    assert_eq!(game.world_mut().item_count(player, pick), 1);
    assert!(game.activate_deferred(target, player));
    assert!(game.world_mut().reference(target).unwrap().opened);
}

#[test]
fn ui_08_repeated_turn_after_success_does_not_consume_remaining_pick() {
    let mut game = fixture();
    let player = game.player();
    let pick = game.world().form("Lockpick").unwrap();
    game.world_mut().add_item(player, pick, 2);
    let target = GlobalFormId { file: 0, object: 0x100 };
    assert!(game.attempt_lockpick(target, player, pick, 46.2));
    assert!(!game.attempt_lockpick(target, player, pick, 0.0));
    assert_eq!(game.world_mut().item_count(player, pick), 3);
    assert!(!game.world_mut().reference(target).unwrap().locked);
}
