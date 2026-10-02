// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRG-01 synthetic skill-use curves and actor save persistence.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    Game,
    npc::{self, Config, Stats},
};
use std::path::PathBuf;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut stats = Stats::default();
    stats.skills[0] = 5;
    plugin.push_top(npc::build_record(
        0x800,
        "Skill Actor",
        "Skill Actor",
        Config::default(),
        stats,
    ));
    plugin.push_top(
        Record::new(T(b"AVIF"), 0x801)
            .sub(T(b"EDID"), b"AVOneHanded\0".to_vec())
            .sub(
                T(b"AVSK"),
                [2.0f32, 1.0, 0.2, 2.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<u8>>(),
            ),
    );
    plugin.push_top(
        Record::new(T(b"GMST"), 0x802)
            .sub(T(b"EDID"), b"fSkillUseCurve\0".to_vec())
            .sub(T(b"DATA"), 2.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Progression.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn prg_01_skill_use_follows_avif_curve_and_persists_level_and_partial_experience() {
    let mut game = game();
    let player = game.player();
    let mut world = game.world_mut();
    let actor = world.place_at(id(0x800), player, false);

    world.advance_skill(actor, "onehanded", 10.0).unwrap();
    assert_eq!(world.base_actor_value(actor, "onehanded"), 7.0);
    let partial = world.skill_experience(actor, "onehanded").unwrap();
    assert!((partial - 4.8).abs() < 0.0001);

    let saved = world.save_state();
    world.load_state(&saved).unwrap();
    assert_eq!(world.base_actor_value(actor, "onehanded"), 7.0);
    assert!((world.skill_experience(actor, "onehanded").unwrap() - 4.8).abs() < 0.0001);
}

#[test]
fn prg_01_invalid_skill_use_leaves_level_and_experience_unchanged() {
    let mut game = game();
    let player = game.player();
    let mut world = game.world_mut();
    let actor = world.place_at(id(0x800), player, false);

    assert!(world.advance_skill(actor, "health", 10.0).is_err());
    assert!(world.advance_skill(actor, "onehanded", f32::NAN).is_err());
    assert_eq!(world.base_actor_value(actor, "onehanded"), 5.0);
    assert_eq!(world.skill_experience(actor, "onehanded"), Some(0.0));
}

#[test]
fn prg_01_skill_use_stops_at_level_one_hundred() {
    let mut game = game();
    let player = game.player();
    let mut world = game.world_mut();
    let actor = world.place_at(id(0x800), player, false);
    world.set_actor_value(actor, "onehanded", 100.0).unwrap();

    world.advance_skill(actor, "onehanded", 100.0).unwrap();
    assert_eq!(world.base_actor_value(actor, "onehanded"), 100.0);
}

#[test]
fn prg_01_installed_skill_avif_curve_advances_and_survives_reload() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed skill-use check");
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
    world.set_actor_value(player, "onehanded", 5.0).unwrap();
    world.advance_skill(player, "onehanded", 1.0).unwrap();
    let level = world.base_actor_value(player, "onehanded");
    let experience = world.skill_experience(player, "onehanded").unwrap();
    assert!(level > 5.0 || experience > 0.0);

    let saved = world.save_state();
    world.load_state(&saved).unwrap();
    assert_eq!(world.base_actor_value(player, "onehanded"), level);
    assert_eq!(
        world.skill_experience(player, "onehanded"),
        Some(experience)
    );
}
