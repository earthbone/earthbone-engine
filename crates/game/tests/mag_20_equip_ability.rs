// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! MGEF equip abilities remain granted only while their source effect lives.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const ABILITY: u32 = 0x400;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn effect() -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 16, u32::MAX);
    put(&mut data, 64, 0);
    put(&mut data, 68, 24);
    put(&mut data, 80, 1);
    put(&mut data, 84, 3);
    put(&mut data, 128, ABILITY);
    Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), data)
}

fn spell(form: u32, duration: u32) -> Record {
    let mut data = vec![0; 36];
    put(&mut data, 16, 1);
    put(&mut data, 20, 3);
    let mut efit = 1.0f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0u32.to_le_bytes());
    efit.extend_from_slice(&duration.to_le_bytes());
    Record::new(T(b"SPEL"), form)
        .sub(T(b"SPIT"), data)
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(T(b"EFIT"), efit)
}

fn ability() -> Record {
    let mut data = vec![0; 36];
    put(&mut data, 8, 4);
    put(&mut data, 16, 0);
    put(&mut data, 20, 0);
    Record::new(T(b"SPEL"), ABILITY)
        .sub(T(b"SPIT"), data)
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(T(b"EFIT"), [0; 12])
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), 0x500u32.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(effect());
    plugin.push_top(spell(SPELL, 3));
    plugin.push_top(ability());
    let mut order = LoadOrder::new();
    order.add("MAG20.esm", plugin).unwrap();
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
fn mag_20_effect_grants_its_authored_equip_ability() {
    let mut game = game();
    cast(&mut game, SPELL);

    assert!(game.world_mut().has_spell(id(ACTOR), id(ABILITY)));
    assert!(
        !game
            .world_mut()
            .actor(id(ACTOR))
            .unwrap()
            .spells
            .contains(&id(ABILITY))
    );
}

#[test]
fn mag_20_expiry_removes_its_equip_ability_grant() {
    let mut game = game();
    cast(&mut game, SPELL);
    assert!(game.world_mut().has_spell(id(ACTOR), id(ABILITY)));

    game.tick(3.0);

    assert!(!game.world_mut().has_spell(id(ACTOR), id(ABILITY)));
}

#[test]
fn mag_20_expiring_grant_preserves_independently_owned_spell() {
    let mut game = game();
    game.world_mut()
        .actor(id(ACTOR))
        .unwrap()
        .spells
        .insert(id(ABILITY));
    cast(&mut game, SPELL);

    game.tick(3.0);

    assert!(game.world_mut().has_spell(id(ACTOR), id(ABILITY)));
    assert_eq!(game.world().active_effects().count(), 0);
}
