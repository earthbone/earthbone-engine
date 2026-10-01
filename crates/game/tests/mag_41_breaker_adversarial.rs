// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/magic_adversarial.rs"]
mod support;

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use support::*;

const ITEM: u32 = 0x220;
const TELEKINESIS: u32 = 0x300;
const SPELL: u32 = 0x310;

fn game_with_telekinesis() -> soe_game::Game {
    let mut effect = effect(TELEKINESIS, 20, 0, 0);
    put(effect.subrecords[0].data.as_mut_slice(), 80, 2);
    put(effect.subrecords[0].data.as_mut_slice(), 84, 0);
    let mut spell = spell(SPELL, TELEKINESIS, 0, 1.0, 2);
    put(spell.subrecords[0].data.as_mut_slice(), 20, 0);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE).sub(Tag(*b"ACBS"), vec![0; 24]));
    plugin.push_top(Record::new(Tag(*b"STAT"), ITEM));
    plugin.push_top(actor(CASTER, [0.0; 3]));
    plugin.push_top(effect);
    plugin.push_top(spell);
    let mut order = LoadOrder::new();
    order.add("TelekinesisBreaker.esm", plugin).unwrap();
    let game = soe_game::Game::new(order);
    game.world_mut()
        .set_actor_value(id(CASTER), "magicka", 100.0)
        .unwrap();
    game
}

fn begin_telekinesis(game: &mut soe_game::Game) {
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .begin_concentration_cast(
            id(SPELL),
            id(CASTER),
            id(CASTER),
            None,
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_41_cannot_grab_before_the_telekinesis_cast_starts() {
    let game = game_with_telekinesis();
    let item = game.world_mut().place_at(id(ITEM), id(CASTER), false);
    assert!(!game
        .world_mut()
        .grab_telekinesis_reference(id(CASTER), item));
    assert_eq!(game.world().telekinesis_reference(id(CASTER)), None);
}

#[test]
fn mag_41_rejects_disabled_references_and_actor_references() {
    let mut game = game_with_telekinesis();
    begin_telekinesis(&mut game);
    let item = game.world_mut().place_at(id(ITEM), id(CASTER), false);
    game.world_mut().reference(item).unwrap().enabled = false;
    assert!(!game
        .world_mut()
        .grab_telekinesis_reference(id(CASTER), item));
    assert!(!game
        .world_mut()
        .grab_telekinesis_reference(id(CASTER), id(CASTER)));
    assert_eq!(game.world().telekinesis_reference(id(CASTER)), None);
}

#[test]
fn mag_41_a_rejected_replacement_cast_preserves_the_current_hold() {
    let mut game = game_with_telekinesis();
    begin_telekinesis(&mut game);
    let item = game.world_mut().place_at(id(ITEM), id(CASTER), false);
    assert!(game
        .world_mut()
        .grab_telekinesis_reference(id(CASTER), item));
    let shared = game.shared_world();
    assert!(shared
        .borrow_mut()
        .begin_concentration_cast(
            id(0x3ff),
            id(CASTER),
            id(CASTER),
            None,
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .is_err());
    assert_eq!(game.world().telekinesis_reference(id(CASTER)), Some(item));
}

#[test]
fn mag_41_releasing_the_cast_releases_the_object_and_allows_a_new_hold() {
    let mut game = game_with_telekinesis();
    begin_telekinesis(&mut game);
    let first = game.world_mut().place_at(id(ITEM), id(CASTER), false);
    let second = game.world_mut().place_at(id(ITEM), id(CASTER), false);
    assert!(game
        .world_mut()
        .grab_telekinesis_reference(id(CASTER), first));
    assert!(game.world_mut().release_concentration_cast());
    assert_eq!(game.world().telekinesis_reference(id(CASTER)), None);
    begin_telekinesis(&mut game);
    assert!(game
        .world_mut()
        .grab_telekinesis_reference(id(CASTER), second));
    assert_eq!(game.world().telekinesis_reference(id(CASTER)), Some(second));
}
