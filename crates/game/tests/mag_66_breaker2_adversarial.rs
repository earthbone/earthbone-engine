// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const SOURCE: u32 = 0x100;
const TARGET: u32 = 0x101;
const CASTER: u32 = 0x102;
const GRAB: u32 = 0x200;
const COMPANION: u32 = 0x201;
const SPELL: u32 = 0x300;
const BASE: u32 = 0x500;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn effect_data(archetype: u32, actor_value: u32) -> Vec<u8> {
    let mut data = vec![0; 152];
    put(&mut data, 8, u32::MAX);
    put(&mut data, 64, archetype);
    put(&mut data, 68, actor_value);
    put(&mut data, 80, 2);
    put(&mut data, 84, 0);
    data
}

fn fixture(cast_type: u32, delivery: u32, companion: bool) -> Game {
    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 16, cast_type);
    put(&mut spell_data, 20, delivery);

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE).sub(Tag(*b"ACBS"), vec![0; 24]));
    for actor in [SOURCE, TARGET, CASTER] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor)
                .sub(Tag(*b"NAME"), BASE.to_le_bytes())
                .sub(Tag(*b"DATA"), vec![0; 24]),
        );
    }
    plugin.push_top(Record::new(Tag(*b"MGEF"), GRAB).sub(Tag(*b"DATA"), effect_data(45, 0)));
    if companion {
        plugin
            .push_top(Record::new(Tag(*b"MGEF"), COMPANION).sub(Tag(*b"DATA"), effect_data(0, 0)));
    }
    let mut spell = Record::new(Tag(*b"SPEL"), SPELL)
        .sub(Tag(*b"SPIT"), spell_data)
        .sub(Tag(*b"EFID"), GRAB.to_le_bytes())
        .sub(
            Tag(*b"EFIT"),
            [
                0.0f32.to_le_bytes(),
                0u32.to_le_bytes(),
                30u32.to_le_bytes(),
            ]
            .concat(),
        );
    if companion {
        spell = spell.sub(Tag(*b"EFID"), COMPANION.to_le_bytes()).sub(
            Tag(*b"EFIT"),
            [
                7.0f32.to_le_bytes(),
                0u32.to_le_bytes(),
                30u32.to_le_bytes(),
            ]
            .concat(),
        );
    }
    plugin.push_top(spell);

    let mut order = LoadOrder::new();
    order.add("GrabActorAdversarial.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [SOURCE, TARGET, CASTER] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
    }
    game
}

fn cast(
    game: &mut Game,
) -> Result<Vec<soe_game::world::magic::EffectId>, soe_game::world::magic::MagicError> {
    let shared = game.shared_world();
    shared.borrow_mut().cast_grab_actor_spell(
        id(SPELL),
        id(SOURCE),
        Some(id(CASTER)),
        id(TARGET),
        &mut |name, class| game.vm.create_object_of(name, class),
    )
}

#[test]
fn mag_66_companion_effect_targets_caster_when_source_is_distinct() {
    let mut game = fixture(2, 0, true);
    assert!(cast(&mut game).is_ok());
    assert_eq!(game.world_mut().actor_value(id(CASTER), "health"), 107.0);
    assert_eq!(game.world_mut().actor_value(id(SOURCE), "health"), 100.0);
    assert_eq!(game.world_mut().actor_value(id(TARGET), "health"), 100.0);
}

#[test]
fn mag_66_non_concentration_spell_is_rejected_without_effects() {
    let mut game = fixture(1, 0, false);
    assert!(cast(&mut game).is_err());
    assert!(game.world().active_effects().next().is_none());
}

#[test]
fn mag_66_non_actor_target_is_rejected_without_effects() {
    let mut game = fixture(2, 0, false);
    assert!(game.world_mut().record(id(BASE)).is_some());
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_grab_actor_spell(
        id(SPELL),
        id(SOURCE),
        Some(id(CASTER)),
        id(BASE),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    assert!(result.is_err());
    assert!(game.world().active_effects().next().is_none());
}
