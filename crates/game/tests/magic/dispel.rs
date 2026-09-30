// SPDX-License-Identifier: GPL-3.0-or-later

//! CK Dispel archetype selection, ordered application and one-time finish events.

use super::*;

const CONSTANT: u32 = EFFECT + 1;
const DISPEL: u32 = EFFECT + 2;
const CONSTANT_SPELL: u32 = SPELL + 1;
const DISPEL_SPELL: u32 = SPELL + 2;
const UNMATCHED: u32 = EFFECT + 3;
const KEYWORD_DISPEL: u32 = EFFECT + 4;
const MIXED_SPELL: u32 = SPELL + 4;

fn linked_spell(form: u32, base: u32, constant: bool, magnitude: f32, duration: u32) -> Record {
    let mut record = spell(
        form,
        if constant { 4 } else { 0 },
        constant,
        magnitude,
        duration,
    );
    record
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"EFID"))
        .unwrap()
        .data = base.to_le_bytes().to_vec();
    record
}

fn dispel_fixture(magnitude: f32) -> Plugin {
    let mut fixture = plugin(
        effect(2, 0, 24, false),
        vec![spell(SPELL, 0, false, 20.0, 10)],
    );
    let mut constant = effect(2, 0, 24, true);
    constant.form_id = CONSTANT;
    fixture.push_top(constant);
    let mut dispelling = effect(0, 2, u32::MAX, false);
    dispelling.form_id = DISPEL;
    fixture.push_top(dispelling);
    fixture.push_top(linked_spell(CONSTANT_SPELL, CONSTANT, true, 5.0, 0));
    fixture.push_top(linked_spell(DISPEL_SPELL, DISPEL, false, magnitude, 5));
    fixture
}

fn keyword_dispel_fixture() -> Plugin {
    let mut fixture = plugin(
        effect(2, 0, 24, false),
        vec![spell(MIXED_SPELL, 0, false, 20.0, 10)],
    );
    let mut unmatched = effect(2, 0, 25, false);
    unmatched.form_id = UNMATCHED;
    unmatched.subrecords.retain(|sub| sub.tag != T(b"KWDA"));
    fixture.push_top(unmatched);
    let mut dispeller = effect(1 << 8, 1, u32::MAX, false);
    dispeller.form_id = KEYWORD_DISPEL;
    fixture.push_top(dispeller);
    let mixed = fixture
        .records
        .iter_mut()
        .find(|record| record.form_id == MIXED_SPELL)
        .unwrap();
    mixed
        .subrecords
        .push(soe_esm::Subrecord::new(T(b"EFID"), UNMATCHED.to_le_bytes()));
    mixed.subrecords.push(soe_esm::Subrecord::new(
        T(b"EFIT"),
        [
            20.0f32.to_le_bytes().as_slice(),
            &0u32.to_le_bytes(),
            &10u32.to_le_bytes(),
        ]
        .concat(),
    ));
    fixture.push_top(linked_spell(DISPEL_SPELL, KEYWORD_DISPEL, false, 0.0, 5));
    fixture
}

#[test]
fn mag_07_keyword_dispel_finishes_only_matching_effect_once() {
    let mut game = game(keyword_dispel_fixture());
    let victims = cast(&mut game, MIXED_SPELL, OTHER).unwrap();
    let other_actor = cast(&mut game, MIXED_SPELL, ACTOR).unwrap();
    game.run_until_idle(64);
    let matching_object = game.object_of(Handle::ActiveEffect(victims[0]));
    retain_effect(&mut game, matching_object);
    cast(&mut game, DISPEL_SPELL, OTHER).unwrap();
    assert_eq!(
        game.world()
            .pending
            .iter()
            .filter(|event| matches!(event, Pending::Event { object, name, .. } if *object == matching_object && name == "OnEffectFinish"))
            .count(),
        1
    );
    assert!(!game
        .world()
        .active_effects()
        .any(|effect| effect.id == victims[0]));
    assert!(game
        .world()
        .active_effects()
        .any(|effect| effect.id == victims[1]));
    assert!(game
        .world()
        .active_effects()
        .any(|effect| effect.id == other_actor[0]));
    game.run_until_idle(64);
    assert_eq!(
        game.vm.get_var(matching_object, "EffectProbe", "finished"),
        Some(Value::Int(1))
    );
    cast(&mut game, DISPEL_SPELL, OTHER).unwrap();
    game.run_until_idle(64);
    assert_eq!(
        game.vm.get_var(matching_object, "EffectProbe", "finished"),
        Some(Value::Int(1))
    );
}

