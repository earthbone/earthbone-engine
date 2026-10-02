// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Werewolf transformations use their authored race and own only state they add.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::world::magic::EffectId;

const ACTOR: u32 = 0x100;
const BASE: u32 = 0x200;
const TRANSFORM_EFFECT: u32 = 0x300;
const ABILITY_EFFECT: u32 = 0x301;
const TRANSFORM_SPELL: u32 = 0x400;
const SECOND_TRANSFORM_EFFECT: u32 = 0x302;
const SECOND_TRANSFORM_SPELL: u32 = 0x404;
const ABILITY_SPELL: u32 = 0x401;
const EXISTING_SPELL: u32 = 0x402;
const WEREWOLF_SHOUT: u32 = 0x403;
const PRIOR_RACE: u32 = 0x500;
const WEREWOLF_RACE: u32 = 0x501;
const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn mgef(
    form: u32,
    archetype: u32,
    associated: u32,
    actor_value: u32,
    cast_type: u32,
    delivery: u32,
) -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 8, associated);
    put(&mut data, 16, u32::MAX);
    put(&mut data, 64, archetype);
    put(&mut data, 68, actor_value);
    put(&mut data, 80, cast_type);
    put(&mut data, 84, delivery);
    Record::new(T(b"MGEF"), form).sub(T(b"DATA"), data)
}

fn spell(
    form: u32,
    effect: u32,
    kind: u32,
    cast_type: u32,
    delivery: u32,
    duration: u32,
) -> Record {
    let mut spit = vec![0; 36];
    put(&mut spit, 8, kind);
    put(&mut spit, 16, cast_type);
    put(&mut spit, 20, delivery);
    let mut efit = 0.0f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0u32.to_le_bytes());
    efit.extend_from_slice(&duration.to_le_bytes());
    Record::new(T(b"SPEL"), form)
        .sub(T(b"SPIT"), spit)
        .sub(T(b"EFID"), effect.to_le_bytes())
        .sub(T(b"EFIT"), efit)
}

