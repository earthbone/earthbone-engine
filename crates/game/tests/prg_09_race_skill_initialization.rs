// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Actor initialization retains resolved skill values across world reloads.
//! Manual DNAM skill values stay cached until race-bonus semantics are known.

use crate::common::installed;

use std::path::PathBuf;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    Game,
    npc::{self, Config, Stats},
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(file: u16, object: u32) -> GlobalFormId {
    GlobalFormId { file, object }
}

fn race(raw: u32, boosts: &[(i8, i8)]) -> Record {
    let mut bytes = vec![0u8; 128];
    for (slot, &(skill, boost)) in boosts.iter().enumerate() {
        bytes[slot * 2] = skill as u8;
        bytes[slot * 2 + 1] = boost as u8;
    }
    for (offset, value) in [
        (36, 100.0f32),
        (40, 50.0),
        (44, 60.0),
        (48, 300.0),
        (52, 1.0),
        (84, 0.7),
        (88, 3.0),
        (92, 5.0),
        (96, 4.0),
        (100, 64.0),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    Record::new(T(b"RACE"), raw).sub(T(b"DATA"), bytes)
}

fn synthetic_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race(0x700, &[(11, 15)]));
    let mut stats = Stats::default();
    stats.skills[5] = 40;
    plugin.push_top(
        npc::build_record(0x800, "Guard", "Guard", Config::default(), stats)
            .sub(T(b"RNAM"), 0x700u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn prg_09_manual_spawn_preserves_the_authored_skill_cache() {
    let mut game = synthetic_game();
    let player = game.player();
    let mut world = game.world_mut();
    let actor = world.place_at(id(0, 0x800), player, false);

    assert_eq!(world.base_actor_value(actor, "heavyarmor"), 40.0);
    assert_eq!(world.base_actor_value(actor, "heavyarmor"), 40.0);
}

#[test]
fn prg_09_manual_skill_cache_survives_world_reload_without_reapplication() {
    let mut game = synthetic_game();
    let player = game.player();
    let mut world = game.world_mut();
    let actor = world.place_at(id(0, 0x800), player, false);
    assert_eq!(world.base_actor_value(actor, "heavyarmor"), 40.0);

    let saved = world.save_state();
    world.load_state(&saved).unwrap();

    assert_eq!(world.base_actor_value(actor, "heavyarmor"), 40.0);
}

#[test]
fn prg_09_installed_spawned_race_skill_bonus_survives_world_reload() {
    let Some(_data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed race-skill check");
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
    let mut game = Game::from_shared_order(order);
    let player = game.player();
    let world = game.world();
    let candidate = world
        .order()
        .winners_of(Tag(*b"NPC_"))
        .filter(|(_, record)| !record.is_deleted())
        .find_map(|(base, _)| {
            let race_id = world.actor_base_race(base).ok()??;
            let race = world.record(race_id)?;
            let boosts = soe_game::race::Attributes::from_record(race).ok()??;
            boosts
                .skill_boosts
                .iter()
                .any(|&(skill, boost)| (6..24).contains(&skill) && boost != 0)
                .then_some(base)
        })
        .expect("installed NPCs with race skill bonuses");
    drop(world);

    let mut world = game.world_mut();
    let actor = world.place_at(candidate, player, false);
    let initial = world.initial_actor_values(candidate).unwrap();
    let race_id = world.actor_base_race(candidate).unwrap().unwrap();
    let race_record = world.record(race_id).unwrap();
    let race = soe_game::race::Attributes::from_record(race_record)
        .unwrap()
        .unwrap();
    let name = race
        .skill_boosts
        .iter()
        .find_map(|&(skill, boost)| {
            let index = usize::try_from(skill.checked_sub(6)?).ok()?;
            (boost != 0).then_some(npc::SKILLS.get(index)?.to_ascii_lowercase())
        })
        .unwrap();
    let expected = initial.iter().find(|(key, _)| key == &name).unwrap().1;
    assert_eq!(world.base_actor_value(actor, &name), expected);

    let saved = world.save_state();
    world.load_state(&saved).unwrap();
    assert_eq!(world.base_actor_value(actor, &name), expected);
}
