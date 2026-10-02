// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn mag_49_cure_addiction_spell_and_ingestible_reject_without_changes() {
    let mut magic_effect = effect(0, 28, u32::MAX, false);
    magic_effect.form_id = EFFECT;

    let mut addiction_spell = spell(SPELL, 10, false, 0.0, 0);
    let mut addiction_data = vec![0; 36];
    put(&mut addiction_data, 8, 10);
    put(&mut addiction_data, 16, 1);
    put(&mut addiction_data, 20, 3);
    addiction_spell
        .subrecords
        .iter_mut()
        .find(|subrecord| subrecord.tag == T(b"SPIT"))
        .unwrap()
        .data = addiction_data;

    let mut plugin = plugin(magic_effect, vec![addiction_spell]);
    let mut potion = ingestible(POTION, 20.0, 5);
    potion
        .subrecords
        .iter_mut()
        .find(|subrecord| subrecord.tag == T(b"EFID"))
        .unwrap()
        .data = EFFECT.to_le_bytes().to_vec();
    plugin.push_top(potion);

    let mut game = game(plugin);
    game.world_mut().actor_value(id(OTHER), "health");
    let before = game.save_state().unwrap();
    assert!(matches!(
        cast(&mut game, SPELL, OTHER),
        Err(MagicError::Unsupported("effect archetype"))
    ));
    assert_eq!(game.save_state().unwrap(), before);
    assert_eq!(game.world().active_effects().count(), 0);

    game.world_mut().add_item(id(OTHER), id(POTION), 1);
    let before_drink = game.world().save_state();
    let shared = game.shared_world();
    assert!(matches!(
        shared
            .borrow_mut()
            .drink_ingestible(id(POTION), id(OTHER), &mut |name, class| game
                .vm
                .create_object_of(name, class),),
        Err(MagicError::Unsupported("effect archetype"))
    ));
    assert_eq!(shared.borrow().save_state(), before_drink);
    assert_eq!(shared.borrow_mut().item_count(id(OTHER), id(POTION)), 1);
    assert_eq!(shared.borrow().active_effects().count(), 0);
}

#[test]
fn mag_49_addiction_spell_survives_general_dispelling() {
    let base_effect = effect(0, 1, u32::MAX, true);
    let mut game = game(plugin(base_effect, vec![spell(SPELL, 10, true, 0.0, 0)]));
    let applied = cast(&mut game, SPELL, OTHER).unwrap();
    assert_eq!(applied.len(), 1);

    let target = Value::Object(game.object_of_form(id(OTHER)));
    native(&mut game, target, "DispelAllSpells", &[]);

    assert_eq!(
        game.world()
            .active_effects()
            .filter(|effect| effect.spell == id(SPELL))
            .count(),
        1
    );
}

#[test]
fn mag_49_installed_cure_addiction_corpus_survey() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping MAG-49 installed corpus survey");
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

    let game = Game::new(order);
    let effects: Vec<_> = game
        .world()
        .order()
        .winners_of(T(b"MGEF"))
        .filter(|(_, record)| !record.is_deleted())
        .filter_map(|(id, record)| {
            let winner = game.world().order().winner_ref(id).unwrap();
            let localized = game
                .world()
                .order()
                .file(winner.file)
                .plugin
                .header
                .is_localized();
            let effect =
                soe_esm::records::gameplay::MagicEffect::from_record(record, localized).unwrap();
            (effect.data.archetype == 28).then_some((id, effect.editor_id))
        })
        .collect();
    assert!(
        game.world().order().winners_of(T(b"MGEF")).count() > 1000,
        "survey must cover the five-master MGEF corpus"
    );

    let mut spell_users = Vec::new();
    for (id, record) in game.world().order().winners_of(T(b"SPEL")) {
        if record.is_deleted() {
            continue;
        }
        let winner = game.world().order().winner_ref(id).unwrap();
        let localized = game
            .world()
            .order()
            .file(winner.file)
            .plugin
            .header
            .is_localized();
        let spell = soe_esm::records::gameplay::Spell::from_record(record, localized).unwrap();
        if spell.effects.iter().any(|entry| {
            game.world()
                .resolve_from(id, entry.base_effect)
                .is_some_and(|effect| effects.iter().any(|(cure, _)| *cure == effect))
        }) {
            spell_users.push((id, spell.editor_id));
        }
    }

    let mut ingestible_users = Vec::new();
    for (id, record) in game.world().order().winners_of(T(b"ALCH")) {
        if record.is_deleted() {
            continue;
        }
        let winner = game.world().order().winner_ref(id).unwrap();
        let localized = game
            .world()
            .order()
            .file(winner.file)
            .plugin
            .header
            .is_localized();
        let item = soe_esm::records::gameplay::Ingestible::from_record(record, localized).unwrap();
        if item.effects.iter().any(|entry| {
            game.world()
                .resolve_from(id, entry.base_effect)
                .is_some_and(|effect| effects.iter().any(|(cure, _)| *cure == effect))
        }) {
            ingestible_users.push((id, item.editor_id));
        }
    }

    eprintln!(
        "MAG-49 five-master survey: {} Cure Addiction MGEFs {effects:?}, {} SPEL users {spell_users:?}, {} ALCH users {ingestible_users:?}",
        effects.len(),
        spell_users.len(),
        ingestible_users.len(),
    );
    assert!(
        effects.is_empty(),
        "the five-master Skyrim corpus has no Cure Addiction archetype records: {effects:?}"
    );
    assert!(spell_users.is_empty());
    assert!(ingestible_users.is_empty());
}
