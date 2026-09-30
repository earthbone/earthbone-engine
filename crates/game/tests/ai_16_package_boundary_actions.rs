// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{package, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x800;
const ACTOR: u32 = 0x801;
const PACKAGE: u32 = 0x802;
const IDLE: u32 = 0x803;
const TOPIC: u32 = 0x804;
const SECOND_PACKAGE: u32 = 0x805;
const END_IDLE: u32 = 0x806;

fn id(form: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId {
        file: 0,
        object: form,
    }
}

fn fixture(idle: Option<u32>, topic: Option<u32>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"NPC_"), BASE)
            .zsub(T(b"EDID"), "ActorBase")
            .sub(T(b"PKID"), PACKAGE.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .zsub(T(b"EDID"), "PackageActor")
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), [0; 24]),
    );
    let mut record =
        package::build_record(PACKAGE, "BoundaryPackage", 0, package::Schedule::default());
    record = record
        .sub(T(b"POBA"), Vec::new())
        .sub(T(b"INAM"), idle.unwrap_or(0).to_le_bytes());
    let mut topic_data = Vec::new();
    topic_data.extend_from_slice(&0u32.to_le_bytes());
    topic_data.extend_from_slice(&topic.unwrap_or(0).to_le_bytes());
    record = record.sub(T(b"PDTO"), topic_data);
    plugin.push_top(record);
    if idle.is_some() {
        plugin.push_top(Record::new(T(b"IDLE"), IDLE).zsub(T(b"EDID"), "BoundaryIdle"));
    }
    if topic.is_some() {
        plugin.push_top(Record::new(T(b"DIAL"), TOPIC).zsub(T(b"EDID"), "BoundaryTopic"));
    }
    let mut order = LoadOrder::new();
    order.add("Ai16.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.player();
    game.world_mut().drive_actor(id(ACTOR));
    game
}

#[test]
fn ai_16_change_and_end_idles_start_at_their_package_boundaries() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"NPC_"), BASE)
            .zsub(T(b"EDID"), "ActorBase")
            .sub(T(b"PKID"), PACKAGE.to_le_bytes())
            .sub(T(b"PKID"), SECOND_PACKAGE.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .zsub(T(b"EDID"), "PackageActor")
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), [0; 24]),
    );
    let first_schedule = package::Schedule {
        hour: 0,
        duration_minutes: 60,
        ..package::Schedule::default()
    };
    let second_schedule = package::Schedule {
        hour: 1,
        duration_minutes: 60,
        ..package::Schedule::default()
    };
    plugin.push_top(
        package::build_record(PACKAGE, "FirstPackage", 0, first_schedule)
            .sub(T(b"POCA"), Vec::new())
            .sub(T(b"INAM"), IDLE.to_le_bytes())
            .sub(T(b"PDTO"), [0; 8]),
    );
    plugin.push_top(
        package::build_record(SECOND_PACKAGE, "SecondPackage", 0, second_schedule)
            .sub(T(b"POEA"), Vec::new())
            .sub(T(b"INAM"), END_IDLE.to_le_bytes())
            .sub(T(b"PDTO"), [0; 8]),
    );
    plugin.push_top(Record::new(T(b"IDLE"), IDLE).zsub(T(b"EDID"), "ChangeIdle"));
    plugin.push_top(Record::new(T(b"IDLE"), END_IDLE).zsub(T(b"EDID"), "EndIdle"));
    let mut order = LoadOrder::new();
    order.add("Ai16.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.player();
    game.world_mut().clock.game_days = 0.5 / 24.0;
    game.world_mut().drive_actor(id(ACTOR));
    game.tick(0.1);
    game.world_mut().clock.game_days = 1.5 / 24.0;
    game.world_mut().actor(id(ACTOR)).unwrap().ai.reevaluate = true;
    game.tick(0.1);
    assert_eq!(
        game.world().animation_events,
        vec![(id(ACTOR), "ChangeIdle".to_string())],
        "a replacement starts the old package change idle"
    );
    game.world_mut().clock.game_days = 3.0 / 24.0;
    game.world_mut().actor(id(ACTOR)).unwrap().ai.reevaluate = true;
    game.tick(0.1);
    assert_eq!(
        game.world().animation_events,
        vec![
            (id(ACTOR), "ChangeIdle".to_string()),
            (id(ACTOR), "EndIdle".to_string())
        ],
        "the last package starts its end idle"
    );
}

#[test]
fn ai_16_begin_idle_starts_for_a_living_conscious_actor() {
    let mut game = fixture(Some(IDLE), None);
    game.tick(0.1);
    assert_eq!(
        game.world().animation_events,
        vec![(id(ACTOR), "BoundaryIdle".to_string())],
        "an eligible actor receives the package begin idle"
    );
}

#[test]
fn ai_16_begin_idle_does_not_start_for_a_dead_actor() {
    let mut game = fixture(Some(IDLE), None);
    game.world_mut().actor(id(ACTOR)).unwrap().dead = true;
    game.tick(0.1);
    assert!(
        game.world().animation_events.is_empty(),
        "a dead actor does not receive the package begin idle"
    );
}

#[test]
fn ai_16_begin_topic_opens_only_for_an_eligible_actor() {
    let mut game = fixture(None, Some(TOPIC));
    game.tick(0.1);
    assert_eq!(
        game.world().dialogue_session().map(|session| session.topic),
        Some(Some(id(TOPIC))),
        "an eligible actor opens the package topic"
    );
}

#[test]
fn ai_16_begin_topic_does_not_open_for_an_unconscious_actor() {
    let mut game = fixture(None, Some(TOPIC));
    game.world_mut().actor(id(ACTOR)).unwrap().unconscious = true;
    game.tick(0.1);
    assert!(
        game.world().dialogue_session().is_none(),
        "an unconscious actor does not open the package topic"
    );
}
