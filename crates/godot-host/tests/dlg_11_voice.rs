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

fn game(sound_override: u32) -> Game {
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
            .zsub(Tag(*b"EDID"), "VoiceQuest")
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
        .zsub(Tag(*b"EDID"), "VoiceTopic")
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
fn dlg_11_selected_response_resolves_voice_file_and_speaker_position() {
    let mut game = game(0);
    let line = choose(&mut game, id(TOPIC)).unwrap().unwrap();
    let voice = voice_asset(&mut game, &line).unwrap();
    assert_eq!(
        voice.path,
        "Sound/Voice/Dialogue.esm/VoiceHuman/VoiceQuest_VoiceTopic_00000A00_3.fuz"
    );
    for (actual, expected) in voice.position.into_iter().zip([1.42875, 4.28625, -2.8575]) {
        assert!((actual - expected).abs() < 0.00001);
    }
}

#[test]
fn dlg_11_response_sound_override_does_not_resolve_a_voice_file() {
    let mut game = game(0x1234);
    let line = choose(&mut game, id(TOPIC)).unwrap().unwrap();
    assert!(voice_asset(&mut game, &line).is_none());
}

#[derive(Default)]
struct FakeVoiceOutput {
    played: Vec<(String, [f32; 3])>,
    stopped: Vec<i32>,
}

impl VoiceOutput for FakeVoiceOutput {
    fn play_voice(&mut self, path: &str, position: [f32; 3]) -> i32 {
        self.played.push((path.to_owned(), position));
        self.played.len() as i32 - 1
    }

    fn stop_voice(&mut self, slot: i32) -> bool {
        self.stopped.push(slot);
        true
    }
}

#[test]
fn dlg_11_starting_and_interrupting_response_plays_at_speaker_and_stops_prior_voice() {
    let mut game = game(0);
    let line = choose(&mut game, id(TOPIC)).unwrap().unwrap();
    let first = voice_asset(&mut game, &line).unwrap();
    let second = VoiceAsset {
        path: "Sound/Voice/Dialogue.esm/VoiceHuman/Next.fuz".to_owned(),
        position: [7.0, 8.0, 9.0],
    };
    let mut output = FakeVoiceOutput::default();
    let mut active = None;

    replace_voice(&mut output, &mut active, Some(&first));
    assert_eq!(active, Some(0));
    assert_eq!(output.played, [(first.path.clone(), first.position)]);
    replace_voice(&mut output, &mut active, Some(&second));
    assert_eq!(output.stopped, [0]);
    assert_eq!(active, Some(1));
    assert_eq!(output.played[1], (second.path.clone(), second.position));
    replace_voice(&mut output, &mut active, None);
    assert_eq!(output.stopped, [0, 1]);
    assert_eq!(active, None);
}
