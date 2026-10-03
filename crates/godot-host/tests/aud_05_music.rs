// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_bsa::{write_bsa, BsaFile, BsaVersion};
use soe_esm::{FileHeader, Plugin};
use soe_godot_host::audio::MusicFade;
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
        data.parent()
            .expect("fixture game directory")
            .join("Skyrim_Default.ini"),
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
    let first = wave(220.0);
    let second = wave(330.0);
    let third = wave(440.0);
    let archive = write_bsa(
        &[
            BsaFile {
                path: "music/aud05_first.wav",
                bytes: &first,
            },
            BsaFile {
                path: "music/aud05_second.wav",
                bytes: &second,
            },
            BsaFile {
                path: "music/aud05_third.wav",
                bytes: &third,
            },
        ],
        BsaVersion::SkyrimSe,
    )
    .expect("write music archive");
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
fn aud_05_music_fade_applies_both_record_durations_and_finishes_cleanly() {
    let mut fade = MusicFade::new(0.8, 2.0, 4.0, false);
    assert_eq!(fade.advance(1.0), (0.6, 0.5, false));
    assert_eq!(fade.advance(1.0), (0.4, 1.0, false));
    assert_eq!(fade.advance(2.0), (0.0, 1.0, true));
}

#[test]
fn aud_05_music_abrupt_transition_ends_both_slots_without_a_tail() {
    let mut fade = MusicFade::new(1.0, 3.0, 8.0, true);
    assert_eq!(fade.advance(0.0), (0.0, 1.0, true));
}

#[test]
fn aud_05_music_restarted_transition_does_not_retain_an_old_fade_object() {
    let mut first = MusicFade::new(1.0, 5.0, 5.0, false);
    let (old_level, new_level, done) = first.advance(1.0);
    assert_eq!((old_level, new_level, done), (0.8, 0.2, false));

    let mut latest = MusicFade::new(new_level, 1.0, 1.0, false);
    assert_eq!(latest.advance(1.0), (0.0, 1.0, true));
}

#[test]
fn aud_05_godot_interrupted_transition_replaces_stale_player_stream() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let extension_library = workspace.join("target/release/libsoe_godot_host.so");
    let extension_config = project.join("soe.gdextension");
    let godot = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from);
    let Some(godot) = godot else {
        eprintln!("skipping: set SOE_GODOT_BIN to a Godot 4.4 or newer executable");
        return;
    };
    if !extension_library.exists() {
        eprintln!("skipping: build the release GDExtension before running this test");
        return;
    }

    let fixture = tempfile::tempdir().expect("create test fixture");
    let data = fixture.path().join("Data");
    write_music_fixture(&data);
    let output = Command::new(godot)
        .args(["--verbose", "--path"])
        .arg(&project)
        .args(["--script"])
        .arg(project.join("aud_05_music_test.gd"))
        .args(["--resolution", "640x360"])
        .env("AUD_05_TEST_DATA", &data)
        .env("AUD_05_EXTENSION", &extension_config)
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS"),
        "Godot music transition test failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
