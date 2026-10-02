#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_audio::fuz;
use soe_bsa::{write_bsa, BsaFile, BsaVersion};
use soe_esm::{FileHeader, Plugin};
use std::path::{Path, PathBuf};
use std::process::Command;

const VOICE: &str = "Sound/Voice/Dialogue.esm/VoiceHuman/Quest_Topic_00000A00_3.fuz";

fn wave() -> Vec<u8> {
    let sample_rate = 22_050_u32;
    let samples = (0..sample_rate as usize)
        .map(|index| {
            ((index as f32 * 440.0 * std::f32::consts::TAU / sample_rate as f32).sin() * 12_000.0)
                as i16
        })
        .collect::<Vec<_>>();
    let data_bytes = (samples.len() * 2) as u32;
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
    for sample in samples {
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
    let voice = fuz::write(1, &[], &wave());
    let archive = write_bsa(
        &[BsaFile {
            path: "sound/voice/dialogue.esm/voicehuman/quest_topic_00000a00_3.fuz",
            bytes: &voice,
        }],
        BsaVersion::SkyrimSe,
    )
    .unwrap();
    std::fs::write(data.join("Skyrim - Voices_en0.bsa"), archive).unwrap();
}

fn godot_binary() -> PathBuf {
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
        .expect("Godot 4.4 or newer is required for the host-space check")
}

#[test]
fn dlg_11_voice_uses_world_speaker_location_under_translated_sound_bank() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let extension = workspace.join("target/release/libsoe_godot_host.so");
    if !extension.exists() {
        eprintln!("skipping: build the release GDExtension before running this test");
        return;
    }
    let godot = godot_binary();

    let fixture = tempfile::tempdir().unwrap();
    let data = fixture.path().join("Data");
    fixture_data(&data);
    let script = fixture.path().join("speaker_world_position.gd");
    std::fs::write(
        &script,
        r#"extends SceneTree

func _initialize() -> void:
	call_deferred("_run")

func _run() -> void:
	var parent := Node3D.new()
	parent.position = Vector3(10.0, 0.0, 0.0)
	root.add_child(parent)
	var bank := ClassDB.instantiate("SoundBank") as Node3D
	parent.add_child(bank)
	await process_frame
	if not bank.call("open_data", OS.get_environment("DLG_11_TEST_DATA")):
		printerr("fixture failed to open: ", bank.call("last_error"))
		quit(1)
		return
	var speaker_position := Vector3(1.25, 0.0, -1.0)
	var slot: int = bank.call("play_voice", OS.get_environment("DLG_11_VOICE"), speaker_position)
	if slot < 0:
		printerr("voice did not start: ", bank.call("last_error"))
		quit(1)
		return
	var player := bank.get_child(slot) as AudioStreamPlayer3D
	if not player.global_position.is_equal_approx(speaker_position):
		printerr("voice is not at speaker: ", player.global_position)
		quit(1)
		return
	if not bank.call("stop_voice", slot) or bank.call("playing_count") != 0:
		printerr("voice did not stop")
		quit(1)
		return
	quit(0)
"#,
    )
    .unwrap();

    let output = Command::new(godot)
        .args(["--path"])
        .arg(project)
        .args(["--resolution", "640x360", "--script"])
        .arg(script)
        .env("DLG_11_TEST_DATA", data)
        .env("DLG_11_VOICE", VOICE)
        .output()
        .expect("Godot process starts");
    assert!(
        output.status.success(),
        "Godot speaker position check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
