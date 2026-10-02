// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::Procedure;
use soe_game::package::{self, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const BASE: u32 = 0x500;
const QUEST: u32 = 0x800;
const TEMPLATE: u32 = 0x810;
const PACK: u32 = 0x811;
const TOPIC: u32 = 0x900;
const INFO: u32 = 0xA00;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture(force_greet_topic: bool) -> Game {
    fixture_with_inputs(force_greet_topic, &[])
}

fn fixture_with_inputs(force_greet_topic: bool, inputs: &[u8]) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"NPC_"), 7)
            .zsub(T(b"EDID"), "Player")
            .sub(T(b"ACBS"), vec![0; 24]),
    );
    plugin.push_top(
        Record::new(T(b"NPC_"), BASE)
            .zsub(T(b"EDID"), "Greeter")
            .sub(T(b"ACBS"), vec![0; 24])
            .sub(T(b"PKID"), PACK.to_le_bytes()),
    );
    let mut placement = vec![0; 24];
    placement[..4].copy_from_slice(&20.0f32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), placement),
    );

    let template = package::finish(package::close_header(
        package::build_template(TEMPLATE, "ForceGreet"),
        None,
    ));
    plugin.push_top(package::with_procedure(
        template,
        "ForceGreet",
        inputs,
        true,
    ));
    let package = package::build_record(PACK, "GreetPlayer", 0, Schedule::default());
    plugin.push_top(package::finish(package::close_header(
        package,
        Some(TEMPLATE),
    )));

    let mut quest_data = vec![0; 12];
    quest_data[2] = 50;
    plugin.push_top(
        Record::new(T(b"QUST"), QUEST)
            .sub(T(b"DNAM"), quest_data)
            .sub(T(b"NEXT"), [])
            .sub(T(b"INDX"), [0, 0, 0, 0]),
    );
    let mut topic_data = [0; 4];
    topic_data[2..4].copy_from_slice(&(if force_greet_topic { 1u16 } else { 2u16 }).to_le_bytes());
    let topic = Record::new(T(b"DIAL"), TOPIC)
        .zsub(T(b"FULL"), "A greeting")
        .sub(T(b"QNAM"), QUEST.to_le_bytes())
        .sub(T(b"BNAM"), (TOPIC + 0x1000).to_le_bytes())
        .sub(T(b"DATA"), topic_data)
        .sub(T(b"PNAM"), 50.0f32.to_le_bytes());
    let branch = Record::new(T(b"DLBR"), TOPIC + 0x1000)
        .sub(T(b"QNAM"), QUEST.to_le_bytes())
        .sub(T(b"TNAM"), 0u32.to_le_bytes())
        .sub(T(b"DNAM"), 1u32.to_le_bytes())
        .sub(T(b"SNAM"), TOPIC.to_le_bytes());
    plugin.push_top(branch);
    let mut response = vec![0; 24];
    response[12] = 1;
    let info = Record::new(T(b"INFO"), INFO)
        .sub(T(b"ENAM"), [0, 0, 0, 0])
        .sub(T(b"TRDT"), response)
        .zsub(T(b"NAM1"), "Welcome, traveler.");
    let mut topic_group = Group::new(GroupKind::Top(T(b"DIAL")));
    topic_group
        .entries
        .push(Entry::Record(plugin.push_record(topic)));
    let mut children = Group::new(GroupKind::TopicChildren(TOPIC));
    children
        .entries
        .push(Entry::Record(plugin.push_record(info)));
    topic_group.entries.push(Entry::Group(children));
    plugin.groups.push(topic_group);

    let mut order = LoadOrder::new();
    order.add("ForceGreet.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.player();
    game.world_mut().drive_actor(id(ACTOR));
    game.start_quest(id(QUEST));
    game
}

#[test]
fn dlg_14_active_force_greet_package_opens_topic_and_completes_after_close() {
    let mut game = fixture(true);
    game.tick(0.016);

    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        assert_eq!(actor.ai.package, Some(id(PACK)));
        assert_eq!(actor.ai.procedure, Procedure::ForceGreet);
    }
    {
        let world = game.world();
        let session = world.dialogue_session().unwrap();
        assert_eq!(session.speaker, id(ACTOR));
        assert_eq!(session.topic, Some(id(TOPIC)));
        assert_eq!(session.current_info, Some(id(INFO)));
    }
    assert!(game.advance_dialogue().unwrap().is_none());

    let saved = game.save_state().unwrap();
    game.load_state(&saved).unwrap();
    {
        let mut world = game.world_mut();
        assert_eq!(
            world.actor(id(ACTOR)).unwrap().ai.procedure,
            Procedure::ForceGreet
        );
    }

    game.close_dialogue();

    let mut world = game.world_mut();
    let actor = world.actor(id(ACTOR)).unwrap();
    assert!(actor.ai.tree.as_ref().unwrap().finished);
    assert!(actor.ai.completed.contains_key(&id(PACK)));
}

#[test]
fn dlg_14_force_greet_package_approaches_player_before_opening_topic() {
    let mut game = fixture(true);
    game.world_mut().reference(id(ACTOR)).unwrap().position = [600.0, 0.0, 0.0];

    for _ in 0..100 {
        game.tick(0.1);
        if game.world().dialogue_session().is_some() {
            break;
        }
    }

    let world = game.world();
    let session = world.dialogue_session().unwrap();
    assert_eq!(session.speaker, id(ACTOR));
    assert_eq!(session.topic, Some(id(TOPIC)));
}

#[test]
fn dlg_14_force_greet_package_rejects_an_ineligible_topic() {
    let mut game = fixture(false);
    game.tick(0.016);

    assert!(game.world().dialogue_session().is_none());
    let mut world = game.world_mut();
    let actor = world.actor(id(ACTOR)).unwrap();
    assert_eq!(actor.ai.procedure, Procedure::ForceGreet);
    assert!(!actor.ai.completed.contains_key(&id(PACK)));
}

#[test]
fn dlg_14_force_greet_missing_bound_inputs_remain_unsupported() {
    let mut game = fixture_with_inputs(false, &[0, 1]);
    game.tick(0.016);

    assert!(game.world().dialogue_session().is_none());
    assert_eq!(
        game.world_mut().actor(id(ACTOR)).unwrap().ai.procedure,
        Procedure::Unsupported("ForceGreet inputs".into())
    );
}
