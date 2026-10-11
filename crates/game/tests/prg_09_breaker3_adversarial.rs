// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! PRG-09 auto-calculated race skill bonus and reload checks.
//! Sources: UESP CLAS Weight Calculations, TES5Edit CLAS and RACE DATA.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn auto_actor_game() -> Game {
    let mut race_data = vec![0u8; 128];
    race_data[0..2].copy_from_slice(&[11, 7]);
    race_data[2..4].copy_from_slice(&[23, 99]);
    race_data[4..6].copy_from_slice(&[5, 88]);
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
    class_data[6] = 1;

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"RACE"), 0x700).sub(T(b"DATA"), race_data));
    plugin.push_top(Record::new(T(b"CLAS"), 0x701).sub(T(b"DATA"), class_data));
    plugin.push_top(
        npc::build_record(
            0x800,
            "RaceSkillActor",
            "Race Skill Actor",
            npc::Config {
                flags: npc::flags::AUTO_CALC_STATS,
                level: 3,
                ..npc::Config::default()
            },
            npc::Stats::default(),
        )
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes())
        .sub(T(b"CNAM"), 0x701u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("RaceSkillAdversarial.esm", plugin).unwrap();
    Game::new(order)
}

fn value(values: &[(String, f32)], name: &str) -> f32 {
    values
        .iter()
        .find(|(candidate, _)| candidate == name)
        .unwrap()
        .1
}

#[test]
fn prg_09_auto_skill_uses_valid_race_bonus_once_and_survives_reload() {
    let mut game = auto_actor_game();
    let initial = game.world().initial_actor_values(id(0x800)).unwrap();
    // OneHanded is skill index 6, so 16 points at level 3 plus the racial 7.
    assert_eq!(value(&initial, "onehanded"), 38.0);
    // Invalid non-skill AV 5 must not alias the first skill slot.
    assert_eq!(value(&initial, "marksman"), 15.0);

    let player = game.player();
    let mut world = game.world_mut();
    let actor = world.place_at(id(0x800), player, false);
    assert_eq!(world.base_actor_value(actor, "onehanded"), 38.0);
    world.set_actor_value(actor, "onehanded", 51.0).unwrap();
    let saved = world.save_state();
    world.load_state(&saved).unwrap();
    assert_eq!(world.base_actor_value(actor, "onehanded"), 51.0);

    let second = world.place_at(id(0x800), player, false);
    assert_eq!(world.base_actor_value(second, "onehanded"), 38.0);
}
