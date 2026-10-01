// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRG-07 character thresholds, choices and persistence.

use std::path::PathBuf;

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{self, Config, Stats},
    Game,
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        0x802,
        "Player",
        "Player",
        Config::default(),
        Stats::default(),
    ));
    plugin.push_top(
        Record::new(T(b"GMST"), 0x800)
            .sub(T(b"EDID"), b"fXPLevelUpBase\0".to_vec())
            .sub(T(b"DATA"), 10.0f32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"GMST"), 0x801)
            .sub(T(b"EDID"), b"fXPLevelUpMult\0".to_vec())
            .sub(T(b"DATA"), 5.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Progression.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn prg_07_threshold_grants_one_choice_and_carries_excess_experience() {
    let mut game = game();
    let player = game.player();
    let mut world = game.world_mut();
    world.actor(player).unwrap();

    assert_eq!(world.character_experience_to_next_level(player), Ok(15.0));
    world.advance_character_experience(player, 17.0).unwrap();

    assert_eq!(world.actor_ref(player).unwrap().level, 2);
    assert_eq!(world.level_up_choices(player), Some(1));
    assert_eq!(world.actor_ref(player).unwrap().character_experience, 2.0);
    assert_eq!(world.player_level, 2);
}

#[test]
fn prg_07_each_crossed_threshold_grants_a_choice_and_choice_spends_on_attribute() {
    let mut game = game();
    let player = game.player();
    let mut world = game.world_mut();
    world.actor(player).unwrap();

    world.advance_character_experience(player, 40.0).unwrap();
    assert_eq!(world.actor_ref(player).unwrap().level, 3);
    assert_eq!(world.level_up_choices(player), Some(2));

    let health = world.base_actor_value(player, "health");
    world.choose_level_up_attribute(player, "health").unwrap();
    assert_eq!(world.level_up_choices(player), Some(1));
    assert_eq!(world.base_actor_value(player, "health"), health + 10.0);
}

#[test]
fn prg_07_unspent_choice_and_current_experience_survive_save_load() {
    let mut game = game();
    let player = game.player();
    let mut world = game.world_mut();
    world.actor(player).unwrap();
    world.advance_character_experience(player, 17.0).unwrap();

    let saved = world.save_state();
    world.load_state(&saved).unwrap();

    assert_eq!(world.actor_ref(player).unwrap().level, 2);
    assert_eq!(world.actor_ref(player).unwrap().character_experience, 2.0);
    assert_eq!(world.level_up_choices(player), Some(1));
    assert_eq!(world.player_level, 2);
}

#[test]
fn prg_07_invalid_experience_and_choice_leave_progression_unchanged() {
    let mut game = game();
    let player = game.player();
    let mut world = game.world_mut();
    world.actor(player).unwrap();

    assert!(world
        .advance_character_experience(player, f32::NAN)
        .is_err());
    assert!(world
        .choose_level_up_attribute(player, "onehanded")
        .is_err());
    assert_eq!(world.actor_ref(player).unwrap().level, 1);
    assert_eq!(world.actor_ref(player).unwrap().character_experience, 0.0);
    assert_eq!(world.level_up_choices(player), Some(0));
}

#[test]
fn prg_07_vanilla_level_settings_award_choice_at_the_recorded_threshold() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed level-up check");
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
        let bytes = std::fs::read(data.join(name)).unwrap();
        order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
    }
    let mut game = Game::new(order);
    let player = game.player();
    let mut world = game.world_mut();
    world.actor(player).unwrap();
    let threshold = world.character_experience_to_next_level(player).unwrap();

    world
        .advance_character_experience(player, threshold)
        .unwrap();

    assert_eq!(world.actor_ref(player).unwrap().level, 2);
    assert_eq!(world.level_up_choices(player), Some(1));
    assert_eq!(world.actor_ref(player).unwrap().character_experience, 0.0);
}
