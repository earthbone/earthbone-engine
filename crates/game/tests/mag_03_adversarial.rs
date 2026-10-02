// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, world::Pending};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const TARGET: u32 = 0x101;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn cast_fixture() -> Game {
    let mut mgef_data = vec![0; 152];
    put(&mut mgef_data, 0, 1 | 4 | (1 << 17));
    put(&mut mgef_data, 16, u32::MAX);
    put(&mut mgef_data, 64, 0);
    put(&mut mgef_data, 68, 24);
    put(&mut mgef_data, 80, 2);
    put(&mut mgef_data, 84, 3);
    let mgef = Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), mgef_data);

    let mut spit = vec![0; 36];
    put(&mut spit, 0, 20);
    put(&mut spit, 4, 1);
    put(&mut spit, 16, 2);
    put(&mut spit, 20, 3);
    let mut efit = 20.0_f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&0_u32.to_le_bytes());
    let spell = Record::new(T(b"SPEL"), SPELL)
        .sub(T(b"SPIT"), spit)
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(T(b"EFIT"), efit);

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    for actor in [ACTOR, TARGET] {
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), 0x500_u32.to_le_bytes())
                .sub(T(b"DATA"), vec![0; 24]),
        );
    }
    plugin.push_top(mgef);
    plugin.push_top(spell);
    let mut order = LoadOrder::new();
    order.add("Magic.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [ACTOR, TARGET] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
        game.world_mut()
            .set_actor_value(id(actor), "magicka", 100.0)
            .unwrap();
    }
    game
}

fn begin(game: &mut Game) {
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .begin_concentration_cast(
            id(SPELL),
            id(ACTOR),
            id(ACTOR),
            Some(id(TARGET)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_03_irregular_ticks_charge_only_elapsed_cast_time() {
    let mut game = cast_fixture();
    game.world_mut()
        .set_actor_value(id(TARGET), "health", 100.0)
        .unwrap();
    begin(&mut game);

    let shared = game.shared_world();
    for dt in [0.03, 0.17, 0.41, 0.09] {
        shared
            .borrow_mut()
            .tick_concentration(dt, &mut |name, class| game.vm.create_object_of(name, class));
    }

    assert!((game.world_mut().actor_value(id(ACTOR), "magicka") - 98.6).abs() < 0.002);
    assert!((game.world_mut().actor_value(id(TARGET), "health") - 86.0).abs() < 0.02);
    assert!(game.world().is_concentration_casting(id(ACTOR)));
}

#[test]
fn mag_03_invalid_tick_does_not_mutate_effect_resource_or_cast() {
    let mut game = cast_fixture();
    game.world_mut()
        .set_actor_value(id(TARGET), "health", 100.0)
        .unwrap();
    begin(&mut game);
    let health = game.world_mut().actor_value(id(TARGET), "health");
    let magicka = game.world_mut().actor_value(id(ACTOR), "magicka");

    let shared = game.shared_world();
    for dt in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(
            shared
                .borrow_mut()
                .tick_concentration(dt, &mut |name, class| game.vm.create_object_of(name, class))
                .is_empty()
        );
    }

    assert_eq!(game.world_mut().actor_value(id(TARGET), "health"), health);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), magicka);
    assert!(game.world().is_concentration_casting(id(ACTOR)));
}

#[test]
fn mag_03_cast_start_notifies_once_across_multiple_ticks() {
    let mut game = cast_fixture();
    begin(&mut game);
    let shared = game.shared_world();
    for _ in 0..3 {
        shared
            .borrow_mut()
            .tick_concentration(0.2, &mut |name, class| {
                game.vm.create_object_of(name, class)
            });
    }
    let events = game
        .world()
        .pending
        .iter()
        .filter(|pending| matches!(pending, Pending::Event { name, .. } if name == "OnSpellCast"))
        .count();
    assert_eq!(events, 1);
}

#[test]
fn mag_03_release_stops_future_effect_and_cost_ticks() {
    let mut game = cast_fixture();
    game.world_mut()
        .set_actor_value(id(TARGET), "health", 100.0)
        .unwrap();
    begin(&mut game);
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .tick_concentration(0.1, &mut |name, class| {
            game.vm.create_object_of(name, class)
        });
    assert!(shared.borrow_mut().release_concentration_cast());
    let health = game.world_mut().actor_value(id(TARGET), "health");
    let magicka = game.world_mut().actor_value(id(ACTOR), "magicka");

    shared
        .borrow_mut()
        .tick_concentration(1.0, &mut |name, class| {
            game.vm.create_object_of(name, class)
        });

    assert_eq!(game.world_mut().actor_value(id(TARGET), "health"), health);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), magicka);
    assert!(!game.world().is_concentration_casting(id(ACTOR)));
}

#[test]
fn mag_03_exhaustion_stops_at_fractional_magicka_boundary() {
    let mut game = cast_fixture();
    game.world_mut()
        .set_actor_value(id(TARGET), "health", 100.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(ACTOR), "magicka", 0.05)
        .unwrap();
    begin(&mut game);

    let shared = game.shared_world();
    shared
        .borrow_mut()
        .tick_concentration(1.0, &mut |name, class| {
            game.vm.create_object_of(name, class)
        });

    assert!(game.world_mut().actor_value(id(ACTOR), "magicka") <= 0.001);
    assert!((game.world_mut().actor_value(id(TARGET), "health") - 99.5).abs() < 0.02);
    assert!(!game.world().is_concentration_casting(id(ACTOR)));
}
