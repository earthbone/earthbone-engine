// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Boundary checks for RACE skill boost actor-value indices.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};

const RACE: u32 = 0x700;
const CLASS: u32 = 0x701;
const NPC: u32 = 0x800;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    // xEdit RACE DATA stores full actor-value indices, while CLAS weights
    // and NPC DNAM skills use the 18-skill ordering beginning at AV 6.
    let mut race_data = vec![0; 128];
    race_data[0..4].copy_from_slice(&[6, 7, 23, 9]);
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

    let mut class_data = vec![0; 35];
    class_data[6] = 1;
    class_data[23] = 1;
    let config = npc::Config {
        flags: npc::flags::AUTO_CALC_STATS,
        level: 1,
        ..npc::Config::default()
    };
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"RACE"), RACE).sub(Tag(*b"DATA"), race_data));
    plugin.push_top(Record::new(Tag(*b"CLAS"), CLASS).sub(Tag(*b"DATA"), class_data));
    plugin.push_top(
        npc::build_record(NPC, "BoundarySkills", "Boundary Skills", config, npc::Stats::default())
            .sub(Tag(*b"RNAM"), RACE.to_le_bytes())
            .sub(Tag(*b"CNAM"), CLASS.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Prg09Boundary.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn prg_09_first_and_last_skill_boost_indices_map_to_correct_skills() {
    // CLAS.wikitext gives 15 + racial bonus at level 1. AV 6 is OneHanded,
    // AV 23 is Enchanting, and adjacent AV 7 is TwoHanded.
    let mut game = game();
    let player = game.player();
    let mut world = game.world_mut();
    let actor = world.place_at(id(NPC), player, false);
    assert_eq!(world.base_actor_value(actor, "onehanded"), 22.0);
    assert_eq!(world.base_actor_value(actor, "twohanded"), 15.0);
    assert_eq!(world.base_actor_value(actor, "enchanting"), 24.0);

    let saved = world.save_state();
    world.load_state(&saved).unwrap();
    assert_eq!(world.base_actor_value(actor, "onehanded"), 22.0);
    assert_eq!(world.base_actor_value(actor, "enchanting"), 24.0);
}
