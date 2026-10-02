// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{self, Config, Stats},
    Game,
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn make_game() -> Game {
    let mut race_data = vec![0u8; 128];
    race_data[0] = 11;
    race_data[1] = 0xf1; // -15, as the signed racial skill bonus.
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
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"RACE"), 0x700).sub(T(b"DATA"), race_data));
    let mut stats = Stats::default();
    stats.skills[5] = 20;
    plugin.push_top(
        npc::build_record(0x800, "Guard", "Guard", Config::default(), stats)
            .sub(T(b"RNAM"), 0x700u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("RaceBonus.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn prg_09_manual_dnam_cache_is_not_adjusted_by_a_signed_race_bonus() {
    let mut game = make_game();
    let player = game.player();
    let mut world = game.world_mut();
    let actor = world.place_at(id(0x800), player, false);

    assert_eq!(world.base_actor_value(actor, "heavyarmor"), 20.0);
    assert_eq!(world.base_actor_value(actor, "heavyarmor"), 20.0);
}

#[test]
fn prg_09_saved_runtime_skill_value_is_not_reseeded_on_reload() {
    let mut game = make_game();
    let player = game.player();
    let mut world = game.world_mut();
    let actor = world.place_at(id(0x800), player, false);
    assert_eq!(world.base_actor_value(actor, "heavyarmor"), 20.0);
    world.set_actor_value(actor, "heavyarmor", 37.0).unwrap();
    let saved = world.save_state();

    world.load_state(&saved).unwrap();
    assert_eq!(world.base_actor_value(actor, "heavyarmor"), 37.0);
}
