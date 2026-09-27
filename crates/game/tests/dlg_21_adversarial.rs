// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::vmad::{InfoFragment, InfoFragments, ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::IAdd;
fn id(n: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object: n }
}
fn quest(n: u32, priority: u8) -> Record {
    let mut d = vec![0; 12];
    d[2] = priority;
    Record::new(Tag(*b"QUST"), n)
        .sub(Tag(*b"DNAM"), d)
        .sub(Tag(*b"NEXT"), [])
        .sub(Tag(*b"INDX"), [0; 4])
}
fn add_topic(p: &mut Plugin, topic: u32, q: u32, force: bool, info: u32) {
    let branch = topic + 0x1000;
    p.push_top(
        Record::new(Tag(*b"DLBR"), branch)
            .sub(Tag(*b"QNAM"), q.to_le_bytes())
            .sub(Tag(*b"TNAM"), 0u32.to_le_bytes())
            .sub(Tag(*b"DNAM"), 1u32.to_le_bytes())
            .sub(Tag(*b"SNAM"), topic.to_le_bytes()),
    );
    let t = Record::new(Tag(*b"DIAL"), topic)
        .zsub(Tag(*b"FULL"), "Line")
        .sub(Tag(*b"QNAM"), q.to_le_bytes())
        .sub(Tag(*b"BNAM"), branch.to_le_bytes())
        .sub(Tag(*b"DATA"), [0, 0, u8::from(force), 0])
        .sub(Tag(*b"SNAM"), if force { *b"PFGT" } else { *b"CUST" })
        .sub(
            Tag(*b"PNAM"),
            (if force { 90f32 } else { 50f32 }).to_le_bytes(),
        );
    let mut root = Group::new(GroupKind::Top(Tag(*b"DIAL")));
    root.entries.push(Entry::Record(p.push_record(t)));
    let mut c = Group::new(GroupKind::TopicChildren(topic));
    let mut r = vec![0; 24];
    r[12] = 1;
    let fragment = |function: &str| InfoFragment {
        marker: 1,
        script: "TopicFragments".into(),
        function: function.into(),
    };
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("TopicFragments")],
        info_fragments: Some(InfoFragments {
            version: 2,
            file: "TopicFragments".into(),
            begin: Some(fragment("Fragment_0")),
            end: Some(fragment("Fragment_1")),
        }),
        ..Vmad::default()
    };
    c.entries.push(Entry::Record(
        p.push_record(
            Record::new(Tag(*b"INFO"), info)
                .sub(Tag(*b"ENAM"), [0; 4])
                .sub(Tag(*b"TRDT"), r)
                .zsub(Tag(*b"NAM1"), "Line")
                .sub(Tag(*b"VMAD"), vmad.write(Tag(*b"INFO"))),
        ),
    ));
    root.entries.push(Entry::Group(c));
    p.groups.push(root);
}

fn fragment_script() -> Vec<u8> {
    let fragment = |name: &str, counter: &str| {
        FunctionBuilder::new(name, "None")
            .param("akSpeakerRef", "ObjectReference")
            .ins(IAdd, vec![ident(counter), ident(counter), i(1)])
            .build()
    };
    ScriptBuilder::new("TopicFragments", "TopicInfo")
        .variable("begins", "Int", i(0))
        .variable("ends", "Int", i(0))
        .function(fragment("Fragment_0", "begins"))
        .function(fragment("Fragment_1", "ends"))
        .bytes()
}

fn fragment_count(game: &mut Game, form: u32, name: &str) -> i32 {
    let object = game.object_of_form(id(form));
    game.vm
        .get_var(object, "TopicFragments", name)
        .unwrap()
        .as_int()
}

#[test]
fn dlg_21_higher_quest_priority_interrupts_once_and_finishes_prior_fragments() {
    let mut p = Plugin::new(FileHeader::default());
    for (f, n) in [(7, "Player"), (0x500, "First"), (0x501, "Second")] {
        p.push_top(
            Record::new(Tag(*b"NPC_"), f)
                .zsub(Tag(*b"EDID"), n)
                .sub(Tag(*b"ACBS"), vec![0; 24]),
        );
    }
    for (f, base, x) in [(0x100u32, 0x500u32, 10f32), (0x101u32, 0x501u32, 20f32)] {
        let mut d = vec![0; 24];
        d[..4].copy_from_slice(&x.to_le_bytes());
        p.push_top(
            Record::new(Tag(*b"ACHR"), f)
                .sub(Tag(*b"NAME"), base.to_le_bytes())
                .sub(Tag(*b"DATA"), d),
        );
    }
    p.push_top(quest(0x800, 10));
    p.push_top(quest(0x801, 11));
    add_topic(&mut p, 0x900, 0x800, false, 0xA00);
    add_topic(&mut p, 0x901, 0x801, true, 0xA01);
    let mut lo = LoadOrder::new();
    lo.add("Dialogue.esm", p).unwrap();
    let mut g = Game::new(lo);
    g.load_script(&fragment_script()).unwrap();
    g.bind();
    g.player();
    g.world_mut().drive_actor(id(0x100));
    g.world_mut().drive_actor(id(0x101));
    g.start_quest(id(0x800));
    g.start_quest(id(0x801));
    let player = g.player();
    g.begin_dialogue(id(0x101), player).unwrap();
    assert!(g
        .dialogue_choices()
        .unwrap()
        .available
        .iter()
        .any(|c| c.topic == id(0x901)));
    g.close_dialogue();
    g.begin_dialogue(id(0x100), player).unwrap();
    g.choose_dialogue(id(0x900)).unwrap();
    assert_eq!(fragment_count(&mut g, 0xA00, "begins"), 1);
    assert_eq!(fragment_count(&mut g, 0xA00, "ends"), 0);
    g.world_mut().actor(id(0x100)).unwrap().in_combat = true;
    g.world_mut().reference(id(0x101)).unwrap().position = [10000.0, 0.0, 0.0];
    g.tick(0.016);
    g.world_mut().reference(id(0x101)).unwrap().position = [20.0, 0.0, 0.0];
    g.tick(0.016);
    let w = g.world();
    let s = w.dialogue_session().unwrap();
    assert_eq!(s.speaker, id(0x101));
    assert_eq!(s.current_info, Some(id(0xA01)));
    drop(w);
    assert_eq!(fragment_count(&mut g, 0xA00, "ends"), 1);
    assert_eq!(fragment_count(&mut g, 0xA01, "begins"), 1);
    g.tick(0.016);
    assert_eq!(
        g.world().dialogue_session().unwrap().current_info,
        Some(id(0xA01))
    );
    assert_eq!(fragment_count(&mut g, 0xA00, "ends"), 1);
    assert_eq!(fragment_count(&mut g, 0xA01, "begins"), 1);
}
