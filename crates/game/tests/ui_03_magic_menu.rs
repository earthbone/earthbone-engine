// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Verifier coverage for UI-03's data path: `World::equip_hand_spell` and
//! `World::hand_spell`. A synthetic SPEL record with `SPIT` spell type 0
//! ("Spell", `TES5Edit/Core/wbDefinitionsTES5.pas`) stands in for a
//! known-spell entry the native magic menu would list.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_game::combat::Hand;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const CASTABLE_SPELL: u32 = 0x300;
const POWER_SPELL: u32 = 0x301;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// A minimal SPIT block: byte 8 carries the spell type, 0 for the
/// player-castable "Spell" kind, 2 for "Power" (never hand-equipped).
fn spell(form: u32, spell_type: u32) -> Record {
    let mut data = vec![0; 36];
    put(&mut data, 8, spell_type);
    Record::new(T(b"SPEL"), form).sub(T(b"SPIT"), data)
}

fn world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), 0x500u32.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(spell(CASTABLE_SPELL, 0));
    plugin.push_top(spell(POWER_SPELL, 2));
    let mut order = LoadOrder::new();
    order.add("UiMagic.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn ui_03_selecting_a_known_spell_equips_it_to_the_chosen_hand() {
    let mut world = world();
    world
        .actor(id(ACTOR))
        .unwrap()
        .spells
        .insert(id(CASTABLE_SPELL));

    assert_eq!(world.hand_spell(id(ACTOR), Hand::Right), None);

    world
        .equip_hand_spell(id(ACTOR), Hand::Right, id(CASTABLE_SPELL))
        .expect("a known, castable spell equips to the selected hand");

    assert_eq!(
        world.hand_spell(id(ACTOR), Hand::Right),
        Some(id(CASTABLE_SPELL))
    );
    assert_eq!(world.hand_spell(id(ACTOR), Hand::Left), None);
}

#[test]
fn ui_03_equipping_the_other_hand_leaves_the_first_hand_untouched() {
    let mut world = world();
    world
        .actor(id(ACTOR))
        .unwrap()
        .spells
        .insert(id(CASTABLE_SPELL));
    world
        .equip_hand_spell(id(ACTOR), Hand::Left, id(CASTABLE_SPELL))
        .unwrap();

    world
        .equip_hand_spell(id(ACTOR), Hand::Right, id(CASTABLE_SPELL))
        .unwrap();

    assert_eq!(
        world.hand_spell(id(ACTOR), Hand::Left),
        Some(id(CASTABLE_SPELL))
    );
    assert_eq!(
        world.hand_spell(id(ACTOR), Hand::Right),
        Some(id(CASTABLE_SPELL))
    );
}

#[test]
fn ui_03_a_spell_the_actor_does_not_know_is_rejected() {
    let mut world = world();

    let result = world.equip_hand_spell(id(ACTOR), Hand::Right, id(CASTABLE_SPELL));

    assert!(result.is_err());
    assert_eq!(world.hand_spell(id(ACTOR), Hand::Right), None);
}

#[test]
fn ui_03_a_power_is_not_a_hand_castable_spell() {
    let mut world = world();
    world
        .actor(id(ACTOR))
        .unwrap()
        .spells
        .insert(id(POWER_SPELL));

    let result = world.equip_hand_spell(id(ACTOR), Hand::Right, id(POWER_SPELL));

    assert!(result.is_err());
    assert_eq!(world.hand_spell(id(ACTOR), Hand::Right), None);
}

#[test]
fn ui_03_unequip_hand_spell_clears_only_the_matching_hand() {
    let mut world = world();
    world
        .actor(id(ACTOR))
        .unwrap()
        .spells
        .insert(id(CASTABLE_SPELL));
    world
        .equip_hand_spell(id(ACTOR), Hand::Left, id(CASTABLE_SPELL))
        .unwrap();
    world
        .equip_hand_spell(id(ACTOR), Hand::Right, id(CASTABLE_SPELL))
        .unwrap();

    world
        .unequip_hand_spell(id(ACTOR), Hand::Right, id(CASTABLE_SPELL))
        .unwrap();

    assert_eq!(
        world.hand_spell(id(ACTOR), Hand::Left),
        Some(id(CASTABLE_SPELL)),
        "unequipping the right hand must not clear the left"
    );
    assert_eq!(world.hand_spell(id(ACTOR), Hand::Right), None);
}

/// UI-03's third missing piece: a hand equip must survive an engine save
/// and load, the way a hand weapon already does (`docs/save.md`, version 32).
#[test]
fn ui_03_a_hand_spell_equip_survives_an_engine_save_and_load() {
    let mut before = world();
    before
        .actor(id(ACTOR))
        .unwrap()
        .spells
        .insert(id(CASTABLE_SPELL));
    before
        .equip_hand_spell(id(ACTOR), Hand::Right, id(CASTABLE_SPELL))
        .unwrap();

    let bytes = before.save_state();

    let mut loaded = world();
    loaded
        .actor(id(ACTOR))
        .unwrap()
        .spells
        .insert(id(CASTABLE_SPELL));
    loaded.load_state(&bytes).unwrap();

    assert_eq!(
        loaded.hand_spell(id(ACTOR), Hand::Right),
        Some(id(CASTABLE_SPELL)),
        "a loaded save must restore the equipped hand spell"
    );
    assert_eq!(loaded.hand_spell(id(ACTOR), Hand::Left), None);
}
