// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    Entry as PluginEntry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record,
    Tag,
};
use soe_game::Game;
use soe_godot_host::dialogue_menu::{choose, entries};

const SPEAKER: u32 = 0x100;
const BASE: u32 = 0x500;
const QUEST: u32 = 0x800;
const TOPIC_A: u32 = 0x900;
const TOPIC_B: u32 = 0x901;
const INFO_A: u32 = 0xA00;
const INFO_B: u32 = 0xA01;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for (form, name) in [(7, "Player"), (BASE, "Speaker")] {
        plugin.push_top(
            Record::new(Tag(*b"NPC_"), form)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"ACBS"), vec![0; 24]),
        );
    }
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), SPEAKER)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]),
    );
    plugin.push_top(
        Record::new(Tag(*b"QUST"), QUEST)
            .sub(Tag(*b"DNAM"), vec![0; 12])
            .sub(Tag(*b"NEXT"), [])
            .sub(Tag(*b"INDX"), [0, 0, 0, 0]),
    );

    let mut group = Group::new(GroupKind::Top(Tag(*b"DIAL")));
    for (topic_id, info_id, label, priority, response) in [
        (TOPIC_A, INFO_A, None, 70.0_f32, "First response"),
        (
            TOPIC_B,
            INFO_B,
            Some("Second topic"),
            50.0_f32,
            "Second response",
        ),
    ] {
        let branch = topic_id + 0x1000;
        plugin.push_top(
            Record::new(Tag(*b"DLBR"), branch)
                .sub(Tag(*b"QNAM"), QUEST.to_le_bytes())
                .sub(Tag(*b"TNAM"), 0u32.to_le_bytes())
                .sub(Tag(*b"DNAM"), 1u32.to_le_bytes())
                .sub(Tag(*b"SNAM"), topic_id.to_le_bytes()),
        );
        let mut topic = Record::new(Tag(*b"DIAL"), topic_id)
            .sub(Tag(*b"QNAM"), QUEST.to_le_bytes())
            .sub(Tag(*b"BNAM"), branch.to_le_bytes())
            .sub(Tag(*b"DATA"), [0, 0, 0, 0])
            .sub(Tag(*b"SNAM"), *b"CUST")
            .sub(Tag(*b"PNAM"), priority.to_le_bytes());
        if let Some(label) = label {
            topic = topic.zsub(Tag(*b"FULL"), label);
        }
        let response_record = Record::new(Tag(*b"INFO"), info_id)
            .sub(Tag(*b"ENAM"), [0; 4])
            .sub(Tag(*b"TRDT"), [0; 24])
            .zsub(Tag(*b"NAM1"), response);
        group
            .entries
            .push(PluginEntry::Record(plugin.push_record(topic)));
        let mut children = Group::new(GroupKind::TopicChildren(topic_id));
        children
            .entries
            .push(PluginEntry::Record(plugin.push_record(response_record)));
        group.entries.push(PluginEntry::Group(children));
    }
    plugin.groups.push(group);

    let mut order = LoadOrder::new();
    order.add("Dialogue.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.start_quest(id(QUEST));
    let player = game.player();
    game.begin_dialogue(id(SPEAKER), player).unwrap();
    game
}

#[test]
fn dlg_10_choices_are_stably_sorted_and_use_topic_name_when_prompt_is_missing() {
    let mut game = game();
    let first = entries(&mut game).unwrap();
    let second = entries(&mut game).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first.iter().map(|entry| entry.topic).collect::<Vec<_>>(),
        vec![id(TOPIC_A), id(TOPIC_B)]
    );
    assert_eq!(first[0].label, "0:000900");
    assert_eq!(first[1].label, "Second topic");
}

#[test]
fn dlg_10_selection_rejects_unoffered_topics_then_executes_the_selected_response() {
    let mut game = game();
    assert!(choose(&mut game, id(TOPIC_A + 40)).is_err());
    assert!(game.dialogue_line().unwrap().is_none());
    let selected = choose(&mut game, id(TOPIC_B)).unwrap().unwrap();
    assert_eq!(selected.topic, id(TOPIC_B));
    assert_eq!(selected.text.as_deref(), Some("Second response"));
    assert!(entries(&mut game).is_err());
}
