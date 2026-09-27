// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::{Game, World};

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

fn template(target: u32) -> Record {
    let rec = package::close_header(package::build_template(TEMPLATE, "MountedPatrol"), None)
        .zsub(T(b"ANAM"), "SingleRef");
    let mut data = 0_i32.to_le_bytes().to_vec();
    data.extend_from_slice(&target.to_le_bytes());
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

fn order(with_horse: bool, owned: bool) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(template(A));
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
    if with_horse {
        let horse = placed(*b"ACHR", HORSE, HORSE_BASE, 5.0).sub(
            T(b"XOWN"),
            (if owned { RIDER_BASE } else { 0x812 }).to_le_bytes(),
        );
        plugin.push_top(horse);
    }
    let mut order = LoadOrder::new();
    order
        .add(
            "MountedPatrol.esm",
            Plugin::parse(&plugin.to_bytes()).unwrap(),
        )
        .unwrap();
    order
}

fn game(with_horse: bool, owned: bool) -> Game {
    let game = Game::new(order(with_horse, owned));
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

#[test]
fn ai_89_patrol_uses_owned_available_horse_and_tracks_route_movement() {
    let mut game = game(true, true);
    game.tick(0.0);
    assert_eq!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .patrol
            .as_ref()
            .unwrap()
            .mount,
        Some(id(HORSE))
    );

    game.tick(1.0);
    let rider = game.world_mut().reference(id(ACTOR)).unwrap().position;
    let horse = game.world_mut().reference(id(HORSE)).unwrap().position;
    assert_eq!(horse, rider);
    assert_eq!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .patrol
            .as_ref()
            .unwrap()
            .current,
        id(A)
    );
}

#[test]
fn ai_89_patrol_continues_on_foot_when_owned_horse_is_unavailable() {
    let mut game = game(true, true);
    game.tick(0.0);
    game.world_mut().reference(id(HORSE)).unwrap().enabled = false;
    game.tick(1.0);
    let world = game.world();
    let patrol = world
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .patrol
        .as_ref()
        .unwrap();
    assert_eq!(patrol.mount, None);
    assert_eq!(patrol.start, id(A));
    assert_eq!(patrol.current, id(A));
}

#[test]
fn ai_89_patrol_dismounts_when_the_horse_owner_changes() {
    let mut game = game(true, true);
    game.tick(0.0);
    game.world_mut().reference(id(HORSE)).unwrap().owner = None;
    game.tick(0.25);
    let world = game.world();
    let patrol = world
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .patrol
        .as_ref()
        .unwrap();
    assert_eq!(patrol.mount, None);
    assert_eq!(patrol.start, id(A));
    assert_eq!(patrol.current, id(A));
}

#[test]
fn ai_89_patrol_save_keeps_mount_and_route_for_dismount_recovery() {
    let mut game = game(true, true);
    game.tick(0.25);
    let saved = game.world().save_state();
    let mut restored = World::new(order(true, true));
    restored.load_state(&saved).unwrap();
    let patrol = restored
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .patrol
        .as_ref()
        .unwrap();
    assert_eq!(patrol.mount, Some(id(HORSE)));
    assert_eq!(patrol.start, id(A));
    assert_eq!(patrol.current, id(A));
}

#[test]
fn ai_89_patrol_without_owned_available_horse_walks_on_foot() {
    let mut game = game(false, true);
    game.tick(0.0);
    assert_eq!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .patrol
            .as_ref()
            .unwrap()
            .mount,
        None
    );
}

#[test]
fn ai_89_patrol_does_not_mount_an_unowned_horse() {
    let mut game = game(true, false);
    game.tick(0.0);
    assert_eq!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .patrol
            .as_ref()
            .unwrap()
            .mount,
        None
    );
}

#[test]
fn ai_89_patrol_selects_a_horse_owned_through_runtime_state() {
    let mut game = game(true, false);
    game.world_mut().reference(id(HORSE)).unwrap().owner = Some(id(RIDER_BASE));
    game.tick(0.0);
    assert_eq!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .patrol
            .as_ref()
            .unwrap()
            .mount,
        Some(id(HORSE))
    );
}
