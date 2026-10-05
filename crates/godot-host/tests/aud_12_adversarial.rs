// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;

use soe_bsa::{write_bsa, BsaFile, BsaVersion};
use soe_esm::{FileHeader, Plugin};

fn wave(seconds: f32) -> Vec<u8> {
    let rate = 22_050_u32;
    let frames = (rate as f32 * seconds) as usize;
    let size = (frames * 4) as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 4).to_le_bytes());
    bytes.extend_from_slice(&4_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&size.to_le_bytes());
    for frame in 0..frames {
        let sample = ((frame as f32 * 220.0 * std::f32::consts::TAU / rate as f32).sin()
            * 8_000.0) as i16;
        for _ in 0..2 {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
    }
    bytes
}

fn fixture(data: &Path) {
    std::fs::create_dir_all(data.join("music")).unwrap();
    std::fs::write(
        data.parent().unwrap().join("Skyrim_Default.ini"),
        "[General]\nsLanguage=ENGLISH\n",
    )
    .unwrap();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            data.join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
    for name in ["first", "second", "third"] {
        std::fs::write(data.join(format!("music/{name}.wav")), wave(8.0)).unwrap();
    }
    let bytes = wave(0.1);
    let voices = write_bsa(
        &[BsaFile {
            path: "sound/voice/fixture.wav",
            bytes: &bytes,
        }],
        BsaVersion::SkyrimSe,
    )
    .unwrap();
    std::fs::write(data.join("Skyrim - Voices_en0.bsa"), voices).unwrap();
}

struct RuntimeScript(PathBuf);

impl Drop for RuntimeScript {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn aud_12_stop_fade_does_not_leave_a_stale_player_after_a_new_track_starts() {
    let binary = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN is required");
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert!(
        workspace.join("target/release/libsoe_godot_host.so").exists(),
        "build the release extension before this check"
    );
    let script_path = workspace.join("godot/aud_12_adversarial_runtime.gd");
    let _script = RuntimeScript(script_path.clone());
    std::fs::write(
        &script_path,
        r#"extends SceneTree

func _initialize() -> void:
	call_deferred("_run")

func check(ok: bool, message: String) -> bool:
	if not ok:
		printerr("AUD-12 adversarial failure: ", message)
		quit(1)
		return false
	return true

func wait_path(bank: Node, path: String) -> bool:
	var deadline := Time.get_ticks_msec() + 5000
	while str(bank.call("current_music_path")) != path and Time.get_ticks_msec() < deadline:
		await process_frame
	return check(str(bank.call("current_music_path")) == path, "track did not become current: " + path)

func _run() -> void:
	var bank := ClassDB.instantiate("SoundBank") as Node3D
	root.add_child(bank)
	await process_frame
	if not check(bank.call("open_data", OS.get_environment("AUD_12_TEST_DATA")), "fixture opens"):
		return
	if not check(bank.call("play_music_track", "music/first.wav", 0.0, 60.0, true), "first request"):
		return
	if not await wait_path(bank, "music/first.wav"):
		return
	if not check(bank.call("play_music_track", "music/second.wav", 0.0, 60.0, true), "second request"):
		return
	if not await wait_path(bank, "music/second.wav"):
		return
	bank.call("stop_music")
	if not check(bank.call("play_music_track", "music/third.wav", 0.0, 0.0, true), "third request"):
		return
	if not await wait_path(bank, "music/third.wav"):
		return
	var first := bank.get_child(16) as AudioStreamPlayer
	var second := bank.get_child(17) as AudioStreamPlayer
	if not check(first.playing != second.playing, "stale outgoing music overlaps the new track"):
		return
	print("aud_12_stop_fade_does_not_leave_a_stale_player_after_a_new_track_starts: PASS")
	quit(0)
"#,
    )
    .unwrap();

    let fixture_root = tempfile::tempdir().unwrap();
    let data = fixture_root.path().join("Data");
    fixture(&data);
    let output = Command::new(binary)
        .arg("--path")
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360", "--script"])
        .arg(script_path)
        .env("AUD_12_TEST_DATA", data)
        .output()
        .expect("Godot process starts and exits");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains(
                "aud_12_stop_fade_does_not_leave_a_stale_player_after_a_new_track_starts: PASS"
            ),
        "Godot AUD-12 adversarial check failed: {}\n{stdout}\n{stderr}",
        output.status
    );
}
