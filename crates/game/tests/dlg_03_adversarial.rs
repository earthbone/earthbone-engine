// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
fn id(object: u32) -> GlobalFormId { GlobalFormId { file: 0, object } }
fn info(form: u32, flags: u16, text: &str) -> Record {
    let mut response = vec![0; 24]; response[12] = 1;
    Record::new(Tag(*b"INFO"), form).sub(Tag(*b"ENAM"), [flags as u8, (flags >> 8) as u8, 0, 0])
        .sub(Tag(*b"TRDT"), response).zsub(Tag(*b"NAM1"), text)
}
#[test]
fn dlg_03_eligible_accepted_response_takes_precedence_over_refusal() {
    const TOPIC:u32=0x400; const QUEST:u32=0x300;
    let mut p=Plugin::new(FileHeader::default());
    for (f,n) in [(7,"Player"),(0x100,"SpeakerBase")] { p.push_top(Record::new(Tag(*b"NPC_"),f).zsub(Tag(*b"EDID"),n).sub(Tag(*b"ACBS"),vec![0;24])); }
    p.push_top(Record::new(Tag(*b"ACHR"),0x200).sub(Tag(*b"NAME"),0x100u32.to_le_bytes()).sub(Tag(*b"DATA"),vec![0;24]));
    p.push_top(Record::new(Tag(*b"QUST"),QUEST).sub(Tag(*b"DNAM"),vec![0;12]).sub(Tag(*b"NEXT"),[]).sub(Tag(*b"INDX"),[0;4]).sub(Tag(*b"INDX"),5u32.to_le_bytes()).sub(Tag(*b"INDX"),10u32.to_le_bytes()).sub(Tag(*b"INDX"),20u32.to_le_bytes()).sub(Tag(*b"INDX"),5u32.to_le_bytes()).sub(Tag(*b"INDX"),10u32.to_le_bytes()).sub(Tag(*b"INDX"),20u32.to_le_bytes()));
    p.push_top(Record::new(Tag(*b"DLBR"),0x1400).sub(Tag(*b"QNAM"),QUEST.to_le_bytes()).sub(Tag(*b"TNAM"),0u32.to_le_bytes()).sub(Tag(*b"DNAM"),1u32.to_le_bytes()).sub(Tag(*b"SNAM"),TOPIC.to_le_bytes()));
    let topic=Record::new(Tag(*b"DIAL"),TOPIC).zsub(Tag(*b"FULL"),"Ask").sub(Tag(*b"QNAM"),QUEST.to_le_bytes()).sub(Tag(*b"BNAM"),0x1400u32.to_le_bytes()).sub(Tag(*b"DATA"),[0;4]).sub(Tag(*b"SNAM"),*b"CUST").sub(Tag(*b"PNAM"),50f32.to_le_bytes());
    let mut root=Group::new(GroupKind::Top(Tag(*b"DIAL"))); root.entries.push(Entry::Record(p.push_record(topic)));
    let mut children=Group::new(GroupKind::TopicChildren(TOPIC)); children.entries.push(Entry::Record(p.push_record(info(0x500,0,"Accepted")))); children.entries.push(Entry::Record(p.push_record(info(0x501,1<<4,"Refused")))); root.entries.push(Entry::Group(children)); p.groups.push(root);
    let mut lo=LoadOrder::new(); lo.add("Dialogue.esm",p).unwrap(); let mut g=Game::new(lo); g.start_quest(id(QUEST)); let player=g.player(); g.begin_dialogue(id(0x200),player).unwrap();
    let choices=g.dialogue_choices().unwrap(); assert!(!choices.available.is_empty(), "choices={:?}, rejected={:?}", choices.available, choices.rejected); let line=g.choose_dialogue(id(TOPIC)).unwrap().unwrap(); assert_eq!(line.info,id(0x500)); assert_eq!(line.text.as_deref(),Some("Accepted"));
}
