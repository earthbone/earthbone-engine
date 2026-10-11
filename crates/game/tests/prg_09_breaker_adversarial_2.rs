// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Race bonuses apply to skills independently of the class skill weights.
//! Sources: UESP CLAS Weight Calculations and xEdit CLAS/RACE DATA layouts.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn values(race_pairs: &[(i8, i8)], weighted_skill: usize, level: u16) -> Vec<(String, f32)> {
    let mut race_data = vec![0u8; 128];
    for (slot, &(actor_value, boost)) in race_pairs.iter().enumerate() {
        race_data[slot * 2] = actor_value as u8;
        race_data[slot * 2 + 1] = boost as u8;
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
        race_data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    let mut class_data = vec![0u8; 35];
    class_data[6 + weighted_skill] = 1;

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"RACE"), 0x700).sub(T(b"DATA"), race_data));
    plugin.push_top(Record::new(T(b"CLAS"), 0x701).sub(T(b"DATA"), class_data));
    plugin.push_top(
        npc::build_record(
            0x800,
            "RaceBonus",
            "Race Bonus",
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
    order.add("Prg09Bonus.esm", plugin).unwrap();
    Game::new(order)
        .world()
        .initial_actor_values(GlobalFormId { file: 0, object: 0x800 })
        .unwrap()
}

fn skill(values: &[(String, f32)], name: &str) -> f32 {
    values
        .iter()
        .find(|(candidate, _)| candidate == name)
        .unwrap()
        .1
}

#[test]
fn prg_09_race_bonus_applies_to_unweighted_skill_without_shifting_neighbors() {
    let actor_values = values(&[(23, 12)], 0, 2);

    assert_eq!(skill(&actor_values, npc::SKILLS[0]), 23.0);
    assert_eq!(skill(&actor_values, npc::SKILLS[16]), 15.0);
    assert_eq!(skill(&actor_values, npc::SKILLS[17]), 27.0);
}

#[test]
fn prg_09_race_skill_boost_actor_value_bounds_map_only_to_skills() {
    let actor_values = values(&[(5, 30), (6, 10), (24, -30)], 0, 1);

    assert_eq!(skill(&actor_values, npc::SKILLS[0]), 25.0);
    assert_eq!(skill(&actor_values, npc::SKILLS[1]), 15.0);
    assert_eq!(skill(&actor_values, npc::SKILLS[17]), 15.0);
}
