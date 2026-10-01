// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! MAG-73 adversarial checks for interruption scope and repeat calls.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), ACTOR));
    let mut order = LoadOrder::new();
    order.add("Mag73.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn mag_73_repeated_interrupt_and_interrupt_without_cast_are_noops() {
    let mut game = game();
    let player = game.player();
    let actor = game.world_mut().place_at(id(ACTOR), player, false);
    assert!(!game.world_mut().interrupt_spell_cast(actor));
    assert!(!game.world_mut().interrupt_spell_cast(actor));
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_73_interrupting_one_reference_does_not_cancel_another_cast() {
    let mut spell = Record::new(T(b"SPEL"), 0x200);
    let mut data = vec![0; 36];
    data[16..20].copy_from_slice(&1u32.to_le_bytes());
    spell = spell.sub(T(b"SPIT"), data);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), ACTOR));
    plugin.push_top(spell);
    let mut order = LoadOrder::new();
    order.add("Mag73.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let (first, second) = {
        let mut world = game.world_mut();
        (
            world.place_at(id(ACTOR), player, false),
            world.place_at(id(ACTOR), player, false),
        )
    };
    game.world_mut()
        .begin_spell_cast(first, id(0x200), None)
        .unwrap();
    game.world_mut()
        .begin_spell_cast(second, id(0x200), None)
        .unwrap();
    game.world_mut().interrupt_spell_cast(first);
    assert!(game.world().pending_spell_cast(first).is_none());
    assert!(game.world().pending_spell_cast(second).is_some());
}
