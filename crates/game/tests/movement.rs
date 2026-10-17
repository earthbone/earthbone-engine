// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! MOVT directional fields and live race/actor inputs, from xEdit and UESP.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    movement::{Gait, MovementType},
    Game,
};

use crate::common::installed;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn sped(rates: [f32; 8]) -> Vec<u8> {
    rates
        .into_iter()
        .chain([0.0; 3])
        .flat_map(f32::to_le_bytes)
        .collect()
}

fn fixture() -> Game {
    fixture_with_race(
        Record::new(T(b"RACE"), 1)
            .sub(T(b"WKMV"), 2_u32.to_le_bytes())
            .sub(T(b"RNMV"), 2_u32.to_le_bytes())
            .sub(T(b"SPMV"), 3_u32.to_le_bytes())
            .sub(T(b"SNMV"), 4_u32.to_le_bytes()),
    )
}

fn fixture_with_race(race: Record) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race);
    plugin.push_top(Record::new(T(b"MISC"), 5).sub(
        T(b"DATA"),
        [0_u32.to_le_bytes(), 6.0_f32.to_le_bytes()].concat(),
    ));
    for (object, speeds) in [
        (2, [70.0, 210.0, 80.0, 220.0, 100.0, 300.0, 60.0, 180.0]),
        (3, [300.0, 400.0, 310.0, 410.0, 450.0, 600.0, 200.0, 250.0]),
        (4, [20.0, 60.0, 25.0, 70.0, 30.0, 90.0, 15.0, 45.0]),
    ] {
        plugin.push_top(Record::new(T(b"MOVT"), object).sub(T(b"SPED"), sped(speeds)));
    }
    let mut order = LoadOrder::new();
    order.add("movement.esp", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut().actor(player).unwrap().race = Some(GlobalFormId { file: 0, object: 1 });
    game
}

#[test]
fn synthetic_movt_reads_each_direction_in_both_sped_versions() {
    let bytes = sped([11.0, 12.0, 21.0, 22.0, 31.0, 32.0, 41.0, 42.0]);
    for length in [40, 44] {
        let record = Record::new(T(b"MOVT"), 1).sub(T(b"SPED"), &bytes[..length]);
        let view = MovementType::from_record(&record).unwrap();
        assert_eq!(
            [
                view.walk.left,
                view.run.left,
                view.walk.right,
                view.run.right,
                view.walk.forward,
                view.run.forward,
                view.walk.back,
                view.run.back
            ],
            [11.0, 12.0, 21.0, 22.0, 31.0, 32.0, 41.0, 42.0]
        );
    }
}

#[test]
fn malformed_movt_returns_errors_and_zero_is_a_valid_direction() {
    for bad in [f32::NAN, f32::INFINITY, -1.0] {
        let mut values = [1.0; 8];
        values[0] = bad;
        assert!(MovementType::from_record(
            &Record::new(T(b"MOVT"), 1).sub(T(b"SPED"), sped(values))
        )
        .is_err());
    }
    for length in [0, 32, 39, 41, 45] {
        assert!(MovementType::from_record(
            &Record::new(T(b"MOVT"), 1).sub(T(b"SPED"), vec![0; length])
        )
        .is_err());
    }
    assert!(MovementType::from_record(&Record::new(T(b"RACE"), 1)).is_err());
    assert!(MovementType::from_record(&Record::new(T(b"MOVT"), 1)).is_err());
    let zero =
        MovementType::from_record(&Record::new(T(b"MOVT"), 1).sub(T(b"SPED"), sped([0.0; 8])))
            .unwrap();
    assert_eq!(zero.run.forward, 0.0);
}

#[test]
fn missing_npc_acbs_defaults_to_100_but_authored_zero_speedmult_is_retained() {
    use soe_game::npc::{self, Config, Npc, Stats};
    assert_eq!(Config::default().speed, 100);
    let missing = Record::new(T(b"NPC_"), 1);
    assert_eq!(Npc::from_record(&missing, false).unwrap().config.speed, 100);
    let explicit_zero = npc::build_record(
        1,
        "still",
        "Still",
        Config {
            speed: 0,
            ..Default::default()
        },
        Stats::default(),
    );
    assert_eq!(
        Npc::from_record(&explicit_zero, false)
            .unwrap()
            .config
            .speed,
        0
    );
}

