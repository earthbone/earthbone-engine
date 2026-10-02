// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! UI-08 lockpick transaction save and load checks.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const DOOR: u32 = 0x100;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut lock = vec![0; 20];
    lock[0] = 25;
    for record in [
        Record::new(T(b"NPC_"), 7).zsub(T(b"EDID"), "Player"),
        Record::new(T(b"DOOR"), 0x10),
        Record::new(T(b"MISC"), 0x200).zsub(T(b"EDID"), "Lockpick"),
        Record::new(T(b"REFR"), DOOR)
            .sub(T(b"NAME"), 0x10_u32.to_le_bytes())
            .sub(T(b"XLOC"), lock),
    ] {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Ui08SaveRoundTrip.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let pick = game.world().form("Lockpick").unwrap();
    game.world_mut().add_item(player, pick, 2);
    game
}

#[test]
fn ui_08_successful_pick_keeps_the_unlocked_door_and_inventory_after_reload() {
    let mut original = game();
    let player = original.player();
    let pick = original.world().form("Lockpick").unwrap();
    let target = id(DOOR);
    let level = original.world_mut().reference(target).unwrap().lock_level;
    let skill = original.world_mut().actor_value(player, "lockpicking");
    let angle = (u64::from(target.object) * 37 % 901) as f32 / 10.0;

    assert!(soe_game::lockpicking::angle_succeeds(
        target, level, skill, angle
    ));
    assert!(original.attempt_lockpick(target, player, pick, angle));
    let saved = original.world().save_state();

    let restored = game();
    restored.world_mut().load_state(&saved).unwrap();
    assert!(!restored.world_mut().reference(target).unwrap().locked);
    assert_eq!(restored.world_mut().item_count(player, pick), 2);
}

#[test]
fn ui_08_failed_pick_keeps_lock_and_one_consumed_pick_after_reload() {
    let mut original = game();
    let player = original.player();
    let pick = original.world().form("Lockpick").unwrap();
    let target = id(DOOR);

    assert!(!original.attempt_lockpick(target, player, pick, 0.0));
    assert!(original.world_mut().reference(target).unwrap().locked);
    assert_eq!(original.world_mut().item_count(player, pick), 1);
    let saved = original.world().save_state();

    let restored = game();
    restored.world_mut().load_state(&saved).unwrap();
    assert!(restored.world_mut().reference(target).unwrap().locked);
    assert_eq!(restored.world_mut().item_count(player, pick), 1);
}
