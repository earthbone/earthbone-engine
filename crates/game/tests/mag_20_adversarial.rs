// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const ACTOR: u32 = 0x100;
const MGEF_A: u32 = 0x200;
const MGEF_B: u32 = 0x201;
const SPELL_A: u32 = 0x300;
const SPELL_B: u32 = 0x301;
const ABILITY: u32 = 0x400;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn effect(form: u32) -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 0, 1 << 1);
    put(&mut data, 16, u32::MAX);
    put(&mut data, 64, 0);
    put(&mut data, 68, 24);
    put(&mut data, 80, 1);
    put(&mut data, 84, 3);
    put(&mut data, 128, ABILITY);
    Record::new(Tag::new(b"MGEF"), form).sub(Tag::new(b"DATA"), data)
}

fn spell(form: u32, effect: u32, duration: u32) -> Record {
    let mut spit = vec![0; 36];
    put(&mut spit, 8, 0);
    put(&mut spit, 16, 1);
    put(&mut spit, 20, 3);
    let mut efit = 1.0f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0u32.to_le_bytes());
    efit.extend_from_slice(&duration.to_le_bytes());
    Record::new(Tag::new(b"SPEL"), form)
        .sub(Tag::new(b"SPIT"), spit)
        .sub(Tag::new(b"EFID"), effect.to_le_bytes())
        .sub(Tag::new(b"EFIT"), efit)
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 0x500).sub(Tag::new(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(Tag::new(b"ACHR"), ACTOR)
            .sub(Tag::new(b"NAME"), 0x500u32.to_le_bytes())
            .sub(Tag::new(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(effect(MGEF_A));
    plugin.push_top(effect(MGEF_B));
    plugin.push_top(spell(SPELL_A, MGEF_A, 2));
    plugin.push_top(spell(SPELL_B, MGEF_B, 5));
    let mut ability_spit = vec![0; 36];
    put(&mut ability_spit, 8, 4);
    put(&mut ability_spit, 20, 0);
    plugin.push_top(Record::new(Tag::new(b"SPEL"), ABILITY).sub(Tag::new(b"SPIT"), ability_spit));
    let mut order = LoadOrder::new();
    order.add("Mag20.esm", plugin).unwrap();
    Game::new(order)
}

fn cast(game: &mut Game, spell: u32) {
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(spell),
            id(ACTOR),
            Some(id(ACTOR)),
            Some(id(ACTOR)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_20_expiry_removes_only_its_own_overlapping_grant() {
    let mut game = game();
    cast(&mut game, SPELL_A);
    cast(&mut game, SPELL_B);
    game.tick(2.0);
    assert!(game.world_mut().has_spell(id(ACTOR), id(ABILITY)));
    assert_eq!(game.world().active_effects().count(), 1);
    game.tick(3.0);
    assert!(!game.world_mut().has_spell(id(ACTOR), id(ABILITY)));
}

#[test]
fn mag_20_overlapping_grants_recompute_after_save_load() {
    let mut original = game();
    cast(&mut original, SPELL_A);
    cast(&mut original, SPELL_B);
    let saved = original.save_state().unwrap();
    let mut restored = game();
    restored.load_state(&saved).unwrap();
    assert!(restored.world_mut().has_spell(id(ACTOR), id(ABILITY)));
    restored.tick(2.0);
    assert!(restored.world_mut().has_spell(id(ACTOR), id(ABILITY)));
    restored.tick(3.0);
    assert!(!restored.world_mut().has_spell(id(ACTOR), id(ABILITY)));
}

#[test]
fn mag_20_same_ability_from_known_spell_survives_all_effect_expiries() {
    let mut game = game();
    game.world_mut()
        .actor(id(ACTOR))
        .unwrap()
        .spells
        .insert(id(ABILITY));
    cast(&mut game, SPELL_A);
    cast(&mut game, SPELL_B);
    game.tick(5.0);
    assert!(game.world_mut().has_spell(id(ACTOR), id(ABILITY)));
    assert_eq!(game.world().active_effects().count(), 0);
}
