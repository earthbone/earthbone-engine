// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::magic::EffectId;
use soe_game::world::Pending;
use soe_game::Game;

const CASTER: u32 = 0x100;
const TARGET: u32 = 0x101;
const BASE: u32 = 0x200;
const TRANSFORM: u32 = 0x300;
const ABILITY_EFFECT: u32 = 0x301;
const SPELL: u32 = 0x400;
const ABILITY: u32 = 0x401;
const OLD_RACE: u32 = 0x500;
const VAMPIRE_RACE: u32 = 0x501;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn mgef(form: u32, archetype: u32, associated: u32) -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 8, associated);
    put(&mut data, 16, u32::MAX);
    put(&mut data, 64, archetype);
    put(&mut data, 68, 24);
    put(&mut data, 80, if archetype == 0 { 0 } else { 1 });
    put(&mut data, 84, if form == ABILITY_EFFECT { 0 } else { 3 });
    Record::new(Tag::new(b"MGEF"), form).sub(Tag::new(b"DATA"), data)
}

fn spell(form: u32, effect: u32, spell_type: u32, cast_type: u32, delivery: u32) -> Record {
    let mut spit = vec![0; 36];
    put(&mut spit, 8, spell_type);
    put(&mut spit, 16, cast_type);
    put(&mut spit, 20, delivery);
    Record::new(Tag::new(b"SPEL"), form)
        .sub(Tag::new(b"SPIT"), spit)
        .sub(Tag::new(b"EFID"), effect.to_le_bytes())
        .sub(Tag::new(b"EFIT"), [0; 12])
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"NPC_"), BASE)
            .sub(Tag::new(b"ACBS"), vec![0; 24])
            .sub(Tag::new(b"RNAM"), OLD_RACE.to_le_bytes()),
    );
    for actor in [CASTER, TARGET] {
        plugin.push_top(
            Record::new(Tag::new(b"ACHR"), actor)
                .sub(Tag::new(b"NAME"), BASE.to_le_bytes())
                .sub(Tag::new(b"DATA"), vec![0; 24]),
        );
    }
    plugin.push_top(Record::new(Tag::new(b"RACE"), OLD_RACE));
    plugin.push_top(
        Record::new(Tag::new(b"RACE"), VAMPIRE_RACE).sub(Tag::new(b"SPLO"), ABILITY.to_le_bytes()),
    );
    plugin.push_top(mgef(TRANSFORM, 46, VAMPIRE_RACE));
    plugin.push_top(mgef(ABILITY_EFFECT, 0, 0));
    plugin.push_top(spell(SPELL, TRANSFORM, 0, 1, 3));
    plugin.push_top(spell(ABILITY, ABILITY_EFFECT, 4, 0, 0));
    let mut order = LoadOrder::new();
    order.add("Mag67.esm", plugin).unwrap();
    Game::new(order)
}

fn cast_transformation(game: &mut Game) -> EffectId {
    let shared = game.shared_world();
    let result = shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(CASTER),
            Some(id(CASTER)),
            Some(id(TARGET)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap()[0];
    result
}

#[test]
fn mag_67_racial_ability_binds_and_receives_effect_start_on_transformation() {
    let mut game = game();
    cast_transformation(&mut game);

    assert_eq!(
        game.world_mut().actor(id(TARGET)).unwrap().race,
        Some(id(VAMPIRE_RACE))
    );
    assert!(game.world_mut().actor(id(TARGET)).unwrap().spells.contains(&id(ABILITY)));
    assert!(game
        .world()
        .active_effects()
        .any(|effect| effect.target == id(TARGET) && effect.spell == id(ABILITY)));
    let ability_object = game.world().pending.iter().find_map(|pending| match pending {
        Pending::BindEffect { object, effect } if *effect == id(ABILITY_EFFECT) => Some(*object),
        _ => None,
    });
    let Some(ability_object) = ability_object else {
        panic!("racial ability did not bind its ActiveMagicEffect object");
    };
    assert!(game.world().pending.iter().any(|pending| {
        matches!(pending, Pending::Event { object, name, .. }
            if *object == ability_object && name == "OnEffectStart")
    }));
}

#[test]
fn mag_67_transformation_reverts_owned_race_and_ability_after_save_load() {
    let mut original = game();
    let transformation = cast_transformation(&mut original);
    let saved = original.save_state().unwrap();
    let mut restored = game();
    restored.load_state(&saved).unwrap();

    assert_eq!(
        restored.world_mut().actor(id(TARGET)).unwrap().race,
        Some(id(VAMPIRE_RACE))
    );
    let shared = restored.shared_world();
    assert!(shared.borrow_mut().dispel_effect(transformation, &mut |name, class| {
        restored.vm.create_object_of(name, class)
    }));
    let mut world = restored.world_mut();
    let actor = world.actor(id(TARGET)).unwrap();
    assert_eq!(actor.race, Some(id(OLD_RACE)));
    assert!(!actor.spells.contains(&id(ABILITY)));
    assert!(!world
        .active_effects()
        .any(|effect| effect.target == id(TARGET) && effect.spell == id(ABILITY)));
}
