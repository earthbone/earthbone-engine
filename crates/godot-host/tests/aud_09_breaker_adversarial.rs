// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_bsa::{write_bsa, BsaFile, BsaVersion};
use soe_esm::{FileHeader, Plugin};
use std::path::{Path, PathBuf};
use std::process::Command;

fn wave(frequency: f32) -> Vec<u8> {
    let rate = 22_050_u32;
    let frames = rate as usize * 4;
    let data_bytes = (frames * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_bytes as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    for frame in 0..frames {
        let sample = ((frame as f32 * frequency * std::f32::consts::TAU / rate as f32).sin()
            * 12_000.0) as i16;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn fixture(data: &Path) {
    std::fs::create_dir_all(data.join("Music")).unwrap();
    std::fs::write(
        data.parent().unwrap().join("Skyrim_Default.ini"),
        "[General]\nsLanguage=ENGLISH\n",
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
    std::fs::write(data.join("Music/First.wav"), wave(220.0)).unwrap();
    std::fs::write(data.join("Music/Second.wav"), wave(440.0)).unwrap();
    let empty = [];
    let voices = write_bsa(
        &[BsaFile {
            path: "sound/voice/placeholder.wav",
            bytes: &empty,
        }],
        BsaVersion::SkyrimSe,
    )
    .unwrap();
    std::fs::write(data.join("Skyrim - Voices_en0.bsa"), voices).unwrap();
}

#[test]
fn aud_09_replacing_cell_ambience_reuses_player_and_release_is_idempotent() {
    let godot = PathBuf::from(std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN set"));
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let extension = project.join("soe.gdextension");
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("Data");
    fixture(&data);
    let script = tmp.path().join("aud_09_replacement.gd");
    std::fs::write(
        &script,
        r#"extends SceneTree
var failures := 0
func check(value: bool, message: String) -> void:
    if not value:
        failures += 1
        printerr("FAIL: ", message)
func _initialize() -> void:
    call_deferred("run")
func run() -> void:
    if not ClassDB.class_exists("SoundBank"):
        GDExtensionManager.load_extension("res://soe.gdextension")
    var bank := ClassDB.instantiate("SoundBank") as Node3D
    root.add_child(bank)
    await process_frame
    check(bank.call("open_data", OS.get_environment("AUD_09_TEST_DATA")), "fixture data opens")
    var player := bank.get_node("AmbiencePlayer") as AudioStreamPlayer
    check(bank.call("play_ambience", "Music/First.wav"), "first cell ambience starts")
    await process_frame
    check(player.playing, "first descriptor owns active playback")
    var first_data: PackedByteArray = player.stream.data
    check(bank.call("play_ambience", "Music/Second.wav"), "next cell ambience starts")
    await process_frame
    check(player.playing, "replacement descriptor plays on the same player")
    check(player.stream.data != first_data, "departed descriptor stream is replaced")
    bank.call("stop_ambience")
    await process_frame
    check(not player.playing, "release stops current cell ambience")
    bank.call("stop_ambience")
    await process_frame
    check(not player.playing, "repeated release remains stopped")
    bank.queue_free()
    print("RESULT: ", "PASS" if failures == 0 else "FAIL")
    quit(0 if failures == 0 else 1)
"#,
    )
    .unwrap();
    let output = Command::new(godot)
        .args(["--path"])
        .arg(&project)
        .args(["--resolution", "640x360", "--script"])
        .arg(script)
        .env("AUD_09_TEST_DATA", data)
        .env("AUD_09_EXTENSION", extension)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS"),
        "Godot failed\n{stdout}\n{stderr}"
    );
}
