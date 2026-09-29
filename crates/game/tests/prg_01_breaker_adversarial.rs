// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{self, Config, Stats},
    Game,
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};

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
                    .collect::<Vec<_>>(),
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
fn prg_01_breaker_repeated_use_recomputes_the_curve_after_each_level_and_round_trips() {
    let mut game = game();
    let player = game.player();
    let mut world = game.world_mut();
    let actor = world.place_at(ACTOR, player, false);
    for _ in 0..4 {
        world.advance_skill(actor, "onehanded", 1.0).unwrap();
    }
    assert_eq!(world.base_actor_value(actor, "onehanded"), 6.0);
    assert!((world.skill_experience(actor, "onehanded").unwrap() - 5.0).abs() < 0.0001);
    let saved = world.save_state();
    world.load_state(&saved).unwrap();
    assert_eq!(world.base_actor_value(actor, "onehanded"), 6.0);
    assert!((world.skill_experience(actor, "onehanded").unwrap() - 5.0).abs() < 0.0001);
}

#[test]
fn prg_01_breaker_level_cap_discards_excess_experience() {
    let mut game = game();
    let player = game.player();
    let mut world = game.world_mut();
    let actor = world.place_at(ACTOR, player, false);
    world.set_actor_value(actor, "onehanded", 100.0).unwrap();
    world.advance_skill(actor, "onehanded", 100.0).unwrap();
    assert_eq!(world.base_actor_value(actor, "onehanded"), 100.0);
    assert_eq!(world.skill_experience(actor, "onehanded"), Some(0.0));
}