#[test]
fn mag_07_ingestible_keyword_dispel_selects_matching_effect() {
    let mut fixture = keyword_dispel_fixture();
    let mut potion = ingestible(POTION, 0.0, 5);
    potion
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"EFID"))
        .unwrap()
        .data = KEYWORD_DISPEL.to_le_bytes().to_vec();
    fixture.push_top(potion);
    let mut game = game(fixture);
    let victims = cast(&mut game, MIXED_SPELL, OTHER).unwrap();
    game.world_mut().add_item(id(OTHER), id(POTION), 1);
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .drink_ingestible(id(POTION), id(OTHER), &mut |name, class| {
            game.vm.create_object_of(name, class)
        })
        .unwrap();
    assert!(!game
        .world()
        .active_effects()
        .any(|effect| effect.id == victims[0]));
    assert!(game
        .world()
        .active_effects()
        .any(|effect| effect.id == victims[1]));
    assert_eq!(game.world_mut().item_count(id(OTHER), id(POTION)), 0);
}

#[test]
fn mag_05_dispel_finishes_timed_effects_once_preserving_constants_and_learned_spells() {
    for magnitude in [0.0, 1000.0] {
        let mut game = game(dispel_fixture(magnitude));
        game.world_mut()
            .actor(id(OTHER))
            .unwrap()
            .spells
            .insert(id(SPELL));
        let timed = cast(&mut game, SPELL, OTHER).unwrap()[0];
        let other_actor = cast(&mut game, SPELL, ACTOR).unwrap()[0];
        let constant = cast(&mut game, CONSTANT_SPELL, OTHER).unwrap()[0];
        game.run_until_idle(64);
        let object = game.object_of(Handle::ActiveEffect(timed));
        let dispelling = Value::Object(game.object_of_form(id(DISPEL_SPELL)));
        let source = Value::Object(game.object_of_form(id(ACTOR)));
        let target = Value::Object(game.object_of_form(id(OTHER)));
        native(&mut game, dispelling, "Cast", &[source, target]);
        assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 105.0);
        assert!(game
            .world()
            .active_effects()
            .any(|effect| effect.id == constant));
        assert!(game
            .world()
            .active_effects()
            .any(|effect| effect.id == other_actor));
        assert!(game
            .world()
            .actor_ref(id(OTHER))
            .unwrap()
            .spells
            .contains(&id(SPELL)));
        // MAG-26 releases a dispelled effect's object once its OnEffectFinish
        // has drained, so the probe is unreachable rather than readable. The
        // release itself is the evidence the callback ran and completed.
        assert_eq!(game.vm.get_var(object, "EffectProbe", "finished"), None);
        assert!(!game
            .world()
            .active_effects()
            .any(|effect| effect.id == timed));
        cast(&mut game, DISPEL_SPELL, OTHER).unwrap();
        game.tick(20.0);
        // A second dispel finds nothing to finish, so nothing is rebound and
        // the released handle stays released.
        assert_eq!(game.vm.get_var(object, "EffectProbe", "finished"), None);
        assert!(!game
            .world()
            .active_effects()
            .any(|effect| effect.id == timed));
    }
}

#[test]
fn mag_05_saved_dispel_does_not_repeat_its_start_action() {
    let mut original = game(dispel_fixture(0.0));
    cast(&mut original, SPELL, OTHER).unwrap();
    cast(&mut original, DISPEL_SPELL, OTHER).unwrap();
    original.run_until_idle(64);
    let saved = original.save_state().unwrap();
    let mut restored = game(dispel_fixture(0.0));
    restored.load_state(&saved).unwrap();
    let buff = cast(&mut restored, SPELL, OTHER).unwrap()[0];
    restored.tick(1.0);
    assert!(restored
        .world()
        .active_effects()
        .any(|effect| effect.id == buff));
    assert_eq!(restored.world_mut().actor_value(id(OTHER), "health"), 120.0);
}

#[test]
fn mag_05_later_unsupported_entry_cannot_partially_dispel() {
    let mut fixture = dispel_fixture(0.0);
    let mut bad = effect(0, 40, u32::MAX, false);
    bad.form_id = EFFECT + 3;
    fixture.push_top(bad);
    let record = fixture
        .records
        .iter_mut()
        .find(|record| record.form_id == DISPEL_SPELL)
        .unwrap();
    record.subrecords.push(soe_esm::Subrecord::new(
        T(b"EFID"),
        (EFFECT + 3).to_le_bytes(),
    ));
    record
        .subrecords
        .push(soe_esm::Subrecord::new(T(b"EFIT"), vec![0; 12]));
    let mut game = game(fixture);
    cast(&mut game, SPELL, OTHER).unwrap();
    let before = game.world().save_state();
    assert!(matches!(
        cast(&mut game, DISPEL_SPELL, OTHER),
        Err(MagicError::Unsupported("effect archetype"))
    ));
    assert_eq!(game.world().save_state(), before);
}

