// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const SPEAKER: u32 = 0x100;
const BASE: u32 = 0x500;
const QUEST: u32 = 0x800;
const TOPIC: u32 = 0x900;
const INFO: u32 = 0xA00;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn greeting_condition(expected: f32) -> Vec<u8> {
    let mut data = vec![0; 32];
    data[4..8].copy_from_slice(&expected.to_le_bytes());
    data[8..10].copy_from_slice(&123u16.to_le_bytes());
    data
}

fn fixture(signature_subtype: bool, expected_greeting: f32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"NPC_"), 7)
            .zsub(T(b"EDID"), "Player")
            .sub(T(b"ACBS"), vec![0; 24]),
    );
    plugin.push_top(
        Record::new(T(b"NPC_"), BASE)
            .zsub(T(b"EDID"), "Greeter")
            .sub(T(b"ACBS"), vec![0; 24]),
    );
    let mut placement = vec![0; 24];
    placement[..4].copy_from_slice(&20.0f32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"ACHR"), SPEAKER)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), placement),
    );
    let mut quest_data = vec![0; 12];
    quest_data[2] = 50;
    plugin.push_top(
        Record::new(T(b"QUST"), QUEST)
            .sub(T(b"DNAM"), quest_data)
            .sub(T(b"NEXT"), [])
            .sub(T(b"INDX"), [0, 0, 0, 0]),
    );

    let mut topic_data = [0; 4];
    topic_data[2..4].copy_from_slice(&1u16.to_le_bytes());
    let topic = Record::new(T(b"DIAL"), TOPIC)
        .zsub(T(b"FULL"), "Hello")
        .sub(T(b"QNAM"), QUEST.to_le_bytes())
        .sub(T(b"BNAM"), (TOPIC + 0x1000).to_le_bytes())
        .sub(T(b"DATA"), topic_data)
        .sub(T(b"PNAM"), 50.0f32.to_le_bytes());
    let topic = if signature_subtype {
        topic.sub(T(b"SNAM"), *b"PFGT")
    } else {
        topic
    };
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
        .sub(T(b"CTDA"), greeting_condition(expected_greeting))
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
    order.add("Greeting.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.player();
    game.world_mut().drive_actor(id(SPEAKER));
    game.start_quest(id(QUEST));
    game
}

#[test]
fn dlg_02_entering_force_greet_range_selects_one_line_once() {
    let mut game = fixture(false, 1.0);

    game.tick(0.016);

    {
        let world = game.world();
        let session = world.dialogue_session().unwrap();
        assert_eq!(session.speaker, id(SPEAKER));
        assert_eq!(session.topic, Some(id(TOPIC)));
        assert_eq!(session.current_info, Some(id(INFO)));
    }
    assert_eq!(
        game.dialogue_line().unwrap().unwrap().text.as_deref(),
        Some("Welcome, traveler.")
    );

    game.tick(0.016);

    {
        let world = game.world();
        let session = world.dialogue_session().unwrap();
        assert_eq!(session.current_info, Some(id(INFO)));
        assert_eq!(session.response_index, 0);
    }

    game.close_dialogue();
    game.tick(0.016);
    assert!(game.world().dialogue_session().is_none());
}

#[test]
fn dlg_02_leaving_and_reentering_range_raises_a_new_greeting_edge() {
    let mut game = fixture(false, 1.0);
    game.tick(0.016);
    game.close_dialogue();

    game.world_mut().reference(id(SPEAKER)).unwrap().position = [10_000.0, 0.0, 0.0];
    game.tick(0.016);
    game.world_mut().reference(id(SPEAKER)).unwrap().position = [20.0, 0.0, 0.0];
    game.tick(0.016);

    assert_eq!(
        game.world().dialogue_session().unwrap().current_info,
        Some(id(INFO))
    );
}

#[test]
fn dlg_02_signature_force_greet_subtype_selects_a_line() {
    let mut game = fixture(true, 1.0);
    game.tick(0.016);

    assert_eq!(
        game.world().dialogue_session().unwrap().current_info,
        Some(id(INFO))
    );
}

#[test]
fn dlg_02_translucent_actor_waits_until_ghost_or_fully_visible() {
    let mut game = fixture(false, 1.0);
    game.world_mut().reference(id(SPEAKER)).unwrap().alpha = 0.5;
    game.tick(0.016);
    assert!(game.world().dialogue_session().is_none());

    game.world_mut().actor(id(SPEAKER)).unwrap().ghost = true;
    game.tick(0.016);
    assert_eq!(
        game.world().dialogue_session().unwrap().current_info,
        Some(id(INFO))
    );
}

#[test]
fn dlg_02_is_greeting_player_condition_is_true_only_for_automatic_selection() {
    let mut game = fixture(false, 0.0);
    game.tick(0.016);
    assert!(game.world().dialogue_session().is_none());

    let player = game.player();
    game.begin_dialogue(id(SPEAKER), player).unwrap();
    assert_eq!(game.dialogue_choices().unwrap().available.len(), 1);
}
