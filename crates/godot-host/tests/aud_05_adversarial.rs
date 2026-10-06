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
    std::fs::create_dir_all(data).unwrap();
    std::fs::write(
        data.parent().unwrap().join("Skyrim_Default.ini"),
        "[General]\nsLanguage=ENGLISH\n[Archive]\nsResourceArchiveList=Skyrim - Sounds.bsa, Skyrim - Voices_en0.bsa\n",
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
    let first = wave(220.0);
    let second = wave(330.0);
    let third = wave(440.0);
    let files = [
        BsaFile {
            path: "music/end_first.wav",
            bytes: &first,
        },
        BsaFile {
            path: "music/end_second.wav",
            bytes: &second,
        },
        BsaFile {
            path: "music/end_third.wav",
            bytes: &third,
        },
    ];
    let archive = write_bsa(&files, BsaVersion::SkyrimSe).unwrap();
    std::fs::write(data.join("Skyrim - Sounds.bsa"), archive).unwrap();
    let voice = soe_audio::fuz::write(1, &[], &wave(440.0));
    let voices = write_bsa(
        &[BsaFile {
            path: "sound/voice/aud05/fixture.fuz",
            bytes: &voice,
        }],
        BsaVersion::SkyrimSe,
    )
    .unwrap();
    std::fs::write(data.join("Skyrim - Voices_en0.bsa"), voices).unwrap();
}

#[test]
fn aud_05_completed_interrupted_fade_leaves_only_latest_track_playing() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: set SOE_GODOT_BIN to a Godot 4.4 or newer executable");
        return;
    };

    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = workspace.join("godot");
    // CONTEXT: Cargo test does not refresh the release library Godot loads.
    let build = Command::new(env!("CARGO"))
        .current_dir(&workspace)
        .args(["build", "-p", "soe-godot-host", "--release", "--target-dir"])
        .arg(workspace.join("target"))
        .output()
        .expect("release extension build starts");
    assert!(
        build.status.success(),
        "release extension build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let extension = project.join("soe.gdextension");
    if !project
        .join("../target/release/libsoe_godot_host.so")
        .exists()
    {
        eprintln!("skipping: build the release GDExtension before running this test");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("Data");
    fixture(&data);
    let script = tmp.path().join("aud_05_final_fade.gd");
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
func wait_for_music_count(bank: Node, expected: int) -> bool:
    for _frame in range(120):
        if int(bank.call("music_start_count")) >= expected:
            return true
        await process_frame
    return false
func run() -> void:
    if not ClassDB.class_exists("SoundBank"):
        GDExtensionManager.load_extension(OS.get_environment("AUD_05_EXTENSION"))
    if not ClassDB.class_exists("SoundBank"):
        printerr("FAIL: SoundBank extension failed to load")
        quit(1)
        return
    var bank := ClassDB.instantiate("SoundBank") as Node3D
    root.add_child(bank)
    await process_frame
    check(bank.call("open_data", OS.get_environment("AUD_05_TEST_DATA")), "fixture data opens")
    check(bank.call("play_music_track", "music/end_first.wav", 0.12, 0.12, false), "first track starts")
    check(await wait_for_music_count(bank, 1), "first track decode finishes")
    check(bank.call("play_music_track", "music/end_second.wav", 0.12, 0.12, false), "second track starts")
    check(await wait_for_music_count(bank, 2), "second track decode finishes")
    check(bank.call("play_music_track", "music/end_third.wav", 0.12, 0.12, false), "third track interrupts fade")
    check(await wait_for_music_count(bank, 3), "third track decode finishes")
    var latest_data: PackedByteArray = (bank.get_child(16) as AudioStreamPlayer).stream.data
    await create_timer(0.3).timeout
    var first := bank.get_child(16) as AudioStreamPlayer
    var second := bank.get_child(17) as AudioStreamPlayer
    check(first.playing != second.playing, "completed fade leaves exactly one music slot playing")
    check(first.playing and first.stream.data == latest_data, "latest track remains active after fade completion")
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
        .env("AUD_05_TEST_DATA", data)
        .env("AUD_05_EXTENSION", extension)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS"),
        "Godot failed\n{stdout}\n{stderr}"
    );
}
