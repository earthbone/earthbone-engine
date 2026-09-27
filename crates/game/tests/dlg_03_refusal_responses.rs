// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! DLG-03: an INFO refusal response is selected when the accepted INFO
//! conditions fail, without starting the accepted INFO.

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x100;
const SPEAKER: u32 = 0x200;
const QUEST: u32 = 0x300;
const TOPIC: u32 = 0x400;
const ACCEPTED: u32 = 0x500;
const REFUSAL: u32 = 0x501;
const INFO_REFUSAL: u16 = 1 << 4;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn condition(function: u16, parameter: u32, expected: f32) -> Vec<u8> {
    let mut data = vec![0; 32];
    data[4..8].copy_from_slice(&expected.to_le_bytes());
    data[8..10].copy_from_slice(&function.to_le_bytes());
    data[12..16].copy_from_slice(&parameter.to_le_bytes());
    data
}

fn info(form: u32, flags: u16, text: &str) -> Record {
    let mut response = vec![0; 24];
    response[12] = 1;
    Record::new(T(b"INFO"), form)
        .sub(T(b"ENAM"), [flags as u8, (flags >> 8) as u8, 0, 0])
        .sub(T(b"TRDT"), response)
        .zsub(T(b"NAM1"), text)
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for (form, editor_id) in [(7, "Player"), (BASE, "SpeakerBase")] {
        plugin.push_top(
            Record::new(T(b"NPC_"), form)
                .zsub(T(b"EDID"), editor_id)
                .sub(T(b"ACBS"), vec![0; 24]),
        );
    }
    plugin.push_top(
        Record::new(T(b"ACHR"), SPEAKER)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(
        Record::new(T(b"QUST"), QUEST)
            .sub(T(b"DNAM"), vec![0; 12])
            .sub(T(b"NEXT"), [])
            .sub(T(b"INDX"), [0, 0, 0, 0]),
    );
    plugin.push_top(
        Record::new(T(b"DLBR"), TOPIC + 0x1000)
            .sub(T(b"QNAM"), QUEST.to_le_bytes())
            .sub(T(b"TNAM"), 0u32.to_le_bytes())
            .sub(T(b"DNAM"), 1u32.to_le_bytes())
            .sub(T(b"SNAM"), TOPIC.to_le_bytes()),
    );
    let topic = Record::new(T(b"DIAL"), TOPIC)
        .zsub(T(b"FULL"), "Ask about the gate")
        .sub(T(b"QNAM"), QUEST.to_le_bytes())
        .sub(T(b"BNAM"), (TOPIC + 0x1000).to_le_bytes())
        .sub(T(b"DATA"), [0, 0, 0, 0])
        .sub(T(b"SNAM"), *b"CUST")
        .sub(T(b"PNAM"), 50.0f32.to_le_bytes());
    let accepted = info(ACCEPTED, 0, "You may pass.").sub(T(b"CTDA"), condition(72, BASE, 0.0));

    let mut top = Group::new(GroupKind::Top(T(b"DIAL")));
    top.entries.push(Entry::Record(plugin.push_record(topic)));
    let mut children = Group::new(GroupKind::TopicChildren(TOPIC));
    children
        .entries
        .push(Entry::Record(plugin.push_record(accepted)));
    children.entries.push(Entry::Record(plugin.push_record(info(
        REFUSAL,
        INFO_REFUSAL,
        "The gate is closed.",
    ))));
    top.entries.push(Entry::Group(children));
    plugin.groups.push(top);

    let mut order = LoadOrder::new();
    order.add("Refusal.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn dlg_03_refusal_selects_authored_response_when_accepted_condition_fails() {
    let mut game = game();
    game.start_quest(id(QUEST));
    let player = game.player();
    game.begin_dialogue(id(SPEAKER), player).unwrap();

    let choices = game.dialogue_choices().unwrap();
    assert!(
        choices
            .available
            .iter()
            .any(|choice| choice.topic == id(TOPIC)),
        "a topic with a refusal response should remain selectable"
    );

    let line = game.choose_dialogue(id(TOPIC)).unwrap().unwrap();
    assert_eq!(line.info, id(REFUSAL));
    assert_eq!(line.text.as_deref(), Some("The gate is closed."));
    assert_ne!(line.info, id(ACCEPTED));
}
