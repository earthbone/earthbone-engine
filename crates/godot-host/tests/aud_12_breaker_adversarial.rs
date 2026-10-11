// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
#![cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]

//! Host-visible AUD-12 replacement and installed transition checks.

use std::path::{Path, PathBuf};
use std::process::Command;

use soe_audio::SoundSource;
use soe_bsa::Vfs;
use soe_esm::{cursor::zstring, LoadOrder, Plugin, Tag};

#[test]
fn aud_12_normalized_track_aliases_share_the_request_key() {
    assert_eq!(
        soe_bsa::normalize("Music\\Region\\Track.XWM"),
        soe_bsa::normalize("music/region/track.xwm")
    );
}

fn write_wave(path: &Path, seconds: f32) {
    let rate = 22_050_u32;
    let frames = (rate as f32 * seconds) as usize;
    let data_size = (frames * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_size as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());
    for frame in 0..frames {
        let sample = ((frame as f32 * 220.0 * std::f32::consts::TAU / rate as f32).sin()
            * 8_000.0) as i16;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn aud_12_failed_music_and_ambience_replacements_keep_current_tracks() {
    let Some(binary) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("skipping: SOE_GODOT_BIN is not set");
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let extension = workspace.join("target/release/libsoe_godot_host.so");
    if !extension.exists() {
        eprintln!("skipping: release extension is not built");
        return;
    }
    let fixture = tempfile::tempdir().unwrap();
    let data = fixture.path().join("Data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(
        fixture.path().join("Skyrim_Default.ini"),
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
            Plugin::new(soe_esm::FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
    write_wave(&data.join("Music/valid.wav"), 5.0);
    std::fs::write(data.join("Music/broken.wav"), b"not a wave").unwrap();
    let script = fixture.path().join("aud_12_failed_replacement.gd");
    std::fs::write(
        &script,
        r#"extends SceneTree

func _initialize() -> void:
	call_deferred("run")

func fail(message: String) -> void:
	printerr("AUD-12 failure: ", message)
	quit(1)

func run() -> void:
	var bank := ClassDB.instantiate("SoundBank") as Node3D
	root.add_child(bank)
	await process_frame
	if not bank.call("open_data", OS.get_environment("AUD_12_DATA")):
		fail("fixture did not open")
		return
	if not bank.call("play_music_track", "Music/valid.wav", 0.0, 0.0, true):
		fail("valid track request failed")
		return
	var deadline := Time.get_ticks_msec() + 5000
	while str(bank.call("current_music_path")) != "music/valid.wav" and Time.get_ticks_msec() < deadline:
		await process_frame
	if str(bank.call("current_music_path")) != "music/valid.wav":
		fail("valid track did not start")
		return
	if not bank.call("play_ambience", "Music/valid.wav"):
		fail("valid ambience request failed")
		return
	deadline = Time.get_ticks_msec() + 5000
	while str(bank.call("current_ambience_path")) != "music/valid.wav" and Time.get_ticks_msec() < deadline:
		await process_frame
	if str(bank.call("current_ambience_path")) != "music/valid.wav":
		fail("valid ambience did not start")
		return
	if not bank.call("play_music_track", "Music/broken.wav", 0.0, 0.0, true):
		fail("replacement request was rejected before decode")
		return
	if not bank.call("play_ambience", "Music/broken.wav"):
		fail("ambience replacement request was rejected before decode")
		return
	deadline = Time.get_ticks_msec() + 5000
	while str(bank.call("last_error")) == "" and Time.get_ticks_msec() < deadline:
		await process_frame
	if str(bank.call("current_music_path")) != "music/valid.wav" or int(bank.call("music_playing_count")) != 1:
		fail("failed replacement stopped or displaced current music")
		return
	if str(bank.call("current_ambience_path")) != "music/valid.wav" or int(bank.call("ambience_start_count")) != 1:
		fail("failed replacement stopped or displaced current ambience")
		return
	print("aud_12_failed_music_and_ambience_replacements_keep_current_tracks: PASS")
	quit(0)
"#,
    )
    .unwrap();
    let output = Command::new(binary)
        .arg("--path")
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360", "--script"])
        .arg(script)
        .env("AUD_12_DATA", data)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("aud_12_failed_music_and_ambience_replacements_keep_current_tracks: PASS"),
        "AUD-12 failed replacement check failed\n{stdout}\n{stderr}"
    );
}

fn installed_tracks(data: &Path) -> Vec<String> {
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let Ok(bytes) = std::fs::read(data.join(name)) else {
            return Vec::new();
        };
        let Ok(plugin) = Plugin::parse(&bytes) else {
            return Vec::new();
        };
        if order.add(name, plugin).is_err() {
            return Vec::new();
        }
    }
    let mut vfs = Vfs::new();
    if vfs.add_archive_path(&data.join("Skyrim - Sounds.bsa")).is_err()
        || vfs.add_loose_root(data).is_err()
    {
        return Vec::new();
    }
    let source = SoundSource::new(&vfs);
    let mut paths: Vec<_> = order
        .winners_of(Tag(*b"MUST"))
        .filter_map(|(_, record)| record.subrecords.iter().find(|sub| sub.tag == Tag(*b"ANAM")))
        .filter_map(|sub| source.resolve(&zstring(&sub.data)))
        .collect();
    paths.sort_unstable();
    paths.dedup();
    paths
        .into_iter()
        .filter(|path| {
            vfs.read(path).is_ok_and(|bytes| {
                soe_audio::Xwma::parse(&bytes).is_ok_and(|track| {
                    (4.0..40.0).contains(&track.duration_seconds())
                        && track.decoded_bytes() <= 8 * 1024 * 1024
                })
            })
        })
        .take(2)
        .collect()
}

#[test]
fn aud_12_installed_prf12_changes_tracks_and_measures_each_transition() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let Some(binary) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("skipping: SOE_GODOT_BIN is not set");
        return;
    };
    let paths = installed_tracks(&data);
    assert_eq!(paths.len(), 2, "installed data must provide two short xWMA tracks");
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let extension = workspace.join("target/release/libsoe_godot_host.so");
    if !extension.exists() {
        eprintln!("skipping: release extension is not built");
        return;
    }
    let output = Command::new(binary)
        .arg("--path")
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360", "--script"])
        .arg(workspace.join("godot/aud_12_walkthrough_test.gd"))
        .env("SKYRIM_DATA", data)
        .env("AUD_12_MUSIC_A", &paths[0])
        .env("AUD_12_MUSIC_B", &paths[1])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("aud_12_installed_walkthrough_music_starts_and_changes_under_two_ms: PASS"),
        "installed AUD-12 transition check failed\n{stdout}\n{stderr}"
    );
}
