#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    Entry as PluginEntry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record,
    Tag,
};
use soe_game::Game;
use soe_godot_host::dialogue_menu::{choose, replace_voice, voice_asset, VoiceAsset, VoiceOutput};

const SPEAKER: u32 = 0x100;
const BASE: u32 = 0x500;
const VOICE: u32 = 0x501;
const QUEST: u32 = 0x800;
const TOPIC: u32 = 0x900;
const INFO: u32 = 0xA00;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game(quest_name: &str, topic_name: &str, sound_override: u32, include_voice: bool) -> Game {
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
    if include_voice {
        plugin.push_top(Record::new(Tag(*b"VTYP"), VOICE).zsub(Tag(*b"EDID"), "VoiceHuman"));
    }
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
        Record::new(Tag(*b"QUST"), QUEST)
            .zsub(Tag(*b"EDID"), quest_name)
            .sub(Tag(*b"DNAM"), vec![0; 12])
            .sub(Tag(*b"NEXT"), [])
            .sub(Tag(*b"INDX"), [0, 0, 0, 0]),
    );
    plugin.push_top(
        Record::new(Tag(*b"DLBR"), TOPIC + 0x1000)
            .sub(Tag(*b"QNAM"), QUEST.to_le_bytes())
            .sub(Tag(*b"TNAM"), 0u32.to_le_bytes())
            .sub(Tag(*b"DNAM"), 1u32.to_le_bytes())
            .sub(Tag(*b"SNAM"), TOPIC.to_le_bytes()),
    );
    let topic = Record::new(Tag(*b"DIAL"), TOPIC)
        .zsub(Tag(*b"EDID"), topic_name)
        .sub(Tag(*b"QNAM"), QUEST.to_le_bytes())
        .sub(Tag(*b"BNAM"), (TOPIC + 0x1000).to_le_bytes())
        .sub(Tag(*b"DATA"), [0, 0, 0, 0])
        .sub(Tag(*b"SNAM"), *b"CUST")
        .sub(Tag(*b"PNAM"), 50.0f32.to_le_bytes());
    let mut trdt = [0; 24];
    trdt[12] = 3;
    trdt[16..20].copy_from_slice(&sound_override.to_le_bytes());
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
fn dlg_11_long_quest_and_topic_editor_ids_follow_voice_filename_limit() {
    let mut game = game("QuestIdentifierLong", "TopicIdentifierIsLong", 0, true);
    let line = choose(&mut game, id(TOPIC)).unwrap().unwrap();
    let voice = voice_asset(&mut game, &line).unwrap();
    assert_eq!(
        voice.path,
        "Sound/Voice/Dialogue.esm/VoiceHuman/QuestIdent_TopicIdentifier_00000A00_3.fuz"
    );
}

#[test]
fn dlg_11_sound_override_has_no_generated_voice_file() {
    let mut game = game("Quest", "Topic", 0x1234, true);
    let line = choose(&mut game, id(TOPIC)).unwrap().unwrap();
    assert!(voice_asset(&mut game, &line).is_none());
}

#[test]
fn dlg_11_missing_voice_record_does_not_resolve_an_asset() {
    let mut game = game("Quest", "Topic", 0, false);
    let line = choose(&mut game, id(TOPIC)).unwrap().unwrap();
    assert!(voice_asset(&mut game, &line).is_none());
}

#[derive(Default)]
struct Output {
    played: Vec<String>,
    stopped: Vec<i32>,
}

impl VoiceOutput for Output {
    fn play_voice(&mut self, path: &str, _position: [f32; 3]) -> i32 {
        self.played.push(path.to_owned());
        self.played.len() as i32 - 1
    }

    fn stop_voice(&mut self, slot: i32) -> bool {
        self.stopped.push(slot);
        true
    }
}

#[test]
fn dlg_11_transition_to_sound_override_stops_voice_without_starting_generated_path() {
    let first = VoiceAsset {
        path: "first.fuz".to_owned(),
        position: [1.0, 2.0, 3.0],
    };
    let mut output = Output::default();
    let mut active = None;
    replace_voice(&mut output, &mut active, Some(&first));
    replace_voice(&mut output, &mut active, None);
    assert_eq!(output.stopped, [0]);
    assert_eq!(output.played, ["first.fuz"]);
    assert_eq!(active, None);
}
