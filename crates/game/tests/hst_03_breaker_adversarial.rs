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

fn order(level: u8, include_pick: bool) -> LoadOrder {
    let mut lock = vec![0; 20];
    lock[0] = level;
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 7).zsub(T(b"EDID"), "Player"));
    plugin.push_top(Record::new(T(b"DOOR"), 0x10));
    if include_pick {
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
fn hst_03_success_updates_the_door_once_and_preserves_the_pick() {
    let mut game = Game::new(order(50, true));
    let player = game.player();
    game.world_mut().add_item(player, id(LOCKPICK), 1);

    assert!(game.attempt_lockpick(id(DOOR), player, id(LOCKPICK), 46.2));
    assert!(!game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(game.world_mut().item_count(player, id(LOCKPICK)), 1);
    assert!(game.activate_no_settle(id(DOOR), player));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().opened);
}

#[test]
fn hst_03_failed_turn_consumes_one_pick_and_keeps_the_lock() {
    let mut game = Game::new(order(50, true));
    let player = game.player();
    game.world_mut().add_item(player, id(LOCKPICK), 2);

    assert!(!game.attempt_lockpick(id(DOOR), player, id(LOCKPICK), 0.0));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(game.world_mut().item_count(player, id(LOCKPICK)), 1);
    assert_eq!(lock_events(&game), 0);
}

#[test]
fn hst_03_key_only_missing_record_or_missing_item_never_unlocks() {
    let mut key_only = Game::new(order(255, true));
    let player = key_only.player();
    key_only.world_mut().add_item(player, id(LOCKPICK), 1);
    assert!(!key_only.attempt_lockpick(id(DOOR), player, id(LOCKPICK), 46.2));
    assert!(key_only.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(key_only.world_mut().item_count(player, id(LOCKPICK)), 1);

    let mut absent_record = Game::new(order(50, false));
    let player = absent_record.player();
    absent_record.world_mut().add_item(player, id(LOCKPICK), 1);
    assert!(!absent_record.attempt_lockpick(id(DOOR), player, id(LOCKPICK), 46.2));
    assert!(
        absent_record
            .world_mut()
            .reference(id(DOOR))
            .unwrap()
            .locked
    );

    let mut absent_item = Game::new(order(50, true));
    let player = absent_item.player();
    assert!(!absent_item.attempt_lockpick(id(DOOR), player, id(LOCKPICK), 46.2));
    assert!(absent_item.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(lock_events(&absent_item), 0);
}

#[test]
fn hst_03_picked_lock_state_survives_engine_save_and_load() {
    let mut game = Game::new(order(50, true));
    let player = game.player();
    game.world_mut().add_item(player, id(LOCKPICK), 1);
    assert!(game.attempt_lockpick(id(DOOR), player, id(LOCKPICK), 46.2));

    let saved = game.save_state().unwrap();
    game.world_mut().set_locked(id(DOOR), true, &mut |_, _| {
        panic!("unexpected form allocation")
    });
    game.load_state(&saved).unwrap();
    assert!(!game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert!(game.activate_no_settle(id(DOOR), player));
    assert!(game.world_mut().reference(id(DOOR)).unwrap().opened);
}

#[test]
fn hst_03_repeating_a_success_does_not_emit_another_lock_event_or_spend_a_pick() {
    let mut game = Game::new(order(50, true));
    let player = game.player();
    game.object_of(Handle::Form(id(DOOR)));
    game.world_mut().add_item(player, id(LOCKPICK), 2);

    assert!(
        game.world_mut()
            .resolve_lockpick(id(DOOR), player, true, &mut |_, _| {
                panic!("unexpected form allocation")
            })
    );
    assert_eq!(lock_events(&game), 1);
    game.world_mut().pending.clear();
    assert!(
        !game
            .world_mut()
            .resolve_lockpick(id(DOOR), player, true, &mut |_, _| {
                panic!("unexpected form allocation")
            })
    );

    assert!(!game.world_mut().reference(id(DOOR)).unwrap().locked);
    assert_eq!(game.world_mut().item_count(player, id(LOCKPICK)), 2);
    assert_eq!(lock_events(&game), 0);
}
