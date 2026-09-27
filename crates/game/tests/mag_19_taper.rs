// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Magic Effect taper values continue an effect after its EFIT duration.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const ACTOR: u32 = 0x100;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture(flags: u32, weight: f32, curve: f32, taper_duration: f32, effect_duration: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x500).sub(Tag(*b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), ACTOR)
            .sub(Tag(*b"NAME"), 0x500u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]),
    );
    let mut data = vec![0; 152];
    put(&mut data, 0, flags);
    put(&mut data, 16, u32::MAX);
    put(&mut data, 64, 0);
    put(&mut data, 68, 24);
    put(&mut data, 80, 1);
    put(&mut data, 84, 3);
    data[28..32].copy_from_slice(&weight.to_le_bytes());
    data[52..56].copy_from_slice(&curve.to_le_bytes());
    data[56..60].copy_from_slice(&taper_duration.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"MGEF"), EFFECT).sub(Tag(*b"DATA"), data));

    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 16, 1);
    put(&mut spell_data, 20, 3);
    let mut efit = 10.0_f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&effect_duration.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"SPEL"), SPELL)
            .sub(Tag(*b"SPIT"), spell_data)
            .sub(Tag(*b"EFID"), EFFECT.to_le_bytes())
            .sub(Tag(*b"EFIT"), efit),
    );
    let mut order = LoadOrder::new();
    order.add("Taper.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut()
        .set_actor_value(id(ACTOR), "health", 100.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(ACTOR), "magicka", 100.0)
        .unwrap();
    game
}

fn cast(game: &mut Game) {
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(ACTOR),
            Some(id(ACTOR)),
            Some(id(ACTOR)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_19_authored_weight_curve_and_duration_drive_taper_progression() {
    let mut game = fixture(1 << 2, 0.5, 2.0, 4.0, 4);
    cast(&mut game);
    let world = game.world();
    let effect = world.active_effects().next().unwrap();
    assert_eq!(effect.taper_weight, 0.5);
    assert_eq!(effect.taper_curve, 2.0);
    assert_eq!(effect.taper_duration, 4.0);
    drop(world);

    game.tick(4.0);
    assert!((game.world_mut().actor_value(id(ACTOR), "health") - 60.0).abs() < 0.001);
    assert_eq!(game.world().active_effects().count(), 1);
    game.tick(1.0);
    let at_taper_second_one = game.world_mut().actor_value(id(ACTOR), "health");
    assert!(
        (at_taper_second_one - 56.1458).abs() < 0.001,
        "unexpected first taper value: {at_taper_second_one}"
    );
}

#[test]
fn mag_19_taper_progression_and_authored_values_survive_save_load() {
    let mut game = fixture(1 << 2, 0.5, 2.0, 4.0, 4);
    cast(&mut game);
    game.tick(5.0);
    let bytes = game.save_state().unwrap();
    let mut restored = fixture(1 << 2, 0.5, 2.0, 4.0, 4);
    restored.load_state(&bytes).unwrap();
    let world = restored.world();
    let effect = world.active_effects().next().unwrap();
    assert_eq!(effect.elapsed, 5.0);
    assert_eq!(effect.taper_weight, 0.5);
    assert_eq!(effect.taper_curve, 2.0);
    assert_eq!(effect.taper_duration, 4.0);
    drop(world);

    restored.tick(1.0);
    let at_taper_second_two = restored.world_mut().actor_value(id(ACTOR), "health");
    assert!((at_taper_second_two - 54.1667).abs() < 0.001);
    restored.tick(2.0);
    assert_eq!(restored.world().active_effects().count(), 0);
    let after_taper = restored.world_mut().actor_value(id(ACTOR), "health");
    assert!((after_taper - 53.3333).abs() < 0.001);
}

#[test]
fn mag_19_recover_effect_keeps_full_magnitude_until_taper_expiry() {
    let mut game = fixture(1 << 1, 0.5, 2.0, 4.0, 4);
    cast(&mut game);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 110.0);
    game.tick(4.0);
    assert_eq!(game.world().active_effects().count(), 1);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 110.0);
    game.tick(4.0);
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
}

#[test]
fn mag_19_zero_duration_effect_runs_its_authored_taper() {
    let mut game = fixture(1 << 2, 0.5, 2.0, 4.0, 0);
    cast(&mut game);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
    game.tick(1.0);
    let health = game.world_mut().actor_value(id(ACTOR), "health");
    assert!((health - 96.1458).abs() < 0.001);
    assert_eq!(game.world().active_effects().count(), 1);
}
