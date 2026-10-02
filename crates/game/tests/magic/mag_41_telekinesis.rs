// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn telekinesis_fixture() -> Game {
    let mut mgef = effect(0, 20, 0, false);
    let data = mgef
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"DATA"))
        .expect("effect fixture has DATA");
    put(data.data.as_mut_slice(), 80, 2);
    put(data.data.as_mut_slice(), 84, 0);

    let mut spell_record = spell(SPELL, 0, false, 1.0, 0);
    let spit = spell_record
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"SPIT"))
        .expect("spell fixture has SPIT");
    put(spit.data.as_mut_slice(), 16, 2);
    put(spit.data.as_mut_slice(), 20, 0);

    game(plugin(mgef, vec![spell_record]))
}

#[test]
fn mag_41_grab_holds_one_eligible_reference_and_interruption_releases_it() {
    let mut game = telekinesis_fixture();
    game.world_mut()
        .set_actor_value(id(ACTOR), "magicka", 100.0)
        .unwrap();
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .begin_concentration_cast(id(SPELL), id(ACTOR), id(ACTOR), None, &mut |name, class| {
            game.vm.create_object_of(name, class)
        })
        .unwrap();

    let object = game.world_mut().place_at(id(ITEM), id(ACTOR), false);
    let other = game.world_mut().place_at(id(OTHER_ITEM), id(ACTOR), false);
    assert!(
        game.world_mut()
            .grab_telekinesis_reference(id(ACTOR), object)
    );
    assert_eq!(game.world().telekinesis_reference(id(ACTOR)), Some(object));
    assert!(
        !game
            .world_mut()
            .grab_telekinesis_reference(id(ACTOR), other)
    );

    assert!(game.world_mut().release_concentration_cast());
    assert_eq!(game.world().telekinesis_reference(id(ACTOR)), None);
    assert!(!game.world_mut().release_telekinesis_reference(id(ACTOR)));
}

#[test]
fn mag_41_installed_telekinesis_effect_uses_archetype_twenty() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed Telekinesis check");
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
    let telekinesis_effects: Vec<_> = order
        .winners_of(T(b"MGEF"))
        .filter(|(_, record)| !record.is_deleted())
        .filter_map(|(id, record)| {
            let winner = order.winner_ref(id)?;
            let localized = order.file(winner.file).plugin.header.is_localized();
            let effect =
                soe_esm::records::gameplay::MagicEffect::from_record(record, localized).ok()?;
            (effect.data.archetype == 20).then_some(id)
        })
        .collect();
    assert_eq!(telekinesis_effects.len(), 1);
    assert_eq!(telekinesis_effects[0].object, 0x1A4CB);
}
