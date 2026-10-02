// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use soe_esm::records::gameplay::{MagicEffect, Spell};

const TARGET: u32 = 0x103;
const TARGET_BASE: u32 = 0x502;
const OLD_RACE: u32 = 0x700;
const VAMPIRE_RACE: u32 = 0x701;
const VAMPIRE_ABILITY: u32 = 0x304;
const EXISTING_ABILITY: u32 = 0x305;

fn ability_spell(form: u32, effect: u32) -> Record {
    let mut data = vec![0; 36];
    put(&mut data, 8, 4);
    Record::new(T(b"SPEL"), form)
        .sub(T(b"SPIT"), data)
        .sub(T(b"EFID"), effect.to_le_bytes())
        .sub(T(b"EFIT"), [0; 12])
}

fn vampire_game() -> Game {
    let transform = effect_with_associated_item(0, 46, 24, false, VAMPIRE_RACE);
    let mut plugin = plugin(transform, vec![spell(SPELL, 0, false, 0.0, 10)]);
    plugin.push_top(Record::new(T(b"RACE"), OLD_RACE));
    plugin.push_top(
        Record::new(T(b"RACE"), VAMPIRE_RACE).sub(T(b"SPLO"), VAMPIRE_ABILITY.to_le_bytes()),
    );
    let mut actor_config = vec![0; 24];
    put(&mut actor_config, 0, 0);
    plugin.push_top(
        Record::new(T(b"NPC_"), TARGET_BASE)
            .sub(T(b"ACBS"), actor_config)
            .sub(T(b"RNAM"), OLD_RACE.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), TARGET)
            .sub(T(b"NAME"), TARGET_BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let mut ability_effect = effect(0, 0, 24, true);
    ability_effect.form_id = 0x201;
    ability_effect.subrecords[1].data[84..88].copy_from_slice(&0u32.to_le_bytes());
    plugin.push_top(ability_effect);
    plugin.push_top(ability_spell(VAMPIRE_ABILITY, 0x201));
    plugin.push_top(ability_spell(EXISTING_ABILITY, 0x201));
    let game = game(plugin);
    game.world_mut()
        .actor(id(TARGET))
        .unwrap()
        .spells
        .insert(id(EXISTING_ABILITY));
    game
}

fn cast_vampire(game: &mut Game) -> EffectId {
    let shared = game.shared_world();
    let result = shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(ACTOR),
            Some(id(ACTOR)),
            Some(id(TARGET)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap()[0];
    result
}

#[test]
fn mag_67_vampire_lord_uses_authored_race_and_race_abilities_then_restores_owned_state() {
    let mut game = vampire_game();
    let transformation = cast_vampire(&mut game);
    {
        let mut world = game.world_mut();
        {
            let actor = world.actor(id(TARGET)).unwrap();
            assert_eq!(actor.race, Some(id(VAMPIRE_RACE)));
            assert!(actor.spells.contains(&id(VAMPIRE_ABILITY)));
            assert!(actor.spells.contains(&id(EXISTING_ABILITY)));
        }
        assert!(world
            .active_effects()
            .any(|effect| effect.spell == id(VAMPIRE_ABILITY)));
    }

    let saved = game.save_state().unwrap();
    let before_reload = game.world().state_hash();
    let mut restored = vampire_game();
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.world().state_hash(), before_reload);
    let shared = restored.shared_world();
    assert!(shared
        .borrow_mut()
        .dispel_effect(transformation, &mut |name, class| restored
            .vm
            .create_object_of(name, class),));
    let mut world = restored.world_mut();
    let actor = world.actor(id(TARGET)).unwrap();
    assert_eq!(actor.race, Some(id(OLD_RACE)));
    assert!(!actor.spells.contains(&id(VAMPIRE_ABILITY)));
    assert!(actor.spells.contains(&id(EXISTING_ABILITY)));
    assert!(!world
        .active_effects()
        .any(|effect| effect.spell == id(VAMPIRE_ABILITY)));
}

#[test]
fn mag_67_installed_vampire_lord_effects_resolve_their_authored_race() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping MAG-67 installed record check");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let mut count = 0;
    let mut authored_abilities = 0;
    for (id, record) in order.winners_of(T(b"MGEF")) {
        if record.is_deleted() {
            continue;
        }
        let winner = order.winner_ref(id).unwrap();
        let localized = order.file(winner.file).plugin.header.is_localized();
        let effect = MagicEffect::from_record(record, localized).unwrap();
        if effect.data.archetype != 46 {
            continue;
        }
        assert_ne!(effect.data.associated_item, 0, "{id} has no race link");
        let race = order.resolve(winner.file, effect.data.associated_item);
        let race_record = order.winner_ref(race).map(|winner| order.record(winner));
        assert!(race_record.is_some_and(|record| record.tag == T(b"RACE") && !record.is_deleted()));
        let race_winner = order.winner_ref(race).unwrap();
        for raw in race_record.unwrap().all(T(b"SPLO")) {
            assert_eq!(raw.len(), 4, "{race} has a malformed SPLO entry");
            let spell = order.resolve(
                race_winner.file,
                u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
            );
            if let Some(spell_winner) = order.winner_ref(spell) {
                let spell_record = order.record(spell_winner);
                if spell_record.tag == T(b"SPEL") && !spell_record.is_deleted() {
                    let localized = order.file(spell_winner.file).plugin.header.is_localized();
                    if Spell::from_record(spell_record, localized)
                        .is_ok_and(|spell| spell.data.spell_type == 4)
                    {
                        authored_abilities += 1;
                    }
                }
            }
        }
        count += 1;
    }
    assert!(count > 0, "Dragonborn should contain Vampire Lord effects");
    assert!(
        authored_abilities > 0,
        "Vampire Lord race should author ability spells"
    );
}
