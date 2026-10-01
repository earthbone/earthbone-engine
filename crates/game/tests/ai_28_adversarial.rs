// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! ForceGreet walks to its package target and starts its assigned topic once.

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::Procedure;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const PLAYER_BASE: u32 = 0x803;
const ACTOR: u32 = 0x100;
const PLAYER: u32 = 0x14;
const QUEST: u32 = 0x810;
const TOPIC: u32 = 0x900;
const INFO: u32 = 0x901;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn placed(form: u32, base: u32, x: f32) -> Record {
    let mut position = vec![0; 24];
    position[..4].copy_from_slice(&x.to_le_bytes());
    Record::new(T(b"ACHR"), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), position)
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    let template = package::build_template(TEMPLATE, "ForceGreet");
    let template = package::close_header(template, None);
    let template = package::finish(template);
    let template = package::with_procedure(template, "ForceGreet", &[0, 1], false);
    let template = package::with_input_name(template, 0, "Target", true);
    let template = package::with_input_name(template, 1, "Greeting", true);
    plugin.push_top(package::with_actions(template));

    let mut package_record = package::build_record(PACK, "GreetingPackage", 0, Schedule::default());
    package_record = package::close_header(package_record, Some(TEMPLATE));
    let mut target = vec![0; 12];
    target[4..8].copy_from_slice(&PLAYER.to_le_bytes());
    target[8..12].copy_from_slice(&1u32.to_le_bytes());
    package_record = package_record
        .zsub(T(b"ANAM"), "Target")
        .sub(T(b"PTDA"), target)
        .sub(T(b"UNAM"), [0]);
    let mut topic = 0u32.to_le_bytes().to_vec();
    topic.extend_from_slice(&TOPIC.to_le_bytes());
    package_record = package_record
        .zsub(T(b"ANAM"), "Topic")
        .sub(T(b"PDTO"), topic)
        .sub(T(b"UNAM"), [1]);
    plugin.push_top(package::with_actions(package::finish(package_record)));

    plugin.push_top(
        npc::build_record(
            BASE,
            "Greeter",
            "Greeter",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACK.to_le_bytes()),
    );
    plugin.push_top(npc::build_record(
        PLAYER_BASE,
        "Player",
        "Player",
        Config::default(),
        Stats::default(),
    ));
    plugin.push_top(placed(ACTOR, BASE, 100.0));
    plugin.push_top(placed(PLAYER, PLAYER_BASE, 0.0));

    let mut quest_data = vec![0; 12];
    quest_data[2] = 50;
    plugin.push_top(
        Record::new(T(b"QUST"), QUEST)
            .sub(T(b"DNAM"), quest_data)
            .sub(T(b"NEXT"), [])
            .sub(T(b"INDX"), [0, 0, 0, 0]),
    );
    let topic_record = Record::new(T(b"DIAL"), TOPIC)
        .zsub(T(b"FULL"), "Greeting")
        .sub(T(b"QNAM"), QUEST.to_le_bytes())
        .sub(T(b"BNAM"), (TOPIC + 0x1000).to_le_bytes())
        .sub(T(b"DATA"), [0, 0, 0, 0])
        .sub(T(b"SNAM"), *b"CUST")
        .sub(T(b"PNAM"), 50.0f32.to_le_bytes());
    let mut response = vec![0; 24];
    response[12] = 1;
    let info = Record::new(T(b"INFO"), INFO)
        .sub(T(b"ENAM"), [0, 0, 0, 0])
        .sub(T(b"TRDT"), response)
        .zsub(T(b"NAM1"), "Hello");
    let mut dialogue_group = Group::new(GroupKind::Top(T(b"DIAL")));
    dialogue_group
        .entries
        .push(Entry::Record(plugin.push_record(topic_record)));
    let mut topic_children = Group::new(GroupKind::TopicChildren(TOPIC));
    topic_children
        .entries
        .push(Entry::Record(plugin.push_record(info)));
    dialogue_group.entries.push(Entry::Group(topic_children));
    plugin.groups.push(dialogue_group);
    plugin.push_top(
        Record::new(T(b"DLBR"), TOPIC + 0x1000)
            .sub(T(b"QNAM"), QUEST.to_le_bytes())
            .sub(T(b"TNAM"), 0u32.to_le_bytes())
            .sub(T(b"DNAM"), 1u32.to_le_bytes())
            .sub(T(b"SNAM"), TOPIC.to_le_bytes()),
    );

    let mut order = LoadOrder::new();
    order
        .add("ForceGreet.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    order
}

fn game(alpha: f32, ghost: bool) -> Game {
    let mut game = Game::new(order());
    game.world_mut().drive_actor(id(ACTOR));
    game.world_mut().drive_actor(id(PLAYER));
    game.world_mut().reference(id(ACTOR)).unwrap().alpha = alpha;
    game.world_mut().actor(id(ACTOR)).unwrap().ghost = ghost;
    game.start_quest(id(QUEST));
    game.tick(0.0);
    game
}

#[test]
fn ai_28_force_greet_approaches_and_opens_the_assigned_topic_once() {
    let mut game = game(1.0, false);
    assert_eq!(
        game.world().actor_ref(id(ACTOR)).unwrap().ai.procedure,
        Procedure::ForceGreet
    );
    game.tick(2.0);
    assert_eq!(
        game.world_mut().reference(id(ACTOR)).unwrap().position[0],
        0.0
    );
    let world = game.world();
    let session = world.dialogue_session().unwrap();
    assert_eq!(session.speaker, id(ACTOR));
    assert_eq!(session.listener, id(PLAYER));
    assert_eq!(session.topic, Some(id(TOPIC)));
    assert!(session.current_info.is_some());
    drop(world);

    game.close_dialogue();
    game.tick(1.0);
    assert!(game.world().dialogue_session().is_none());
    assert!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .force_greet_started
    );
}

#[test]
fn ai_28_force_greet_invisible_actor_needs_ghost_flag_to_start() {
    let mut invisible = game(0.5, false);
    invisible.tick(2.0);
    assert!(invisible.world().dialogue_session().is_none());
    assert!(
        invisible
            .world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .force_greet_started
    );

    let mut ghost = game(0.5, true);
    ghost.tick(2.0);
    assert_eq!(
        ghost.world().dialogue_session().unwrap().topic,
        Some(id(TOPIC))
    );
}

#[test]
fn ai_28_force_greet_attempt_state_survives_save_and_load() {
    let mut game = game(1.0, false);
    game.tick(2.0);
    game.close_dialogue();
    let bytes = game.world().save_state();
    let mut restored = Game::new(order());
    restored.world_mut().load_state(&bytes).unwrap();
    restored.tick(2.0);
    assert!(restored.world().dialogue_session().is_none());
    assert!(
        restored
            .world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .force_greet_started
    );
    let mut future = bytes;
    let future_version = soe_game::world::SAVE_VERSION + 1;
    future[4..6].copy_from_slice(&future_version.to_le_bytes());
    assert!(restored.world_mut().load_state(&future).is_err());
}
