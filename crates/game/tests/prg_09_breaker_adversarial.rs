// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRG-09 race skill bonus lifecycle checks.
//! Sources: local UESP CLAS.wikitext Weight Calculations, UESP NPC.wikitext,
//! TES5Edit Core/wbDefinitionsTES5.pas CLAS and RACE DATA layouts, and skymp
//! GetBaseActorValues.cpp for resolved race and NPC source ownership.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn race(raw: u32, actor_value: i8, boost: i8) -> Record {
    let mut data = vec![0u8; 128];
    data[0] = actor_value as u8;
    data[1] = boost as u8;
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
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    Record::new(T(b"RACE"), raw).sub(T(b"DATA"), data)
}

fn auto_game(race_record: Record, actor_value: i8, level: u16) -> Game {
    let mut class_data = vec![0u8; 35];
    class_data[6 + usize::try_from(actor_value - 6).unwrap()] = 1;
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race_record);
    plugin.push_top(Record::new(T(b"CLAS"), 0x701).sub(T(b"DATA"), class_data));
    plugin.push_top(
        npc::build_record(
            0x800,
            "RaceSkillActor",
            "Race Skill Actor",
            npc::Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level,
                ..npc::Config::default()
            },
            npc::Stats::default(),
        )
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes())
        .sub(T(b"CNAM"), 0x701u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Prg09Breaker.esm", plugin).unwrap();
    Game::new(order)
}

fn skill(values: &[(String, f32)], name: &str) -> f32 {
    let name = name.to_ascii_lowercase();
    values
        .iter()
        .find(|(candidate, _)| candidate == &name)
        .unwrap()
        .1
}

#[test]
fn spawned_race_bonus_is_seeded_once_and_save_keeps_actor_changes() {
    let mut game = auto_game(race(0x700, 11, 15), 11, 1);
    let player = game.player();
    let mut world = game.world_mut();
    let first = world.place_at(id(0x800), player, false);
    assert_eq!(world.base_actor_value(first, "heavyarmor"), 30.0);
    world.set_actor_value(first, "heavyarmor", 77.0).unwrap();

    let saved = world.save_state();
    world.load_state(&saved).unwrap();
    assert_eq!(world.base_actor_value(first, "heavyarmor"), 77.0);

    let second = world.place_at(id(0x800), player, false);
    assert_eq!(world.base_actor_value(second, "heavyarmor"), 30.0);
}

#[test]
fn signed_negative_race_bonus_targets_the_authored_skill_slot() {
    let game = auto_game(race(0x700, 23, -5), 23, 1);
    let values = game.world().initial_actor_values(id(0x800)).unwrap();
    assert_eq!(skill(&values, npc::SKILLS[17]), 10.0);
    assert_eq!(skill(&values, npc::SKILLS[16]), 15.0);
}

#[test]
fn missing_race_record_rejects_autocalculated_skill_seed() {
    let mut class_data = vec![0u8; 35];
    class_data[6 + 5] = 1;
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CLAS"), 0x701).sub(T(b"DATA"), class_data));
    plugin.push_top(
        npc::build_record(
            0x800,
            "MissingRace",
            "Missing Race",
            npc::Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 5,
                ..npc::Config::default()
            },
            npc::Stats::default(),
        )
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes())
        .sub(T(b"CNAM"), 0x701u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Prg09MissingRace.esm", plugin).unwrap();
    let game = Game::new(order);
    assert!(game.world().initial_actor_values(id(0x800)).is_err());
}
