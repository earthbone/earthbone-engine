#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    Entry as PluginEntry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record,
    Tag,
};
use soe_game::Game;
use soe_godot_host::dialogue_menu::{choose, voice_asset};

const SPEAKER: u32 = 0x100;
const ACTIVATOR_SPEAKER: u32 = 0x101;
const BASE: u32 = 0x500;
const VOICE: u32 = 0x501;
const QUEST: u32 = 0x800;
const TOPIC: u32 = 0x900;
const INFO: u32 = 0xA00;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game(quest_name: Option<&str>, topic_name: Option<&str>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"NPC_"), 7)
            .zsub(Tag(*b"EDID"), "Player")
            .sub(Tag(*b"ACBS"), vec![0; 24]),
    );
    plugin.push_top(
        Record::new(Tag(*b"NPC_"), BASE)
            .zsub(Tag(*b"EDID"), "DialogueSpeaker")
            .sub(Tag(*b"ACBS"), vec![0; 24])
            .sub(Tag(*b"VTCK"), VOICE.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"VTYP"), VOICE).zsub(Tag(*b"EDID"), "VoiceHuman"));
    plugin.push_top(
        Record::new(Tag(*b"TACT"), BASE + 2)
            .zsub(Tag(*b"EDID"), "TalkingActivator")
            .sub(Tag(*b"VNAM"), VOICE.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), SPEAKER)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes())
            .sub(
                Tag(*b"DATA"),
                [100.0_f32, 200.0, 300.0, 0.0, 0.0, 0.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
    );
    plugin.push_top(
        Record::new(Tag(*b"REFR"), ACTIVATOR_SPEAKER)
            .sub(Tag(*b"NAME"), (BASE + 2).to_le_bytes())
            .sub(
                Tag(*b"DATA"),
                [400.0_f32, 500.0, 600.0, 0.0, 0.0, 0.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
    );
    let mut quest = Record::new(Tag(*b"QUST"), QUEST)
        .sub(Tag(*b"DNAM"), vec![0; 12])
        .sub(Tag(*b"NEXT"), [])
        .sub(Tag(*b"INDX"), [0, 0, 0, 0]);
    if let Some(name) = quest_name {
        quest = quest.zsub(Tag(*b"EDID"), name);
    }
    plugin.push_top(quest);
    plugin.push_top(
        Record::new(Tag(*b"DLBR"), TOPIC + 0x1000)
            .sub(Tag(*b"QNAM"), QUEST.to_le_bytes())
            .sub(Tag(*b"TNAM"), 0u32.to_le_bytes())
            .sub(Tag(*b"DNAM"), 1u32.to_le_bytes())
            .sub(Tag(*b"SNAM"), TOPIC.to_le_bytes()),
    );
    let mut topic = Record::new(Tag(*b"DIAL"), TOPIC)
        .sub(Tag(*b"QNAM"), QUEST.to_le_bytes())
        .sub(Tag(*b"BNAM"), (TOPIC + 0x1000).to_le_bytes())
        .sub(Tag(*b"DATA"), [0, 0, 0, 0])
        .sub(Tag(*b"SNAM"), *b"CUST")
        .sub(Tag(*b"PNAM"), 50.0f32.to_le_bytes());
    if let Some(name) = topic_name {
        topic = topic.zsub(Tag(*b"EDID"), name);
    }
    let mut trdt = [0; 24];
    trdt[12] = 3;
    let response = Record::new(Tag(*b"INFO"), INFO)
        .sub(Tag(*b"ENAM"), [0; 4])
        .sub(Tag(*b"TRDT"), trdt)
        .zsub(Tag(*b"NAM1"), "Speak now");
    let mut group = Group::new(GroupKind::Top(Tag(*b"DIAL")));
    group
        .entries
        .push(PluginEntry::Record(plugin.push_record(topic)));
    let mut children = Group::new(GroupKind::TopicChildren(TOPIC));
    children
        .entries
        .push(PluginEntry::Record(plugin.push_record(response)));
    group.entries.push(PluginEntry::Group(children));
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
fn dlg_11_missing_editor_ids_keep_mutagen_empty_filename_components() {
    let mut game = game(None, None);
    let line = choose(&mut game, id(TOPIC)).unwrap().unwrap();
    let voice = voice_asset(&mut game, &line).unwrap();
    assert_eq!(
        voice.path,
        "Sound/Voice/Dialogue.esm/VoiceHuman/__00000A00_3.fuz"
    );
}

#[test]
fn dlg_11_talking_activator_uses_its_vnam_voice_type_and_position() {
    let mut game = game(Some("Quest"), Some("Topic"));
    let mut line = choose(&mut game, id(TOPIC)).unwrap().unwrap();
    line.speaker = id(ACTIVATOR_SPEAKER);
    let voice = voice_asset(&mut game, &line).unwrap();
    assert_eq!(
        voice.path,
        "Sound/Voice/Dialogue.esm/VoiceHuman/Quest_Topic_00000A00_3.fuz"
    );
    for (actual, expected) in voice.position.into_iter().zip([5.715, 8.5725, -7.14375]) {
        assert!((actual - expected).abs() < 0.0001);
    }
}
