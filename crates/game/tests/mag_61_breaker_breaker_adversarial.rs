// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CASTER: u32 = 0x100;
const TARGET: u32 = 0x101;
const BASE: u32 = 0x200;
const EFFECT: u32 = 0x300;
const SPELL: u32 = 0x301;
const HAZARD: u32 = 0x400;
const CONTACT_EFFECT: u32 = 0x500;
const CONTACT_SPELL: u32 = 0x501;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture(malformed_hazard: bool) -> Game {
    let mut magic_effect = vec![0; 152];
    put(&mut magic_effect, 8, HAZARD);
    put(&mut magic_effect, 64, 40);
    put(&mut magic_effect, 80, 1);
    put(&mut magic_effect, 84, 3);
    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 8, 0);
    put(&mut spell_data, 16, 1);
    put(&mut spell_data, 20, 3);
    let mut efit = 20.0f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0u32.to_le_bytes());
    efit.extend_from_slice(&5u32.to_le_bytes());
    let mut hazard_data = vec![0; 40];
    put(&mut hazard_data, 0, 4);
    put(&mut hazard_data, 4, 64.0f32.to_bits());
    put(&mut hazard_data, 8, 3.0f32.to_bits());
    put(&mut hazard_data, 16, 0.25f32.to_bits());
    put(&mut hazard_data, 24, CONTACT_SPELL);

    let mut contact_effect = vec![0; 152];
    put(&mut contact_effect, 0, 1 << 2);
    put(&mut contact_effect, 64, 0);
    put(&mut contact_effect, 68, 24);
    put(&mut contact_effect, 80, 1);
    put(&mut contact_effect, 84, 3);
    let mut contact_spell_data = vec![0; 36];
    put(&mut contact_spell_data, 8, 0);
    put(&mut contact_spell_data, 16, 1);
    put(&mut contact_spell_data, 20, 3);
    let mut contact_efit = 10.0f32.to_le_bytes().to_vec();
    contact_efit.extend_from_slice(&0u32.to_le_bytes());
    contact_efit.extend_from_slice(&0u32.to_le_bytes());

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), BASE).sub(T(b"ACBS"), vec![0; 24]));
    for (form, position) in [(CASTER, [0.0f32, 0.0, 0.0]), (TARGET, [12.0f32, 0.0, 0.0])] {
        let mut data = vec![0; 24];
        for (axis, value) in position.into_iter().enumerate() {
            data[axis * 4..axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        plugin.push_top(
            Record::new(T(b"ACHR"), form)
                .sub(T(b"NAME"), BASE.to_le_bytes())
                .sub(T(b"DATA"), data),
        );
    }
    let hazard = Record::new(T(b"HAZD"), HAZARD).sub(
        T(b"DATA"),
        if malformed_hazard { vec![0; 39] } else { hazard_data },
    );
    plugin.push_top(hazard);
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), magic_effect));
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spell_data)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(T(b"EFIT"), efit),
    );
    plugin.push_top(Record::new(T(b"MGEF"), CONTACT_EFFECT).sub(T(b"DATA"), contact_effect));
    plugin.push_top(
        Record::new(T(b"SPEL"), CONTACT_SPELL)
            .sub(T(b"SPIT"), contact_spell_data)
            .sub(T(b"EFID"), CONTACT_EFFECT.to_le_bytes())
            .sub(T(b"EFIT"), contact_efit),
    );
    let mut order = LoadOrder::new();
    order.add("SpawnHazardBreaker.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [CASTER, TARGET] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
    }
    game
}

fn cast(game: &mut Game) -> Result<Vec<soe_game::world::magic::EffectId>, soe_game::world::magic::MagicError> {
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell(
        id(SPELL),
        id(CASTER),
        Some(id(CASTER)),
        Some(id(TARGET)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    result
}

#[test]
fn mag_61_hazard_periodically_applies_its_contact_spell_to_the_target() {
    let mut game = fixture(false);
    cast(&mut game).unwrap();
    game.tick(0.26);

    assert!(game.world_mut().actor_value(id(TARGET), "health") < 100.0);
    assert_eq!(game.world_mut().actor_value(id(CASTER), "health"), 100.0);
}

#[test]
fn mag_61_malformed_hazard_record_rejects_before_any_effect_or_world_mutation() {
    let mut game = fixture(true);
    let before = game.save_state().unwrap();

    assert!(cast(&mut game).is_err());
    assert_eq!(game.save_state().unwrap(), before);
    assert_eq!(game.world().active_effects().count(), 0);
}
