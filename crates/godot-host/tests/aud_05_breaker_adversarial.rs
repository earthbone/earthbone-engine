// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_bsa::{write_bsa, BsaFile, BsaVersion};
use soe_esm::{FileHeader, Plugin};
use std::path::{Path, PathBuf};
use std::process::Command;

fn wave(frequency: f32) -> Vec<u8> {
    let sample_rate = 22_050_u32;
    let frames = sample_rate as usize * 4;
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
        let sample = ((frame as f32 * frequency * std::f32::consts::TAU / sample_rate as f32).sin()
            * 12_000.0) as i16;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn write_music_fixture(data: &Path) {
    std::fs::create_dir_all(data).expect("create fixture data directory");
    std::fs::write(
        data.parent().expect("fixture game directory").join("Skyrim_Default.ini"),
        "[General]\nsLanguage=ENGLISH\n[Archive]\nsResourceArchiveList=Skyrim - Sounds.bsa, Skyrim - Voices_en0.bsa\n",
    )
    .expect("write fixture settings");
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
        .expect("write empty master plugin");
    }
    let tracks = [wave(220.0), wave(330.0), wave(440.0)];
    let files = [
        BsaFile {
            path: "music/aud05_first.wav",
            bytes: &tracks[0],
        },
        BsaFile {
            path: "music/aud05_second.wav",
            bytes: &tracks[1],
        },
        BsaFile {
            path: "music/aud05_third.wav",
            bytes: &tracks[2],
        },
    ];
    let archive = write_bsa(&files, BsaVersion::SkyrimSe).expect("write music archive");
    std::fs::write(data.join("Skyrim - Sounds.bsa"), archive)
        .expect("write music archive to fixture data");
    let voice = soe_audio::fuz::write(1, &[], &wave(440.0));
    let voice_archive = write_bsa(
        &[BsaFile {
            path: "sound/voice/aud05/fixture.fuz",
            bytes: &voice,
        }],
        BsaVersion::SkyrimSe,
    )
    .expect("write voice archive");
    std::fs::write(data.join("Skyrim - Voices_en0.bsa"), voice_archive)
        .expect("write voice archive to fixture data");
}

#[test]
fn aud_05_interrupted_fades_leave_only_the_latest_music_instance_playing() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: set SOE_GODOT_BIN to a Godot 4.4 or newer executable");
        return;
    };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    if !workspace
        .join("target/release/libsoe_godot_host.so")
        .exists()
    {
        eprintln!("skipping: build the release GDExtension before running this test");
        return;
    }
    let extension = project.join("soe.gdextension");
    let fixture = tempfile::tempdir().expect("create test fixture");
    let data = fixture.path().join("Data");
    write_music_fixture(&data);

    let script = fixture.path().join("aud_05_breaker.gd");
    std::fs::write(
        &script,
        r#"extends SceneTree

var failures := 0

func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func wait_for_music_count(bank: Node, expected: int) -> bool:
	for _frame in range(120):
		if int(bank.call("music_start_count")) >= expected:
			return true
		await process_frame
	return false

func wait_ready(bank: Node3D, path: String) -> void:
	var deadline := Time.get_ticks_msec() + 5_000
	while str(bank.call("current_music_path")) != path.to_lower() and Time.get_ticks_msec() < deadline:
		await process_frame
	check(str(bank.call("current_music_path")) == path.to_lower(), "decode did not become ready: " + path)
func _initialize() -> void:
	call_deferred("_run")

func _run() -> void:
	if not ClassDB.class_exists("SoundBank"):
		var status := GDExtensionManager.load_extension(OS.get_environment("AUD_05_EXTENSION"))
		check(status == GDExtensionManager.LOAD_STATUS_OK, "extension failed to load")
	var bank := ClassDB.instantiate("SoundBank") as Node3D
	get_root().add_child(bank)
	await process_frame
	check(bank.call("open_data", OS.get_environment("AUD_05_TEST_DATA")), "fixture data did not open")
	check(bank.call("play_music_track", "music/aud05_first.wav", 1.0, 1.0, false), "first track did not start")
	check(await wait_for_music_count(bank, 1), "first track decode did not finish")
	await wait_ready(bank, "music/aud05_first.wav")
	var first_player := bank.get_child(16) as AudioStreamPlayer
	var second_player := bank.get_child(17) as AudioStreamPlayer
	var first_stream = first_player.stream
	check(bank.call("play_music_track", "music/aud05_second.wav", 1.0, 1.0, false), "second track did not start")
	check(await wait_for_music_count(bank, 2), "second track decode did not finish")
	await wait_ready(bank, "music/aud05_second.wav")
	check(first_player.playing and second_player.playing, "first crossfade did not overlap its two tracks")
	check(bank.call("play_music_track", "music/aud05_third.wav", 1.0, 1.0, false), "third track did not interrupt")
	check(await wait_for_music_count(bank, 3), "third track decode did not finish")
	await wait_ready(bank, "music/aud05_third.wav")
	check(not first_player.playing or first_player.stream != first_stream, "stale first instance was not stopped or replaced")
	await create_timer(1.2).timeout
	var active := int(first_player.playing) + int(second_player.playing)
	check(active == 1, "completed fade left stale music instances playing")
	var newest := first_player if first_player.playing else second_player
	check(newest.has_stream_playback(), "latest track has no stream playback")
	check(newest.stream != first_stream, "latest player still carries the first track")
	first_player.stop()
	second_player.stop()
	bank.queue_free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " latest music instance only")
	quit(0 if failures == 0 else 1)
"#,
    )
    .expect("write Godot integration script");

    let output = Command::new(godot)
        .args(["--verbose", "--path"])
        .arg(project)
        .args(["--script"])
        .arg(script)
        .args(["--resolution", "640x360"])
        .env("AUD_05_TEST_DATA", data)
        .env("AUD_05_EXTENSION", extension)
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS"),
        "Godot interrupted music test failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
