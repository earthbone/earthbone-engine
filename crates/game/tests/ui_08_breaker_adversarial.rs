// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

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
        Record::new(T(b"REFR"), DOOR).sub(T(b"NAME"), 0x10_u32.to_le_bytes()).sub(T(b"XLOC"), lock),
    ] {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn game_with_pick() -> (Game, GlobalFormId, GlobalFormId) {
    let mut game = Game::new(load_order());
    let player = game.player();
    let pick = game.world().form("Lockpick").unwrap();
    game.world_mut().add_item(player, pick, 2);
    (game, player, pick)
}

#[test]
fn ui_08_failed_pick_and_unlocked_door_survive_save_load() {
    let (mut game, player, pick) = game_with_pick();
    assert!(!game.attempt_lockpick(id(DOOR), player, pick, 0.0));
    let saved = game.save_state().unwrap();
    let mut loaded = Game::new(load_order());
    loaded.load_state(&saved).unwrap();
    assert_eq!(loaded.world_mut().item_count(player, pick), 1);
    assert!(loaded.world_mut().reference(id(DOOR)).unwrap().locked);

    assert!(loaded.attempt_lockpick(id(DOOR), player, pick, 46.2));
    let saved = loaded.save_state().unwrap();
    let mut resumed = Game::new(load_order());
    resumed.load_state(&saved).unwrap();
    assert!(!resumed.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(resumed.world_mut().item_count(player, pick), 1);
}
