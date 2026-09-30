// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, Handle, Pending};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const DOOR: u32 = 0x100;
const LOCKPICK: u32 = 0xA;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn order(level: u8, with_lockpick: bool) -> LoadOrder {
    let mut lock = vec![0; 20];
    lock[0] = level;
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 7).zsub(T(b"EDID"), "Player"));
    plugin.push_top(Record::new(T(b"DOOR"), 0x10));
    if with_lockpick {
        plugin.push_top(Record::new(T(b"MISC"), LOCKPICK).zsub(T(b"EDID"), "Lockpick"));
    }
    plugin.push_top(
        Record::new(T(b"REFR"), DOOR)
            .sub(T(b"NAME"), 0x10_u32.to_le_bytes())
            .sub(T(b"XLOC"), lock),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn lock_events(game: &Game) -> usize {
    game.world()
        .pending
        .iter()
        .filter(
            |event| matches!(event, Pending::Event { name, .. } if name == "OnLockStateChanged"),
        )
        .count()
}

#[test]
fn hst_03_unlock_of_a_very_easy_lock_emits_one_state_change() {
    let mut game = Game::new(order(0, true));
    let player = game.player();
    game.object_of(Handle::Form(id(DOOR)));
    game.world_mut().add_item(player, id(LOCKPICK), 1);
    assert!(game
        .world_mut()
        .resolve_lockpick(id(DOOR), player, true, &mut |_, _| {
            panic!("unexpected form allocation")
        }));
    assert!(!game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(lock_events(&game), 1);
}

#[test]
fn hst_03_key_only_lock_remains_locked_and_does_not_emit_a_change() {
    let mut game = Game::new(order(255, true));
    let player = game.player();
    game.world_mut().add_item(player, id(LOCKPICK), 1);
    assert!(!game.resolve_lockpick(id(DOOR), player, true));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(lock_events(&game), 0);
}

#[test]
fn hst_03_repeating_success_cannot_change_an_already_unlocked_door() {
    let mut game = Game::new(order(50, true));
    let player = game.player();
    game.object_of(Handle::Form(id(DOOR)));
    game.world_mut().add_item(player, id(LOCKPICK), 2);
    assert!(game
        .world_mut()
        .resolve_lockpick(id(DOOR), player, true, &mut |_, _| {
            panic!("unexpected form allocation")
        }));
    assert_eq!(lock_events(&game), 1);
    game.world_mut().pending.clear();
    assert!(!game
        .world_mut()
        .resolve_lockpick(id(DOOR), player, true, &mut |_, _| {
            panic!("unexpected form allocation")
        }));
    assert!(!game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(lock_events(&game), 0);
    assert_eq!(game.world_mut().item_count(player, id(LOCKPICK)), 2);
}

#[test]
fn hst_03_missing_lockpick_record_cannot_unlock_the_door() {
    let mut game = Game::new(order(25, false));
    let player = game.player();
    game.world_mut().add_item(player, id(LOCKPICK), 1);
    assert!(!game.resolve_lockpick(id(DOOR), player, true));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(lock_events(&game), 0);
}

#[test]
fn hst_03_unlock_state_survives_engine_save_and_load() {
    let mut game = Game::new(order(50, true));
    let player = game.player();
    game.world_mut().add_item(player, id(LOCKPICK), 1);
    assert!(game.resolve_lockpick(id(DOOR), player, true));
    let saved = game.save_state().unwrap();
    game.world_mut().set_locked(id(DOOR), true, &mut |_, _| {
        panic!("unexpected form allocation")
    });
    game.load_state(&saved).unwrap();
    assert!(!game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert!(game.activate_no_settle(id(DOOR), player));
}

#[test]
fn hst_03_missing_target_does_not_change_the_existing_locked_door() {
    let mut game = Game::new(order(50, true));
    let player = game.player();
    game.world_mut().add_item(player, id(LOCKPICK), 1);
    assert!(!game.resolve_lockpick(id(DOOR + 1), player, true));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(lock_events(&game), 0);
}
