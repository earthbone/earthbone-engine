// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Runs cell ambience through the host's non-positional Godot player.

use soe_bsa::{write_bsa, BsaFile, BsaVersion};
use soe_esm::{FileHeader, Plugin};
use std::path::{Path, PathBuf};
use std::process::Command;

fn wave() -> Vec<u8> {
    let sample_rate = 22_050_u32;
    let frames = sample_rate as usize * 8;
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

fn write_fixture_data(data: &Path) {
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
    std::fs::write(data.join("Music/Cell.wav"), wave()).unwrap();
    let placeholder = [];
    let voices = write_bsa(
        &[BsaFile {
            path: "sound/voice/placeholder.wav",
            bytes: &placeholder,
        }],
        BsaVersion::SkyrimSe,
    )
    .unwrap();
    std::fs::write(data.join("Skyrim - Voices_en0.bsa"), voices).unwrap();
}

#[test]
fn aud_09_godot_ambience_starts_and_stops_on_cell_transition() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let extension = workspace.join("target/release/libsoe_godot_host.so");
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: set SOE_GODOT_BIN to a Godot 4.4 or newer executable");
        return;
    };
    if !extension.exists() {
        eprintln!("skipping: build the release GDExtension before running this test");
        return;
    }

    let fixture = tempfile::tempdir().unwrap();
    let data = fixture.path().join("Data");
    write_fixture_data(&data);
    let script = project.join("aud_09_ambience_test.gd");
    let output = Command::new(godot)
        .args(["--path"])
        .arg(&project)
        .args(["--windowed", "--resolution", "640x360", "--script"])
        .arg(script)
        .env("AUD_09_TEST_DATA", &data)
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS"),
        "Godot ambience test failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn aud_09_playernode_cell_change_starts_and_releases_ambience() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: SOE_GODOT_BIN is not set");
        return;
    };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let extension = workspace.join("target/release/libsoe_godot_host.so");
    assert!(
        extension.exists(),
        "build the release GDExtension before running this test"
    );

    let output = Command::new(godot)
        .args(["--path"])
        .arg(&project)
        .args(["--windowed", "--resolution", "640x360", "--script"])
        .arg(project.join("aud_09_player_cell_test.gd"))
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot PlayerNode ambience process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("AUD-09 RESULT: PASS"),
        "PlayerNode ambience transition failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
