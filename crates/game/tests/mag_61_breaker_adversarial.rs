// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::magic::EffectId;
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CASTER: u32 = 0x100;
const TARGET: u32 = 0x101;
const BASE: u32 = 0x200;
const EFFECT: u32 = 0x300;
const SPELL: u32 = 0x301;
const HAZARD: u32 = 0x400;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture(association: u32, include_hazard: bool) -> Game {
    let mut magic_effect = vec![0; 152];
    put(&mut magic_effect, 8, association);
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
    put(&mut hazard_data, 20, 0x0a);

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), BASE).sub(T(b"ACBS"), vec![0; 24]));
    for (form, position) in [(CASTER, [0.0f32, 0.0, 0.0]), (TARGET, [12.0, 0.0, 0.0])] {
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
    if include_hazard {
        plugin.push_top(Record::new(T(b"HAZD"), HAZARD).sub(T(b"DATA"), hazard_data));
    }
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), magic_effect));
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spell_data)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(T(b"EFIT"), efit),
    );
    let mut order = LoadOrder::new();
    order.add("SpawnHazard.esm", plugin).unwrap();
    Game::new(order)
}

fn game() -> Game {
    fixture(HAZARD, true)
}

fn cast(game: &mut Game) -> Result<Vec<EffectId>, soe_game::world::magic::MagicError> {
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

fn spawned_reference(game: &Game, effect: EffectId) -> GlobalFormId {
    game.world()
        .active_effects()
        .find(|active| active.id == effect)
        .unwrap()
        .summoned
        .unwrap()
}

#[test]
fn mag_61_saved_hazard_loads_and_finishes_its_remaining_lifetime() {
    let mut original = game();
    let effect = cast(&mut original).unwrap()[0];
    let hazard = spawned_reference(&original, effect);
    original.tick(2.0);
    let saved = original.save_state().unwrap();

    let mut restored = game();
    restored.load_state(&saved).unwrap();
    assert!(restored.world().reference_ref(hazard).unwrap().enabled);
    restored.tick(3.0);
    let world = restored.world();
    let state = world.reference_ref(hazard).unwrap();
    assert!(!state.enabled);
    assert!(state.deleted);
    assert_eq!(restored.world().active_effects().count(), 0);
}

#[test]
fn mag_61_missing_associated_hazard_rejects_cast_without_partial_state() {
    let mut missing_record = fixture(HAZARD, false);
    let confidence = missing_record
        .world_mut()
        .actor_value(id(TARGET), "confidence");
    assert!(cast(&mut missing_record).is_err());
    assert_eq!(missing_record.world().active_effects().count(), 0);
    assert_eq!(
        missing_record
            .world_mut()
            .actor_value(id(TARGET), "confidence"),
        confidence
    );

    let mut missing_association = fixture(0, true);
    let confidence = missing_association
        .world_mut()
        .actor_value(id(TARGET), "confidence");
    assert!(cast(&mut missing_association).is_err());
    assert_eq!(missing_association.world().active_effects().count(), 0);
    assert_eq!(
        missing_association
            .world_mut()
            .actor_value(id(TARGET), "confidence"),
        confidence
    );
}

#[test]
fn mag_61_dispelling_one_of_two_hazards_keeps_the_other_live() {
    let mut game = game();
    let effects = cast(&mut game).unwrap();
    let first = spawned_reference(&game, effects[0]);
    let second_effect = cast(&mut game).unwrap()[0];
    let second = spawned_reference(&game, second_effect);
    assert_ne!(first, second);

    let shared = game.shared_world();
    assert!(shared
        .borrow_mut()
        .dispel_effect(effects[0], &mut |name, class| {
            game.vm.create_object_of(name, class)
        }));
    let world = shared.borrow();
    assert!(world.reference_ref(first).unwrap().deleted);
    assert!(!world.reference_ref(second).unwrap().deleted);
    assert!(world.reference_ref(second).unwrap().enabled);
    assert_eq!(world.active_effects().count(), 1);
}
