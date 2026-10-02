// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const DOOR: u32 = 0x100;
const PICK: u32 = 0xA;
const KEY: u32 = 0x200;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn lock(level: u8, key: Option<u32>) -> Vec<u8> {
    let mut data = vec![0; 20];
    data[0] = level;
    if let Some(key) = key {
        data[4..8].copy_from_slice(&key.to_le_bytes());
    }
    data
}

fn game(level: u8, key: Option<u32>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in [
        Record::new(T(b"NPC_"), 7).zsub(T(b"EDID"), "Player"),
        Record::new(T(b"DOOR"), 0x10),
        Record::new(T(b"MISC"), PICK).zsub(T(b"EDID"), "Lockpick"),
        Record::new(T(b"KEYM"), KEY),
        Record::new(T(b"REFR"), DOOR)
            .sub(T(b"NAME"), 0x10_u32.to_le_bytes())
            .sub(T(b"XLOC"), lock(level, key)),
    ] {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn hst_03_matching_key_uses_regular_activation_and_preserves_both_items() {
    let mut game = game(255, Some(KEY));
    let player = game.player();
    game.world_mut().add_item(player, id(KEY), 1);
    game.world_mut().add_item(player, id(PICK), 1);

    assert!(game.activate_no_settle(id(DOOR), player));
    let (locked, opened) = {
        let mut world = game.world_mut();
        let door = world.reference(id(DOOR)).unwrap();
        (door.locked, door.opened)
    };
    assert!(!locked);
    assert!(opened);
    assert_eq!(game.world_mut().item_count(player, id(KEY)), 1);
    assert_eq!(game.world_mut().item_count(player, id(PICK)), 1);
}

#[test]
fn hst_03_key_only_door_with_pick_but_without_key_stays_locked() {
    let mut game = game(255, Some(KEY));
    let player = game.player();
    game.world_mut().add_item(player, id(PICK), 1);

    assert!(!game.activate_no_settle(id(DOOR), player));
    let (locked, opened) = {
        let mut world = game.world_mut();
        let door = world.reference(id(DOOR)).unwrap();
        (door.locked, door.opened)
    };
    assert!(locked);
    assert!(!opened);
    assert_eq!(game.world_mut().item_count(player, id(PICK)), 1);
}

#[test]
fn hst_03_blocked_keyless_door_does_not_consume_a_pick() {
    let mut game = game(25, None);
    let player = game.player();
    game.world_mut().add_item(player, id(PICK), 1);
    game.world_mut()
        .reference(id(DOOR))
        .unwrap()
        .activation_blocked = true;

    assert!(!game.activate_no_settle(id(DOOR), player));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(game.world_mut().item_count(player, id(PICK)), 1);
}

#[test]
fn hst_03_unlocked_reference_does_not_enter_lockpick_activation() {
    let mut game = game(25, None);
    let player = game.player();
    game.world_mut().add_item(player, id(PICK), 1);
    game.world_mut()
        .reference(id(DOOR))
        .unwrap()
        .locked = false;

    assert!(game.activate_no_settle(id(DOOR), player));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().opened);
    assert_eq!(game.world_mut().item_count(player, id(PICK)), 1);
}
