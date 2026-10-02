// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! DLG-18: Scene dialogue uses its ordered action slot, alias speaker and DIAL.

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::dialogue::Error;
use soe_game::Game;
use soe_papyrus::ObjectId;

#[allow(clippy::trivially_copy_pass_by_ref)]
fn tag(bytes: &[u8; 4]) -> Tag {
    Tag::new(bytes)
}

const fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

const SPEAKER: u32 = 0x100;
const BASE: u32 = 0x500;
const QUEST: u32 = 0x800;
const TOPIC: u32 = 0x900;
const INFO: u32 = 0xA00;
const SCENE: u32 = 0xB00;

fn fixture(alias_filled: bool, responses: &[&str]) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(tag(b"NPC_"), 7)
            .zsub(tag(b"EDID"), "Player")
            .sub(tag(b"ACBS"), vec![0; 24]),
    );
    plugin.push_top(
        Record::new(tag(b"NPC_"), BASE)
            .zsub(tag(b"EDID"), "Speaker")
            .sub(tag(b"ACBS"), vec![0; 24]),
    );
    plugin.push_top(
        Record::new(tag(b"ACHR"), SPEAKER)
            .sub(tag(b"NAME"), BASE.to_le_bytes())
            .sub(tag(b"DATA"), vec![0; 24]),
    );

    let mut quest = Record::new(tag(b"QUST"), QUEST)
        .sub(tag(b"DNAM"), vec![0; 12])
        .sub(tag(b"NEXT"), [])
        .sub(tag(b"ALST"), 0u32.to_le_bytes())
        .zsub(tag(b"ALID"), "Speaker")
        .sub(tag(b"FNAM"), 2u32.to_le_bytes());
    if alias_filled {
        quest = quest.sub(tag(b"ALFR"), SPEAKER.to_le_bytes());
    }
    plugin.push_top(quest.sub(tag(b"ALED"), []));

    let topic = Record::new(tag(b"DIAL"), TOPIC)
        .zsub(tag(b"FULL"), "Scene line")
        .sub(tag(b"QNAM"), QUEST.to_le_bytes())
        .sub(tag(b"DATA"), [0, 0, 0, 0])
        .sub(tag(b"SNAM"), *b"SCEN");
    let mut info = Record::new(tag(b"INFO"), INFO).sub(tag(b"ENAM"), [0; 4]);
    for (index, text) in responses.iter().enumerate() {
        let mut response = vec![0; 24];
        response[12] = index as u8 + 1;
        info = info.sub(tag(b"TRDT"), response).zsub(tag(b"NAM1"), text);
    }
    let mut dial = Group::new(GroupKind::Top(tag(b"DIAL")));
    dial.entries.push(Entry::Record(plugin.push_record(topic)));
    let mut children = Group::new(GroupKind::TopicChildren(TOPIC));
    children
        .entries
        .push(Entry::Record(plugin.push_record(info)));
    dial.entries.push(Entry::Group(children));
    plugin.groups.push(dial);

    let scene = Record::new(tag(b"SCEN"), SCENE)
        .sub(tag(b"HNAM"), [])
        .zsub(tag(b"NAM0"), "First")
        .sub(tag(b"NEXT"), [])
        .sub(tag(b"HNAM"), [])
        .zsub(tag(b"NAM0"), "Second")
        .sub(tag(b"HNAM"), [])
        .sub(tag(b"ANAM"), 0u16.to_le_bytes())
        .zsub(tag(b"NAM0"), "Assigned line")
        .sub(tag(b"ALID"), 0i32.to_le_bytes())
        .sub(tag(b"INAM"), 10u32.to_le_bytes())
        .sub(tag(b"SNAM"), 0u32.to_le_bytes())
        .sub(tag(b"ENAM"), 1u32.to_le_bytes())
        .sub(tag(b"DATA"), TOPIC.to_le_bytes())
        .sub(tag(b"ANAM"), [])
        .sub(tag(b"PNAM"), QUEST.to_le_bytes());
    plugin.push_top(scene);

    let mut order = LoadOrder::new();
    order.add("SceneDialogue.esp", plugin).unwrap();
    let mut game = Game::new(order);
    game.start_quest(id(QUEST));
    game
}

fn start_scene(game: &mut Game) {
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().start_scene(id(SCENE), &mut allocate));
}

#[test]
fn dlg_18_scene_dialogue_uses_alias_speaker_and_completes_after_all_responses() {
    let mut game = fixture(true, &["First scene line", "Second scene line"]);
    start_scene(&mut game);

    let first = game
        .start_scene_dialogue_action(id(SCENE), 0)
        .unwrap()
        .unwrap();
    assert_eq!(first.speaker, id(SPEAKER));
    assert_eq!(first.text.as_deref(), Some("First scene line"));

    let second = game
        .advance_scene_dialogue_action(id(SCENE), 0)
        .unwrap()
        .unwrap();
    assert_eq!(second.text.as_deref(), Some("Second scene line"));
    assert!(game
        .advance_scene_dialogue_action(id(SCENE), 0)
        .unwrap()
        .is_none());
    assert!(game.dialogue_line().unwrap().is_none());

    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game
        .world_mut()
        .advance_scene_phase(id(SCENE), &mut allocate));
    assert_eq!(game.world().scene_phase(id(SCENE)), Some(1));
    assert!(game
        .world_mut()
        .advance_scene_phase(id(SCENE), &mut allocate));
    assert!(!game.world().scenes.contains(&id(SCENE)));
}

#[test]
fn dlg_18_scene_dialogue_rejects_unfilled_alias_without_opening_conversation() {
    let mut game = fixture(false, &["Hidden"]);
    start_scene(&mut game);

    assert!(matches!(
        game.start_scene_dialogue_action(id(SCENE), 0),
        Err(Error::State("scene dialogue actor alias is unfilled"))
    ));
    assert!(game.dialogue_line().unwrap().is_none());
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game
        .world_mut()
        .advance_scene_phase(id(SCENE), &mut allocate));
    assert!(!game
        .world_mut()
        .advance_scene_phase(id(SCENE), &mut allocate));
}
