// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const DOOR: u32 = 0x100;
const PICK: u32 = 0xA;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut xloc = vec![0; 20];
    xloc[0] = 50;
    let mut plugin = Plugin::new(FileHeader::default());
    for record in [
        Record::new(T(b"NPC_"), 7).zsub(T(b"EDID"), "Player"),
        Record::new(T(b"DOOR"), 0x10),
        Record::new(T(b"MISC"), PICK).zsub(T(b"EDID"), "Lockpick"),
        Record::new(T(b"REFR"), DOOR)
            .sub(T(b"NAME"), 0x10_u32.to_le_bytes())
            .sub(T(b"XLOC"), xloc),
    ] {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn hst_03_activation_waits_for_pick_then_opens_shared_reference_once() {
    let mut game = game();
    let player = game.player();
    game.world_mut().add_item(player, id(PICK), 1);

    assert!(!game.activate_no_settle(id(DOOR), player));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert!(!game.world_mut().reference(id(DOOR)).unwrap().opened);
    assert!(game.attempt_lockpick(id(DOOR), player, id(PICK), 46.2));
    assert!(!game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(game.world_mut().item_count(player, id(PICK)), 1);

    assert!(game.activate_no_settle(id(DOOR), player));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().opened);
}

#[test]
fn hst_03_invalid_angle_does_not_mutate_lock_or_inventory() {
    let mut game = game();
    let player = game.player();
    game.world_mut().add_item(player, id(PICK), 1);

    assert!(!game.attempt_lockpick(id(DOOR), player, id(PICK), 91.0));
    assert!(!game.attempt_lockpick(id(DOOR), player, id(PICK), f32::NAN));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(game.world_mut().item_count(player, id(PICK)), 1);
}

#[test]
fn hst_03_successful_unlock_survives_save_load_before_regular_activation() {
    let mut game = game();
    let player = game.player();
    game.world_mut().add_item(player, id(PICK), 1);
    assert!(game.attempt_lockpick(id(DOOR), player, id(PICK), 46.2));

    let saved = game.save_state().unwrap();
    game.world_mut().set_locked(id(DOOR), true, &mut |_, _| {
        panic!("unexpected form allocation")
    });
    game.load_state(&saved).unwrap();

    let mut world = game.world_mut();
    let door = world.reference(id(DOOR)).unwrap();
    assert!(!door.locked);
    assert!(!door.opened);
    drop(world);
    assert!(game.activate_no_settle(id(DOOR), player));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().opened);
}

#[test]
fn hst_03_missing_target_cannot_spend_pick_or_unlock_real_door() {
    let mut game = game();
    let player = game.player();
    game.world_mut().add_item(player, id(PICK), 1);

    assert!(!game.attempt_lockpick(id(DOOR + 1), player, id(PICK), 46.2));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(game.world_mut().item_count(player, id(PICK)), 1);
}
