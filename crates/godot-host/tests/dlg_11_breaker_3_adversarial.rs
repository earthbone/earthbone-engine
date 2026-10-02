// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_audio::fuz;
use soe_bsa::{write_bsa, BsaFile, BsaVersion};
use soe_esm::{
    Entry as PluginEntry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record,
    Tag,
};
use soe_game::Game;
use soe_godot_host::dialogue_menu::{choose, voice_asset};
use std::path::{Path, PathBuf};
use std::process::Command;

const VOICE: &str = "Sound/Voice/Dialogue.esm/VoiceHuman/First_00000A00_3.fuz";

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn dialogue_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"NPC_"), 7)
            .zsub(Tag(*b"EDID"), "Player")
            .sub(Tag(*b"ACBS"), vec![0; 24]),
    );
    plugin.push_top(
        Record::new(Tag(*b"NPC_"), 0x500)
            .zsub(Tag(*b"EDID"), "Speaker")
            .sub(Tag(*b"ACBS"), vec![0; 24])
            .sub(Tag(*b"VTCK"), 0x501_u32.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"VTYP"), 0x501).zsub(Tag(*b"EDID"), "VoiceHuman"));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), 0x100)
            .sub(Tag(*b"NAME"), 0x500_u32.to_le_bytes())
            .sub(
                Tag(*b"DATA"),
                [100.0_f32, 200.0, 300.0, 0.0, 0.0, 0.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
    );
    plugin.push_top(
        Record::new(Tag(*b"QUST"), 0x800)
            .zsub(Tag(*b"EDID"), "Quest")
            .sub(Tag(*b"DNAM"), vec![0; 12])
            .sub(Tag(*b"NEXT"), [])
            .sub(Tag(*b"INDX"), [0, 0, 0, 0]),
    );
    plugin.push_top(
        Record::new(Tag(*b"DLBR"), 0x1900)
            .sub(Tag(*b"QNAM"), 0x800_u32.to_le_bytes())
            .sub(Tag(*b"TNAM"), 0u32.to_le_bytes())
            .sub(Tag(*b"DNAM"), 1u32.to_le_bytes())
            .sub(Tag(*b"SNAM"), 0x900_u32.to_le_bytes()),
    );
    let topic = Record::new(Tag(*b"DIAL"), 0x900)
        .zsub(Tag(*b"EDID"), "Topic")
        .sub(Tag(*b"QNAM"), 0x800_u32.to_le_bytes())
        .sub(Tag(*b"BNAM"), 0x1900_u32.to_le_bytes())
        .sub(Tag(*b"DATA"), [0, 0, 0, 0])
        .sub(Tag(*b"SNAM"), *b"CUST")
        .sub(Tag(*b"PNAM"), 50.0f32.to_le_bytes());
    let mut trdt = [0; 24];
    trdt[12] = 3;
    let response = Record::new(Tag(*b"INFO"), 0xA00)
        .sub(Tag(*b"ENAM"), [0; 4])
        .sub(Tag(*b"TRDT"), trdt)
        .zsub(Tag(*b"NAM1"), "Speak now");
    let mut group = Group::new(GroupKind::Top(Tag(*b"DIAL")));
    group
        .entries
        .push(PluginEntry::Record(plugin.push_record(topic)));
    let mut children = Group::new(GroupKind::TopicChildren(0x900));
    children
        .entries
        .push(PluginEntry::Record(plugin.push_record(response)));
    group.entries.push(PluginEntry::Group(children));
    plugin.groups.push(group);
    let mut order = LoadOrder::new();
    order.add("Dialogue.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.start_quest(id(0x800));
    let player = game.player();
    game.begin_dialogue(id(0x100), player).unwrap();
    game
}

fn wave() -> Vec<u8> {
    let sample_rate = 22_050_u32;
    let frames = sample_rate as usize;
    let data_bytes = (frames * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_bytes as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    for frame in 0..frames {
        let sample = ((frame as f32 * 440.0 * std::f32::consts::TAU / sample_rate as f32).sin()
            * 12_000.0) as i16;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn fixture_data(data: &Path) {
    std::fs::create_dir_all(data).unwrap();
    std::fs::write(
        data.parent().unwrap().join("Skyrim_Default.ini"),
        "[General]\nsLanguage=English\n[Archive]\nsResourceArchiveList=Skyrim - Voices_en0.bsa\n",
    )
    .unwrap();
    for master in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            data.join(master),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
    let audio = wave();
    let voice = fuz::write(1, &[], &audio);
    let archive = write_bsa(
        &[BsaFile {
            path: "sound/voice/dialogue.esm/voicehuman/first_00000a00_3.fuz",
            bytes: &voice,
        }],
        BsaVersion::SkyrimSe,
    )
    .unwrap();
    std::fs::write(data.join("Skyrim - Voices_en0.bsa"), archive).unwrap();
}

fn godot_binary() -> Option<PathBuf> {
    std::env::var_os("SOE_GODOT_BIN")
        .map(PathBuf::from)
        .or_else(|| {
            Command::new("godot")
                .arg("--version")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|_| PathBuf::from("godot"))
        })
}

#[test]
fn dlg_11_missing_speaker_reference_does_not_resolve_voice_asset() {
    let mut game = dialogue_game();
    let mut line = choose(&mut game, id(0x900)).unwrap().unwrap();
    line.speaker = id(0xDEAD);
    assert!(voice_asset(&mut game, &line).is_none());
}

#[test]
fn dlg_11_voice_child_global_position_remains_at_speaker_under_moved_parent() {
    let Some(godot) = godot_binary() else {
        eprintln!("skipping: Godot is unavailable");
        return;
    };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let extension = workspace.join("target/release/libsoe_godot_host.so");
    if !extension.exists() {
        eprintln!("skipping: release GDExtension is unavailable");
        return;
    }

    let fixture = tempfile::tempdir().unwrap();
    let data = fixture.path().join("Data");
    fixture_data(&data);
    let script = fixture.path().join("speaker_position.gd");
    std::fs::write(
        &script,
        r#"extends SceneTree

func _initialize() -> void:
	call_deferred("_run")

func _run() -> void:
	var status := GDExtensionManager.load_extension("res://soe.gdextension")
	if status != 0:
		printerr("could not load extension: ", status)
		quit(1)
		return
	var parent := Node3D.new()
	parent.position = Vector3(10.0, 0.0, 0.0)
	get_root().add_child(parent)
	var bank := ClassDB.instantiate("SoundBank") as Node3D
	parent.add_child(bank)
	await process_frame
	if not bank.call("open_data", OS.get_environment("DLG_11_TEST_DATA")):
		printerr("fixture did not open: ", bank.call("last_error"))
		quit(1)
		return
	var expected := Vector3(1.25, 0.0, -1.0)
	var slot: int = bank.call("play_voice", OS.get_environment("DLG_11_VOICE"), expected)
	if slot < 0:
		printerr("voice did not start: ", bank.call("last_error"))
		quit(1)
		return
	var player := bank.get_child(slot) as AudioStreamPlayer3D
	if not player.global_position.is_equal_approx(expected):
		printerr("voice moved from speaker: expected ", expected, " got ", player.global_position)
		quit(1)
		return
	bank.call("stop_voice", slot)
	quit(0)
"#,
    )
    .unwrap();

    let output = Command::new(godot)
        .args(["--path"])
        .arg(&project)
        .args(["--script"])
        .arg(&script)
        .env("DLG_11_TEST_DATA", &data)
        .env("DLG_11_VOICE", VOICE)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "nested SoundBank speaker position test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
