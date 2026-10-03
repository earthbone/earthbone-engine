// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Actor initialization retains resolved skill values across world reloads.
//! Manual DNAM skill values stay cached until race-bonus semantics are known.

use crate::common::installed;

use std::path::{Path, PathBuf};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{self, Config, Npc, Stats},
    Game,
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

#[test]
fn prg_09_installed_manual_skill_uses_dnam_and_offset_without_race_bonus() {
    let Some(_data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed manual skill check");
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
    let world = game.world();
    let candidate = world
        .order()
        .winners_of(Tag(*b"NPC_"))
        .filter(|(_, record)| !record.is_deleted())
        .find_map(|(base, record)| {
            let npc = Npc::from_record(record, false).ok()?;
            if npc.template.is_some() || npc.config.flags & npc::flags::AUTO_CALC_STATS != 0 {
                return None;
            }
            let race_id = world.actor_base_race(base).ok()??;
            let race_record = world.record(race_id)?;
            let race = soe_game::race::Attributes::from_record(race_record).ok()??;
            let stats = npc.stats?;
            race.skill_boosts.iter().find_map(|&(actor_value, boost)| {
                let index = usize::try_from(actor_value.checked_sub(6)?).ok()?;
                if index >= stats.skills.len() || boost == 0 {
                    return None;
                }
                let value = f32::from(stats.skills[index]) + f32::from(stats.skill_offsets[index]);
                Some((base, npc::SKILLS[index].to_ascii_lowercase(), value, boost))
            })
        })
        .expect("vanilla manual NPC with a nonzero racial skill bonus");
    drop(world);

    let player = game.player();
    let mut world = game.world_mut();
    let actor = world.place_at(candidate.0, player, false);
    assert_eq!(world.base_actor_value(actor, &candidate.1), candidate.2);
    assert_ne!(candidate.2, candidate.2 + f32::from(candidate.3));

    let saved = world.save_state();
    world.load_state(&saved).unwrap();
    assert_eq!(world.base_actor_value(actor, &candidate.1), candidate.2);
}

fn collect_saves(directory: &Path, paths: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_saves(&path, paths);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ess"))
        {
            paths.push(path);
        }
    }
}

#[test]
fn prg_09_save_corpus_checks_manual_npc_skill_overrides() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES not set, skipping manual skill save probe");
        return;
    };
    let Some(order) = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        eprintln!("SKYRIM_DATA not set, skipping manual skill save probe");
        return;
    };
    let game = Game::from_shared_order(order);
    let world = game.world();
    let mut paths = Vec::new();
    collect_saves(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected the 60-save corpus");

    let mut inspected = 0usize;
    let mut matching = 0usize;
    let mut equivalent_to_dnam = 0usize;
    let mut equivalent_to_dnam_plus_race = 0usize;
    for path in paths {
        let save_name = path.display().to_string();
        let save = soe_save::Save::open(path).unwrap();
        for change in &save.change_forms {
            if change.type_index() != 9 {
                continue;
            }
            let Some(saved_npc) = change.npc_change().unwrap() else {
                continue;
            };
            let Some(saved_skills) = saved_npc.skills else {
                continue;
            };
            let Some(form_id) = save.resolve(change.form) else {
                continue;
            };
            let save_file = (form_id >> 24) as usize;
            let Some(plugin_name) = save.plugins.get(save_file) else {
                continue;
            };
            let Some(file) = world.order().position_of(&plugin_name.to_text()) else {
                continue;
            };
            let base = world.order().resolve(file, form_id & 0x00ff_ffff);
            let Some(record) = world.record(base) else {
                continue;
            };
            if record.is_deleted() {
                continue;
            }
            let Ok(npc) = Npc::from_record(record, false) else {
                continue;
            };
            if npc.template.is_some() || npc.config.flags & npc::flags::AUTO_CALC_STATS != 0 {
                continue;
            }
            let Some(stats) = npc.stats else {
                continue;
            };
            let Some(race_id) = world.actor_base_race(base).ok().flatten() else {
                continue;
            };
            let Some(race_record) = world.record(race_id) else {
                continue;
            };
            let Ok(Some(race)) = soe_game::race::Attributes::from_record(race_record) else {
                continue;
            };
            for &(actor_value, boost) in &race.skill_boosts {
                let Some(index) = actor_value
                    .checked_sub(6)
                    .map(usize::try_from)
                    .transpose()
                    .ok()
                    .flatten()
                else {
                    continue;
                };
                if index >= stats.skills.len() || boost == 0 {
                    continue;
                }
                matching += 1;
                let saved = i32::from(saved_skills[index]) + i32::from(saved_skills[18 + index]);
                let dnam = i32::from(stats.skills[index]) + i32::from(stats.skill_offsets[index]);
                inspected += 1;
                equivalent_to_dnam += usize::from(saved == dnam);
                equivalent_to_dnam_plus_race += usize::from(saved == dnam + i32::from(boost));
                assert_eq!(
                    saved,
                    dnam,
                    "saved manual NPC {} in {save_name} has skill {} = {saved}, DNAM plus offset is {dnam}, race boost is {boost}",
                    npc.editor_id.as_deref().unwrap_or("<no EDID>"),
                    npc::SKILLS[index]
                );
                assert_ne!(
                    saved,
                    dnam + i32::from(boost),
                    "saved manual NPC {} in {save_name} includes its race boost for {}",
                    npc.editor_id.as_deref().unwrap_or("<no EDID>"),
                    npc::SKILLS[index]
                );
            }
        }
    }
    assert!(
        matching > 0,
        "expected a saved manual NPC with a race boost"
    );
    assert!(
        inspected > 0,
        "expected at least one comparable saved skill"
    );
    eprintln!(
        "PRG-09 saved manual skills: {inspected} comparisons, {matching} race-boosted fields, {equivalent_to_dnam} equal DNAM plus offset, {equivalent_to_dnam_plus_race} equal DNAM plus offset and race boost"
    );
}