#[test]
fn missing_race_links_use_sparse_dobj_movement_defaults() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"RACE"), 1).sub(T(b"RNMV"), 0_u32.to_le_bytes()));
    let mut defaults = Vec::new();
    for (key, movement) in [
        (*b"DMWL", 2_u32),
        (*b"DMRN", 2),
        (*b"DMSP", 3),
        (*b"DMSN", 4),
    ] {
        defaults.extend_from_slice(&key);
        defaults.extend_from_slice(&movement.to_le_bytes());
    }
    plugin.push_top(Record::new(T(b"DOBJ"), 0x31).sub(T(b"DNAM"), defaults));
    for (object, rate) in [(2, 300.0), (3, 500.0), (4, 120.0)] {
        plugin.push_top(Record::new(T(b"MOVT"), object).sub(T(b"SPED"), sped([rate; 8])));
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut().actor(player).unwrap().race = Some(GlobalFormId { file: 0, object: 1 });
    let profile = game.world_mut().actor_movement(player);
    assert_eq!(profile.run.run.forward, 300.0);
    assert_eq!(profile.sprint.run.forward, 500.0);
    assert_eq!(profile.sneak.walk.forward, 120.0);
}

#[test]
fn actor_gait_table_uses_the_race_types_and_live_speedmult() {
    let mut game = fixture();
    let player = game.player();
    for multiplier in [100.0, 150.0, 0.0] {
        game.world_mut()
            .set_actor_value(player, "speedmult", multiplier)
            .unwrap();
        let profile = game.world_mut().actor_movement(player);
        for (gait, rates) in [
            (Gait::Walk, [70.0, 80.0, 100.0, 60.0]),
            (Gait::FastWalk, [210.0, 220.0, 300.0, 180.0]),
            (Gait::Jog, [70.0, 80.0, 100.0, 60.0]),
            (Gait::Run, [210.0, 220.0, 300.0, 180.0]),
            (Gait::Sprint, [400.0, 410.0, 600.0, 250.0]),
            (Gait::SneakWalk, [20.0, 25.0, 30.0, 15.0]),
            (Gait::SneakRun, [60.0, 70.0, 90.0, 45.0]),
        ] {
            for ((x, y), expected) in [(-1.0, 0.0), (1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
                .into_iter()
                .zip(rates)
            {
                assert_eq!(profile.speed(gait, x, y), expected * multiplier / 100.0);
            }
        }
        assert_eq!(profile.speed(Gait::Run, 0.0, 0.0), 0.0);
        assert!(profile.speed(Gait::Run, 1.0, 1.0) <= 300.0 * multiplier / 100.0);
    }
}

#[test]
fn carried_inventory_and_actor_value_changes_limit_encumbered_gaits() {
    let mut game = fixture();
    let player = game.player();
    game.world_mut()
        .set_actor_value(player, "carryweight", 10.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(player, "inventoryweight", 10.0)
        .unwrap();
    assert!(!game.world_mut().actor_movement(player).encumbered);
    game.world_mut()
        .mod_actor_value(player, "inventoryweight", 1.0)
        .unwrap();
    let profile = game.world_mut().actor_movement(player);
    assert!(profile.encumbered);
    assert_eq!(profile.speed(Gait::Sprint, 0.0, 1.0), 100.0);
    game.world_mut()
        .mod_actor_value(player, "carryweight", 1.0)
        .unwrap();
    assert!(!game.world_mut().actor_movement(player).encumbered);
}

#[test]
fn inventory_pickup_and_removal_change_encumbrance_without_a_weight_cache() {
    let mut game = fixture();
    let player = game.player();
    let item = GlobalFormId { file: 0, object: 5 };
    let mut world = game.world_mut();
    world.set_actor_value(player, "carryweight", 10.0).unwrap();
    world.add_item(player, item, 1);
    assert_eq!(world.carried_weight(player), 6.0);
    assert!(!world.actor_movement(player).encumbered);
    world.add_item(player, item, 1);
    assert_eq!(world.carried_weight(player), 12.0);
    assert!(world.actor_movement(player).encumbered);
    world.remove_item(player, item, 1, None);
    assert!(!world.actor_movement(player).encumbered);
}

#[test]
fn race_overrides_change_only_matching_positive_directional_fields() {
    let mut game = fixture_with_race(
        Record::new(T(b"RACE"), 1)
            .sub(T(b"WKMV"), 2_u32.to_le_bytes())
            .sub(T(b"RNMV"), 2_u32.to_le_bytes())
            .sub(T(b"MTYP"), 3_u32.to_le_bytes())
            .sub(T(b"SPED"), sped([999.0; 8]))
            .sub(T(b"MTYP"), 2_u32.to_le_bytes())
            .sub(
                T(b"SPED"),
                sped([0.0, 0.0, 0.0, 0.0, 130.0, 330.0, 0.0, 0.0]),
            ),
    );
    let player = game.player();
    let profile = game.world_mut().actor_movement(player);
    assert_eq!(profile.walk.walk.forward, 130.0);
    assert_eq!(profile.run.run.forward, 330.0);
    assert_eq!(profile.run.run.left, 210.0);
    assert_eq!(profile.walk.walk.back, 60.0);
}

#[test]
fn winning_race_links_use_the_override_plugins_formid_namespace() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(T(b"RACE"), 1).sub(T(b"RNMV"), 2_u32.to_le_bytes()));
    base.push_top(Record::new(T(b"MOVT"), 2).sub(T(b"SPED"), sped([100.0; 8])));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["base.esm".into()],
        ..Default::default()
    });
    patch.push_top(Record::new(T(b"RACE"), 1).sub(T(b"RNMV"), 0x0100_0002_u32.to_le_bytes()));
    patch.push_top(Record::new(T(b"MOVT"), 0x0100_0002).sub(T(b"SPED"), sped([500.0; 8])));
    let mut order = LoadOrder::new();
    order.add("base.esm", base).unwrap();
    order.add("patch.esp", patch).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut().actor(player).unwrap().race = Some(GlobalFormId { file: 0, object: 1 });
    assert_eq!(
        game.world_mut().actor_movement(player).run.run.forward,
        500.0
    );
}

