// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use soe_esm::records::gameplay::Spell;

fn concentration_spell() -> Record {
    let mut record = spell(SPELL, 0, false, 20.0, 0);
    let spit = record
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"SPIT"))
        .expect("spell fixture has SPIT");
    put(spit.data.as_mut_slice(), 0, 20);
    put(spit.data.as_mut_slice(), 4, 1);
    put(spit.data.as_mut_slice(), 16, 2);
    put(spit.data.as_mut_slice(), 20, 3);
    record
}

fn fixture() -> Game {
    let mut mgef = effect(1 | 4 | (1 << 17), 0, 24, false);
    let data = mgef
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"DATA"))
        .expect("effect fixture has DATA");
    put(data.data.as_mut_slice(), 80, 2);
    game(plugin(mgef, vec![concentration_spell()]))
}

fn begin(game: &mut Game) {
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .begin_concentration_cast(
            id(SPELL),
            id(ACTOR),
            id(ACTOR),
            Some(id(OTHER)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_03_held_cast_ticks_effect_and_magicka_with_unpaused_game_time() {
    let mut game = fixture();
    game.world_mut()
        .set_actor_value(id(OTHER), "health", 100.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(ACTOR), "magicka", 100.0)
        .unwrap();
    begin(&mut game);

    game.tick(0.5);

    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 90.0);
    assert!((game.world_mut().actor_value(id(ACTOR), "magicka") - 99.0).abs() < 0.001);
    assert!(game.world().is_concentration_casting(id(ACTOR)));
}

#[test]
fn mag_03_concentration_start_emits_one_spell_cast_event() {
    let mut game = fixture();
    begin(&mut game);
    let count = |game: &Game| {
        game.world()
            .pending
            .iter()
            .filter(
                |pending| matches!(pending, Pending::Event { name, .. } if name == "OnSpellCast"),
            )
            .count()
    };
    assert_eq!(count(&game), 1);

    let shared = game.shared_world();
    shared
        .borrow_mut()
        .tick_concentration(0.5, &mut |name, class| {
            game.vm.create_object_of(name, class)
        });

    assert_eq!(count(&game), 1);
}

#[test]
fn mag_03_menu_pause_does_not_advance_held_cast() {
    let mut game = fixture();
    game.world_mut()
        .set_actor_value(id(OTHER), "health", 100.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(ACTOR), "magicka", 100.0)
        .unwrap();
    begin(&mut game);

    game.tick_menu_mode(1.0, true);

    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 100.0);
    assert!(game.world().is_concentration_casting(id(ACTOR)));
}

#[test]
fn mag_03_release_stops_future_effect_and_resource_ticks() {
    let mut game = fixture();
    game.world_mut()
        .set_actor_value(id(OTHER), "health", 100.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(ACTOR), "magicka", 100.0)
        .unwrap();
    begin(&mut game);
    game.tick(0.5);
    assert!(game.world_mut().release_concentration_cast());

    game.tick(0.5);

    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 90.0);
    assert!((game.world_mut().actor_value(id(ACTOR), "magicka") - 99.0).abs() < 0.001);
    assert!(!game.world().is_concentration_casting(id(ACTOR)));
}

#[test]
fn mag_03_magicka_exhaustion_stops_cast_at_available_duration() {
    let mut game = fixture();
    game.world_mut()
        .set_actor_value(id(OTHER), "health", 100.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(ACTOR), "magicka", 0.5)
        .unwrap();
    begin(&mut game);

    game.tick(1.0);

    assert!((game.world_mut().actor_value(id(OTHER), "health") - 95.0).abs() < 0.001);
    assert!(game.world_mut().actor_value(id(ACTOR), "magicka") <= 0.001);
    assert!(!game.world().is_concentration_casting(id(ACTOR)));
}

#[test]
fn mag_03_installed_concentration_record_ticks_magicka() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed concentration record check");
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
    let concentration_spells: Vec<_> = order
        .winners_of(T(b"SPEL"))
        .filter(|(_, record)| record.flags & 0x20 == 0)
        .filter_map(|(id, record)| {
            Spell::from_record(record, false)
                .ok()
                .map(|spell| (id, spell))
        })
        .filter(|(_, spell)| spell.data.cast_type == 2 && matches!(spell.data.delivery, 0 | 2 | 3))
        .map(|(id, _)| id)
        .collect();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut()
        .set_actor_value(player, "magicka", 100.0)
        .unwrap();
    let mut started = None;
    for spell in concentration_spells {
        let cost = game.world_mut().spell_cost(spell, player).unwrap_or(0.0);
        if cost <= 0.0 {
            continue;
        }
        let result = game.shared_world().borrow_mut().begin_concentration_cast(
            spell,
            player,
            player,
            Some(player),
            &mut |name, class| game.vm.create_object_of(name, class),
        );
        if result.is_ok() {
            started = Some((spell, cost));
            break;
        }
    }
    let Some((spell, cost)) = started else {
        panic!("installed corpus has a supported concentration spell with positive cost");
    };
    assert!(game.world().is_concentration_casting(player));
    game.tick(0.25);
    assert!(
        game.world_mut().actor_value(player, "magicka") < 100.0,
        "installed concentration spell {spell:?} with total cost {cost} did not drain magicka"
    );
}
