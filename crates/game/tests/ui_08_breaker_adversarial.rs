// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game(lock_level: Option<u8>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in [
        Record::new(T(b"NPC_"), 7).zsub(T(b"EDID"), "Player"),
        Record::new(T(b"DOOR"), 0x10),
        Record::new(T(b"MISC"), 0x200).zsub(T(b"EDID"), "Lockpick"),
    ] {
        plugin.push_top(record);
    }
    let door = Record::new(T(b"REFR"), 0x100).sub(T(b"NAME"), 0x10_u32.to_le_bytes());
    let door = if let Some(level) = lock_level {
        let mut xloc = vec![0; 20];
        xloc[0] = level;
        door.sub(T(b"XLOC"), xloc)
    } else {
        door
    };
    plugin.push_top(door);
    let mut order = LoadOrder::new();
    order.add("Ui08Breaker.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let pick = game.world().form("Lockpick").unwrap();
    game.world_mut().add_item(player, pick, 1);
    game
}

#[test]
fn ui_08_no_xloc_is_unlocked_and_never_consumes_a_pick() {
    let mut game = game(None);
    let player = game.player();
    let pick = game.world().form("Lockpick").unwrap();

    assert!(!game.world_mut().reference(id(0x100)).unwrap().locked);
    assert!(!game.attempt_lockpick(id(0x100), player, pick, 45.0));
    assert_eq!(game.world_mut().item_count(player, pick), 1);
}

#[test]
fn ui_08_zero_level_locked_reference_accepts_a_valid_sweet_spot() {
    let mut game = game(Some(0));
    let player = game.player();
    let pick = game.world().form("Lockpick").unwrap();
    let target = id(0x100);
    let angle = ((u64::from(target.object) * 37) % 901) as f32 / 10.0;

    assert!(game.world_mut().reference(target).unwrap().locked);
    assert!(game.attempt_lockpick(target, player, pick, angle));
    assert!(!game.world_mut().reference(target).unwrap().locked);
    assert_eq!(game.world_mut().item_count(player, pick), 1);
}
