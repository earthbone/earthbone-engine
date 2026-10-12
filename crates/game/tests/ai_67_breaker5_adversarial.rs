// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::conditions;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const RIDER_BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;
const A: u32 = 0x804;
const B: u32 = 0x805;
const C: u32 = 0x806;
const HORSE_BASE: u32 = 0x807;
const HORSE: u32 = 0x808;
const RIDER_RACE: u32 = 0x809;
const HORSE_RACE: u32 = 0x80a;
const HORSE_KEYWORD: u32 = 0x80b;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn template() -> Record {
    let rec = package::close_header(package::build_template(TEMPLATE, "MountedPatrol"), None)
        .zsub(T(b"ANAM"), "SingleRef");
    let mut data = 0_i32.to_le_bytes().to_vec();
    data.extend_from_slice(&A.to_le_bytes());
    data.extend_from_slice(&0_i32.to_le_bytes());
    let rec = rec.sub(T(b"PTDA"), data).sub(T(b"UNAM"), [7]);
    let rec = package::with_float_input(rec, 11, 0.0);
    let rec = package::with_bool_input(rec, 3, false);
    let rec = package::with_bool_input(rec, 9, false);
    let rec = package::with_bool_input(rec, 14, true);
    let rec = package::finish(package::with_bool_input(rec, 15, false));
    package::with_procedure(rec, "Patrol", &[7, 11, 3, 9, 14, 15], false)
}

fn placed(tag: [u8; 4], form: u32, base: u32, x: f32) -> Record {
    let mut data = vec![0; 24];
    data[..4].copy_from_slice(&x.to_le_bytes());
    Record::new(T(&tag), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(template());
    plugin.push_top(package::finish(package::close_header(
        package::build_record(PACK, "MountedPatrol", 0, Schedule::default()),
        Some(TEMPLATE),
    )));
    plugin.push_top(
        npc::build_record(
            RIDER_BASE,
            "Patroller",
            "Patroller",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"RNAM"), RIDER_RACE.to_le_bytes())
        .sub(T(b"PKID"), PACK.to_le_bytes()),
    );
    plugin.push_top(
        npc::build_record(
            HORSE_BASE,
            "PatrolHorse",
            "Patrol Horse",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"RNAM"), HORSE_RACE.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"RACE"), RIDER_RACE).zsub(T(b"EDID"), "HumanRace"));
    plugin.push_top(
        Record::new(T(b"RACE"), HORSE_RACE)
            .zsub(T(b"EDID"), "HorseRace")
            .sub(T(b"KWDA"), HORSE_KEYWORD.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"KYWD"), HORSE_KEYWORD).zsub(T(b"EDID"), "ActorTypeHorse"));
    plugin.push_top(placed(*b"ACHR", ACTOR, RIDER_BASE, 0.0).sub(T(b"XLKR"), A.to_le_bytes()));
    for (form, x, next) in [(A, 110.0, Some(B)), (B, 220.0, Some(C)), (C, 330.0, None)] {
        let mut marker = placed(*b"REFR", form, 0, x).sub(T(b"XPRD"), 0_f32.to_le_bytes());
        if let Some(next) = next {
            marker = marker.sub(T(b"XLKR"), next.to_le_bytes());
        }
        plugin.push_top(marker);
    }
    plugin.push_top(
        placed(*b"ACHR", HORSE, HORSE_BASE, 5.0).sub(T(b"XOWN"), RIDER_BASE.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order
        .add(
            "Ai67Mounted.esm",
            Plugin::parse(&plugin.to_bytes()).unwrap(),
        )
        .unwrap();
    order
}

fn game() -> Game {
    let game = Game::new(order());
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

fn ridden(game: &Game, subject: GlobalFormId) -> Result<bool, conditions::Error> {
    let condition = Condition {
        flags: 0,
        comparison: Comparison::Number(1.0),
        function: 714,
        parameters: [0, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    };
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(RIDER_BASE),
            subject,
            target: None,
            quest: None,
            tempering_enchanted: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
            skill_use: None,
            light_level: None,
        },
        &condition,
    )
}

#[test]
fn ai_67_ridden_query_and_mount_flags_survive_world_save_load() {
    let mut game = game();
    game.tick(0.0);
    assert_eq!(ridden(&game, id(HORSE)), Ok(true));
    let saved = game.world().save_state();

    let restored_game = Game::new(order());
    restored_game.world_mut().load_state(&saved).unwrap();
    assert_eq!(ridden(&restored_game, id(HORSE)), Ok(true));
    assert!(
        restored_game
            .world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .riding_mount
    );
    assert!(
        restored_game
            .world()
            .actor_ref(id(HORSE))
            .unwrap()
            .being_ridden
    );
}

#[test]
fn ai_67_unavailable_horse_releases_ridden_state_for_each_actor_failure() {
    for unavailable in ["dead", "unconscious"] {
        let mut game = game();
        game.tick(0.0);
        assert_eq!(ridden(&game, id(HORSE)), Ok(true));
        {
            let mut world = game.world_mut();
            let horse = world.actor(id(HORSE)).unwrap();
            if unavailable == "dead" {
                horse.dead = true;
            } else {
                horse.unconscious = true;
            }
        }
        game.tick(0.25);
        assert_eq!(ridden(&game, id(HORSE)), Ok(false), "{unavailable}");
        assert!(!game.world().actor_ref(id(ACTOR)).unwrap().riding_mount);
    }
}

#[test]
fn ai_67_repeated_patrol_ticks_do_not_toggle_mount_condition_off() {
    let mut game = game();
    for delta in [0.0, 0.0, 0.25, 0.25] {
        game.tick(delta);
        assert_eq!(ridden(&game, id(HORSE)), Ok(true));
        assert!(game.world().actor_ref(id(ACTOR)).unwrap().riding_mount);
    }
}

#[test]
fn ai_67_patrol_failure_releases_mount_condition_state() {
    let mut game = game();
    game.tick(0.0);
    assert_eq!(ridden(&game, id(HORSE)), Ok(true));
    game.world_mut().reference(id(A)).unwrap().deleted = true;

    game.tick(0.25);

    assert_eq!(ridden(&game, id(HORSE)), Ok(false));
    assert!(!game.world().actor_ref(id(ACTOR)).unwrap().riding_mount);
}

#[test]
fn ai_67_is_being_ridden_is_false_for_a_non_mount_actor() {
    let mut game = game();
    game.world_mut()
        .set_mount_rider(id(HORSE), id(ACTOR), true)
        .unwrap();

    assert_eq!(ridden(&game, id(ACTOR)), Ok(false));
}
