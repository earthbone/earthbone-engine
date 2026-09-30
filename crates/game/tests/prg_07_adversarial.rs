// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial PRG-07 threshold and character progression cases.

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{self, Config, Stats},
    Game,
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn game(base: Option<f32>, multiplier: Option<f32>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        0x802,
        "Player",
        "Player",
        Config::default(),
        Stats::default(),
    ));
    plugin.push_top(
        Record::new(T(b"AVIF"), 0x803)
            .sub(T(b"EDID"), b"AVOneHanded\0".to_vec())
            .sub(
                T(b"AVSK"),
                [2.0f32, 1.0, 0.2, 2.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<u8>>(),
            ),
    );
    if let Some(value) = base {
        plugin.push_top(
            Record::new(T(b"GMST"), 0x800)
                .sub(T(b"EDID"), b"fXPLevelUpBase\0".to_vec())
                .sub(T(b"DATA"), value.to_le_bytes()),
        );
    }
    if let Some(value) = multiplier {
        plugin.push_top(
            Record::new(T(b"GMST"), 0x801)
                .sub(T(b"EDID"), b"fXPLevelUpMult\0".to_vec())
                .sub(T(b"DATA"), value.to_le_bytes()),
        );
    }
    plugin.push_top(
        Record::new(T(b"GMST"), 0x804)
            .sub(T(b"EDID"), b"fSkillUseCurve\0".to_vec())
            .sub(T(b"DATA"), 2.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Progression.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn prg_07_exact_threshold_and_repeated_calls_award_each_level_once() {
    let mut game = game(Some(10.0), Some(5.0));
    let player = game.player();
    let mut world = game.world_mut();
    world.actor(player).unwrap();

    world.advance_character_experience(player, 15.0).unwrap();
    assert_eq!(world.actor_ref(player).unwrap().level, 2);
    assert_eq!(world.level_up_choices(player), Some(1));
    assert_eq!(world.actor_ref(player).unwrap().character_experience, 0.0);
    world.advance_character_experience(player, 0.0).unwrap();
    assert_eq!(world.level_up_choices(player), Some(1));
}

#[test]
fn prg_07_multiple_thresholds_use_the_new_level_each_time() {
    let mut game = game(Some(10.0), Some(5.0));
    let player = game.player();
    let mut world = game.world_mut();
    world.actor(player).unwrap();

    world.advance_character_experience(player, 40.0).unwrap();
    assert_eq!(world.actor_ref(player).unwrap().level, 3);
    assert_eq!(world.level_up_choices(player), Some(2));
    assert_eq!(world.actor_ref(player).unwrap().character_experience, 0.0);
}

#[test]
fn prg_07_skill_level_gains_feed_character_level_progression() {
    let mut game = game(Some(1.0), Some(0.0));
    let player = game.player();
    let mut world = game.world_mut();
    world.actor(player).unwrap();
    world.set_actor_value(player, "onehanded", 5.0).unwrap();

    world.advance_skill(player, "onehanded", 10.0).unwrap();

    assert!(world.actor_ref(player).unwrap().level > 1);
}

#[test]
fn prg_07_missing_or_nonpositive_threshold_rejects_without_mutation() {
    for (base, multiplier) in [
        (None, Some(5.0)),
        (Some(10.0), None),
        (Some(-10.0), Some(0.0)),
    ] {
        let mut game = game(base, multiplier);
        let player = game.player();
        let mut world = game.world_mut();
        world.actor(player).unwrap();
        let result = world.advance_character_experience(player, 100.0);
        assert!(result.is_err());
        assert_eq!(world.actor_ref(player).unwrap().level, 1);
        assert_eq!(world.actor_ref(player).unwrap().character_experience, 0.0);
        assert_eq!(world.level_up_choices(player), Some(0));
    }
}