#[test]
fn installed_nord_movt_run_and_sprint_exceed_walk_with_dlc() {
    let Some(mut game) = installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        return;
    };
    let player = game.player();
    let mut world = game.world_mut();
    let nord = world.form("NordRace").expect("installed NordRace");
    let race = world.actor(player).expect("installed player").race;
    assert_eq!(
        race,
        Some(nord),
        "new player race must come from the installed NPC"
    );
    let default = world
        .form("NPC_Default_MT")
        .expect("installed default MOVT");
    let view = MovementType::from_record(world.record(default).unwrap()).unwrap();
    assert!(
        view.run.forward > view.walk.forward * 1.5,
        "run must clearly exceed walk"
    );
    let profile = world.actor_movement(player);
    let sprint = soe_game::footsteps::Footsteps::new(world.order())
        .default_object(*b"DMSP")
        .unwrap()
        .expect("installed sprint MOVT default");
    let sprint_view = MovementType::from_record(world.record(sprint).unwrap()).unwrap();
    assert_eq!(profile.sprint, sprint_view);
    assert!(profile.run.run.forward > profile.walk.walk.forward * 1.5);
    assert!(profile.sprint.run.forward > profile.run.run.forward * 1.25);
    eprintln!(
        "Nord MOVT: walk={}, run={}, sprint={}, sneak walk={}, sneak run={}, SpeedMult={}",
        profile.walk.walk.forward,
        profile.run.run.forward,
        profile.sprint.run.forward,
        profile.sneak.walk.forward,
        profile.sneak.run.forward,
        profile.multiplier * 100.0
    );
}
