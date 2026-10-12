// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-26 pickup ownership, invalid target, and world-save regressions.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"FACT"), 44));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 7).sub(Tag(*b"ACBS"), [0; 24]));
    plugin.push_top(Record::new(Tag(*b"MISC"), 42).zsub(Tag(*b"FULL"), "Owned coin"));
    plugin.push_top(
        Record::new(Tag(*b"REFR"), 43)
            .sub(Tag(*b"NAME"), 42_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), [0; 24])
            .sub(Tag(*b"XOWN"), 44_u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), 46)
            .sub(Tag(*b"NAME"), 7_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), [0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("Loot.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn hst_26_owned_pickup_disables_reference_and_preserves_stolen_stack_after_save() {
    let mut game = game();
    let player = game.player();
    assert!(game.world_mut().loose_item_is_stealing(id(43), player));
    assert!(game.activate(id(43), player));
    assert!(!game.world().reference_ref(id(43)).unwrap().enabled);
    let saved = game.world().save_state();
    let mut loaded = Game::new(game.world().order().clone());
    loaded.world_mut().load_state(&saved).unwrap();
    assert!(!loaded.world().reference_ref(id(43)).unwrap().enabled);
    let stack = &loaded
        .world()
        .reference_ref(player)
        .unwrap()
        .inventory
        .stacks(id(42))[0];
    assert_eq!(stack.count(), 1);
    assert!(stack.extra(33).is_some());
}

#[test]
fn hst_26_missing_and_disabled_targets_do_not_add_inventory() {
    let mut game = game();
    let player = game.player();
    assert!(!game.activate(id(99), player));
    game.world_mut().set_reference_enabled(id(43), false);
    assert!(!game.activate(id(43), player));
    assert_eq!(game.world_mut().item_count(player, id(42)), 0);
}

#[test]
fn hst_26_repeated_pickup_never_duplicates_a_taken_reference() {
    let mut game = game();
    let player = game.player();
    assert!(game.activate(id(43), player));
    assert!(!game.activate(id(43), player));
    assert_eq!(game.world_mut().item_count(player, id(42)), 1);
}
