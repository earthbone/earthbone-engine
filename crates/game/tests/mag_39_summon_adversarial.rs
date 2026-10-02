// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::magic::{EffectId, MagicError};
use soe_game::world::World;
use soe_game::Game;
use soe_papyrus::asm::ScriptBuilder;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const SOURCE: u32 = 0x100;
const TARGET: u32 = 0x101;
const CASTER: u32 = 0x102;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const BASE: u32 = 0x500;
const SUMMON: u32 = 0x501;
const SUMMONABLE: u32 = 0x4000;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture(summon_flags: Option<u32>, delivery: u32, duration: u32) -> Game {
    let mut effect_data = vec![0; 152];
    put(&mut effect_data, 8, SUMMON);
    put(&mut effect_data, 16, u32::MAX);
    put(&mut effect_data, 64, 18);
    put(&mut effect_data, 80, 1);
    put(&mut effect_data, 84, delivery);

    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 8, 0);
    put(&mut spell_data, 16, 1);
    put(&mut spell_data, 20, delivery);

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), BASE).sub(T(b"ACBS"), vec![0; 24]));
    if let Some(summon_flags) = summon_flags {
        let mut summon_acbs = vec![0; 24];
        put(&mut summon_acbs, 0, summon_flags);
        plugin.push_top(Record::new(T(b"NPC_"), SUMMON).sub(T(b"ACBS"), summon_acbs));
    }
    for actor in [SOURCE, TARGET, CASTER] {
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), BASE.to_le_bytes())
                .sub(T(b"DATA"), vec![0; 24]),
        );
    }
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), effect_data));
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spell_data)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(
                T(b"EFIT"),
                [
                    0.0f32.to_le_bytes(),
                    0u32.to_le_bytes(),
                    duration.to_le_bytes(),
                ]
                .concat(),
            ),
    );

    let mut order = LoadOrder::new();
    order.add("Breaker.esm", plugin).unwrap();
    let mut game = Game::new(order);
    for actor in [SOURCE, TARGET, CASTER] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
    }
    game.load_script(&ScriptBuilder::new("EffectProbe", "ActiveMagicEffect").bytes())
        .unwrap();
    game.bind();
    game
}

fn cast(
    game: &mut Game,
    caster: Option<u32>,
    target: Option<u32>,
) -> Result<Vec<EffectId>, MagicError> {
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell(
        id(SPELL),
        id(SOURCE),
        caster.map(id),
        target.map(id),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    result
}

fn active_summon(world: &World, effect: EffectId) -> GlobalFormId {
    world
        .active_effects()
        .find(|active| active.id == effect)
        .unwrap()
        .summoned
        .unwrap()
}

#[test]
fn mag_39_rejects_an_actor_base_without_the_summonable_flag() {
    let mut game = fixture(Some(0), 0, 10);

    let result = cast(&mut game, Some(CASTER), None);

    assert!(
        result.is_err(),
        "MGEF archetype 18 requires a Summonable NPC base"
    );
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_39_uses_the_credited_caster_when_caster_differs_from_target() {
    let mut game = fixture(Some(SUMMONABLE), 3, 10);

    let effects = cast(&mut game, Some(CASTER), Some(TARGET)).unwrap();
    let spawned = active_summon(&game.world(), effects[0]);

    assert_eq!(
        game.world().reference_ref(spawned).unwrap().owner,
        Some(id(CASTER))
    );
}

#[test]
fn mag_39_repeated_casts_have_independent_expiry() {
    let mut game = fixture(Some(SUMMONABLE), 0, 2);

    let first = cast(&mut game, Some(CASTER), None).unwrap()[0];
    let second = cast(&mut game, Some(CASTER), None).unwrap()[0];
    let first_summon = active_summon(&game.world(), first);
    let second_summon = active_summon(&game.world(), second);
    assert_ne!(first_summon, second_summon);

    game.tick(2.0);

    let world = game.world();
    for summon in [first_summon, second_summon] {
        let reference = world.reference_ref(summon).unwrap();
        assert!(reference.deleted);
        assert!(!reference.enabled);
    }
    assert_eq!(world.active_effects().count(), 0);
}

#[test]
fn mag_39_dispelling_an_effect_dismisses_its_actor_once() {
    let mut game = fixture(Some(SUMMONABLE), 0, 30);

    let effect = cast(&mut game, Some(CASTER), None).unwrap()[0];
    let summon = active_summon(&game.world(), effect);
    let shared = game.shared_world();
    let dismissed = shared
        .borrow_mut()
        .dispel_effect(effect, &mut |name, class| {
            game.vm.create_object_of(name, class)
        });
    assert!(dismissed);

    let world = game.world();
    let reference = world.reference_ref(summon).unwrap();
    assert!(reference.deleted);
    assert!(!reference.enabled);
    drop(world);
    let shared = game.shared_world();
    let dismissed_again = shared
        .borrow_mut()
        .dispel_effect(effect, &mut |name, class| {
            game.vm.create_object_of(name, class)
        });
    assert!(!dismissed_again);
}

#[test]
fn mag_39_missing_associated_actor_does_not_create_an_active_effect() {
    let mut game = fixture(None, 0, 10);

    assert!(cast(&mut game, Some(CASTER), None).is_err());
    assert_eq!(game.world().active_effects().count(), 0);
}