fn game() -> Game {
    let base = Record::new(T(b"NPC_"), BASE)
        .sub(T(b"ACBS"), vec![0; 24])
        .sub(T(b"RNAM"), PRIOR_RACE.to_le_bytes())
        .sub(T(b"SPLO"), EXISTING_SPELL.to_le_bytes());
    let actor = Record::new(T(b"ACHR"), ACTOR)
        .sub(T(b"NAME"), BASE.to_le_bytes())
        .sub(T(b"DATA"), vec![0; 24]);
    let race = Record::new(T(b"RACE"), WEREWOLF_RACE)
        .sub(T(b"SPLO"), ABILITY_SPELL.to_le_bytes())
        .sub(T(b"SPLO"), WEREWOLF_SHOUT.to_le_bytes())
        .sub(T(b"SPLO"), EXISTING_SPELL.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    for record in [
        base,
        actor,
        Record::new(T(b"RACE"), PRIOR_RACE),
        race,
        Record::new(T(b"SHOU"), WEREWOLF_SHOUT),
        mgef(TRANSFORM_EFFECT, 36, WEREWOLF_RACE, u32::MAX, 1, 3),
        mgef(SECOND_TRANSFORM_EFFECT, 36, WEREWOLF_RACE, u32::MAX, 1, 3),
        mgef(ABILITY_EFFECT, 0, 0, 24, 0, 0),
        spell(TRANSFORM_SPELL, TRANSFORM_EFFECT, 0, 1, 3, 20),
        spell(SECOND_TRANSFORM_SPELL, SECOND_TRANSFORM_EFFECT, 0, 1, 3, 20),
        spell(ABILITY_SPELL, ABILITY_EFFECT, 4, 0, 0, 0),
        spell(EXISTING_SPELL, ABILITY_EFFECT, 0, 1, 3, 0),
    ] {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("MAG57.esm", plugin).unwrap();
    Game::new(order)
}

fn cast(game: &mut Game) -> EffectId {
    cast_spell(game, TRANSFORM_SPELL).unwrap()
}

fn cast_spell(game: &mut Game, spell: u32) -> Result<EffectId, soe_game::world::magic::MagicError> {
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell(
        id(spell),
        id(ACTOR),
        Some(id(ACTOR)),
        Some(id(ACTOR)),
        &mut |name, class| game.vm.create_object_of(name, class),
    )?;
    Ok(result[0])
}

#[test]
fn mag_57_transformation_uses_authored_race_and_grants_its_ability() {
    let mut game = game();
    cast(&mut game);

    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        assert_eq!(actor.race, Some(id(WEREWOLF_RACE)));
        assert!(actor.spells.contains(&id(ABILITY_SPELL)));
        assert!(actor.spells.contains(&id(EXISTING_SPELL)));
        assert!(actor.shouts.contains(&id(WEREWOLF_SHOUT)));
    }
    let active_spells: Vec<_> = game
        .world()
        .active_effects()
        .map(|effect| effect.spell)
        .collect();
    assert!(
        active_spells.contains(&id(ABILITY_SPELL)),
        "{active_spells:?}"
    );
}

#[test]
fn mag_57_reversion_restores_only_race_and_abilities_owned_by_transform() {
    let mut game = game();
    let transform = cast(&mut game);
    let object = game.object_of_form(id(ACTOR));
    assert!(
        game.world_mut()
            .dispel_effect(transform, &mut |_, _| object)
    );

    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        assert_eq!(actor.race, Some(id(PRIOR_RACE)));
        assert!(!actor.spells.contains(&id(ABILITY_SPELL)));
        assert!(actor.spells.contains(&id(EXISTING_SPELL)));
        assert!(!actor.shouts.contains(&id(WEREWOLF_SHOUT)));
    }
    assert!(
        !game
            .world()
            .active_effects()
            .any(|effect| effect.spell == id(ABILITY_SPELL))
    );
}

#[test]
fn mag_57_timed_expiry_reverts_the_authored_race_and_added_ability() {
    let mut game = game();
    cast(&mut game);

    game.tick(20.0);

    let mut world = game.world_mut();
    let actor = world.actor(id(ACTOR)).unwrap();
    assert_eq!(actor.race, Some(id(PRIOR_RACE)));
    assert!(!actor.spells.contains(&id(ABILITY_SPELL)));
    assert!(!actor.shouts.contains(&id(WEREWOLF_SHOUT)));
}

#[test]
fn mag_57_saved_transform_restores_owned_state_after_load() {
    let mut original = game();
    let transform = cast(&mut original);
    let saved = original.save_state().unwrap();
    let mut restored = game();
    restored.load_state(&saved).unwrap();

    assert_eq!(
        restored.world_mut().actor(id(ACTOR)).unwrap().race,
        Some(id(WEREWOLF_RACE))
    );
    let object = restored.object_of_form(id(ACTOR));
    assert!(
        restored
            .world_mut()
            .dispel_effect(transform, &mut |_, _| object)
    );
    {
        let mut world = restored.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        assert_eq!(actor.race, Some(id(PRIOR_RACE)));
        assert!(!actor.spells.contains(&id(ABILITY_SPELL)));
        assert!(actor.spells.contains(&id(EXISTING_SPELL)));
        assert!(!actor.shouts.contains(&id(WEREWOLF_SHOUT)));
    }
}

#[test]
fn mag_57_rejected_overlapping_transform_does_not_change_saved_world_state() {
    let mut game = game();
    cast(&mut game);
    assert!(cast_spell(&mut game, SECOND_TRANSFORM_SPELL).is_err());

    assert_eq!(
        game.world_mut().actor(id(ACTOR)).unwrap().race,
        Some(id(WEREWOLF_RACE))
    );
    assert_eq!(game.world().active_effects().count(), 2);
}
