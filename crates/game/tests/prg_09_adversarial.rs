// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Race skill initialization checks grounded in UESP CLAS weight calculations.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};

const RACE: u32 = 0x700;
const CLASS: u32 = 0x701;
const NPC: u32 = 0x800;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    // TES5Edit RACE DATA starts with seven actor-value/boost pairs.
    let mut race_data = vec![0; 128];
    race_data[0] = 11; // HeavyArmor actor value index.
    race_data[1] = 15;
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

    // xEdit CLAS DATA has 18 skill weights at bytes 6 to 23.
    let mut class_data = vec![0; 35];
    class_data[6] = 2; // OneHanded.
    class_data[11] = 3; // HeavyArmor.

    let mut config = npc::Config {
        flags: npc::flags::AUTO_CALC_STATS,
        level: 3,
        ..npc::Config::default()
    };
    config.calc_min = 1;
    config.calc_max = 10;
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"RACE"), RACE).sub(Tag(*b"DATA"), race_data));
    plugin.push_top(Record::new(Tag(*b"CLAS"), CLASS).sub(Tag(*b"DATA"), class_data));
    plugin.push_top(
        npc::build_record(NPC, "RaceSkillActor", "Race Skill Actor", config, npc::Stats::default())
            .sub(Tag(*b"RNAM"), RACE.to_le_bytes())
            .sub(Tag(*b"CNAM"), CLASS.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Prg09.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn prg_09_auto_calculated_race_bonus_is_added_to_the_authored_skill_once() {
    // At level 3, 16 skill points across weights 2 and 3 give HeavyArmor
    // 10 class points, OneHanded 6, then one remainder point to HeavyArmor.
    // UESP's CLAS formula adds the race's +15 to HeavyArmor only.
    let mut game = game();
    let player = game.player();
    let mut world = game.world_mut();

    let first = world.place_at(id(NPC), player, false);
    let second = world.place_at(id(NPC), player, false);
    assert_eq!(world.base_actor_value(first, "heavyarmor"), 40.0);
    assert_eq!(world.base_actor_value(first, "onehanded"), 21.0);
    assert_eq!(world.base_actor_value(second, "heavyarmor"), 40.0);
    let initial = world.initial_actor_values(id(NPC)).unwrap();
    assert_eq!(
        initial
            .iter()
            .find(|(name, _)| name == "heavyarmor")
            .unwrap()
            .1,
        40.0
    );
}

#[test]
fn prg_09_race_skill_value_survives_reload_without_a_second_bonus() {
    let mut game = game();
    let player = game.player();
    let mut world = game.world_mut();
    let actor = world.place_at(id(NPC), player, false);
    assert_eq!(world.base_actor_value(actor, "heavyarmor"), 40.0);

    let saved = world.save_state();
    world.load_state(&saved).unwrap();
    assert_eq!(world.base_actor_value(actor, "heavyarmor"), 40.0);

    let second = world.place_at(id(NPC), player, false);
    assert_eq!(world.base_actor_value(second, "heavyarmor"), 40.0);
}