#[test]
fn mag_05_authored_order_allows_recast_only_after_dispelling() {
    for dispel_first in [true, false] {
        let mut fixture = dispel_fixture(0.0);
        let buff = fixture
            .records
            .iter_mut()
            .find(|record| record.form_id == EFFECT)
            .unwrap();
        put(
            &mut buff
                .subrecords
                .iter_mut()
                .find(|sub| sub.tag == T(b"DATA"))
                .unwrap()
                .data,
            0,
            2 | (1 << 17),
        );
        let buff_entry = spell(SPELL + 3, 0, false, 20.0, 10);
        let buff_entry: Vec<_> = buff_entry
            .subrecords
            .into_iter()
            .filter(|sub| matches!(sub.tag.as_bytes(), b"EFID" | b"EFIT"))
            .collect();
        let record = fixture
            .records
            .iter_mut()
            .find(|record| record.form_id == DISPEL_SPELL)
            .unwrap();
        if dispel_first {
            record.subrecords.extend(buff_entry);
        } else {
            record.subrecords.splice(1..1, buff_entry);
        }
        let mut game = game(fixture);
        let old = cast(&mut game, SPELL, OTHER).unwrap()[0];
        let applied = cast(&mut game, DISPEL_SPELL, OTHER).unwrap();
        assert!(!game.world().active_effects().any(|effect| effect.id == old));
        assert_eq!(applied.len(), if dispel_first { 2 } else { 1 });
        assert_eq!(
            game.world_mut().actor_value(id(OTHER), "health"),
            if dispel_first { 120.0 } else { 100.0 }
        );
    }
}

#[test]
fn mag_05_failed_modifier_reversal_leaves_all_effects_and_events_unchanged() {
    let mut fixture = dispel_fixture(0.0);
    let mut negative = effect(2 | 4, 0, 25, false);
    negative.form_id = EFFECT + 3;
    fixture.push_top(negative);
    fixture.push_top(linked_spell(SPELL + 3, EFFECT + 3, false, f32::MAX, 10));
    let mut game = game(fixture);
    cast(&mut game, SPELL, OTHER).unwrap();
    cast(&mut game, SPELL + 3, OTHER).unwrap();
    game.world_mut()
        .mod_actor_value(id(OTHER), "magicka", f32::MAX)
        .unwrap();
    game.world_mut()
        .actor(id(OTHER))
        .unwrap()
        .values
        .modify_temporary("magicka", f32::MAX)
        .unwrap();
    let before = game.world().save_state();
    assert!(matches!(
        cast(&mut game, DISPEL_SPELL, OTHER),
        Err(MagicError::Number)
    ));
    assert_eq!(game.world().save_state(), before);
}

#[test]
fn installed_dispel_archetype_survey_records_the_direct_cast_boundary() {
    let Some(_data) = installed::data_dir() else {
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let effect_count = order
        .winners_of(T(b"MGEF"))
        .filter(|(_, record)| record.flags & 0x20 == 0)
        .count();
    let spell_count = order
        .winners_of(T(b"SPEL"))
        .filter(|(_, record)| record.flags & 0x20 == 0)
        .count();
    assert!(
        effect_count > 1000 && spell_count > 1000,
        "survey must cover all five masters"
    );
    let mut dispels = std::collections::BTreeSet::new();
    for (id, record) in order.winners_of(T(b"MGEF")) {
        if record.flags & 0x20 != 0 {
            continue;
        }
        let owner = order.winner_ref(id).unwrap();
        let effect = soe_esm::records::gameplay::MagicEffect::from_record(
            record,
            order.file(owner.file).plugin.header.is_localized(),
        )
        .unwrap();
        if effect.data.archetype == 2 {
            eprintln!("Dispel MGEF {id}: {:?}", effect.editor_id);
            dispels.insert(id);
        }
    }
    let mut spells = Vec::new();
    for (id, record) in order.winners_of(T(b"SPEL")) {
        if record.flags & 0x20 != 0 {
            continue;
        }
        let owner = order.winner_ref(id).unwrap();
        let spell = soe_esm::records::gameplay::Spell::from_record(
            record,
            order.file(owner.file).plugin.header.is_localized(),
        )
        .unwrap();
        if spell
            .effects
            .iter()
            .any(|entry| dispels.contains(&order.resolve(owner.file, entry.base_effect)))
        {
            spells.push(id);
        }
    }
    let mut game = Game::from_shared_order(order);
    let player = game.player();
    game.world_mut().actor(player).unwrap();
    let baseline = game.world().save_state();
    let mut accepted = 0;
    let mut rejected = std::collections::BTreeMap::new();
    for spell in &spells {
        game.world_mut().load_state(&baseline).unwrap();
        let shared = game.shared_world();
        let result = shared.borrow_mut().cast_spell(
            *spell,
            player,
            Some(player),
            Some(player),
            &mut |name, class| game.vm.create_object_of(name, class),
        );
        match result {
            Ok(_) => accepted += 1,
            Err(error) => {
                assert_eq!(
                    shared.borrow().save_state(),
                    baseline,
                    "rejected cast {spell}"
                );
                *rejected.entry(error.to_string()).or_insert(0usize) += 1;
            }
        }
    }
    eprintln!("Dispel survey: {effect_count} MGEF, {spell_count} SPEL, {} Dispel effects, {} linked spells, {accepted} accepted, {rejected:?}", dispels.len(), spells.len());
}
