// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]

//! Scene checks use record fixtures and the installed Bannered Mare door route.

#[path = "common/isolated_project.rs"]
mod isolated_project;

use soe_bsa::{write_bsa, BsaFile, BsaVersion};
use soe_esm::{FileHeader, Plugin, Record, Tag};
use std::path::{Path, PathBuf};
use std::process::Command;

fn wave() -> Vec<u8> {
    let rate = 22_050_u32;
    let frames = rate as usize;
    let size = (frames * 2) as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&size.to_le_bytes());
    for frame in 0..frames {
        let sample =
            ((frame as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 12000.0) as i16;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn fixture(data: &Path) {
    std::fs::create_dir_all(data.join("Sound")).unwrap();
    std::fs::write(
        data.parent().unwrap().join("Skyrim_Default.ini"),
        "[General]\nsLanguage=ENGLISH\n",
    )
    .unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    for (cell, space, flags) in [
        (0x800_u32, 0x900_u32, 1),
        (0x801, 0x901, 1),
        (0x802, 0, 0),
        (0x803, 0, 1),
        (0x804, 0x904, 1),
    ] {
        let mut record = Record::new(Tag(*b"CELL"), cell).sub(Tag(*b"DATA"), [flags]);
        if space != 0 {
            record = record.sub(Tag(*b"XCAS"), space.to_le_bytes());
        }
        if cell == 0x802 {
            record = record.sub(Tag(*b"XCLR"), 0x902_u32.to_le_bytes());
        }
        plugin.push_top(record);
    }
    for (space, sound) in [(0x900_u32, 0xA00_u32), (0x901, 0xA01), (0x904, 0xA04)] {
        plugin.push_top(Record::new(Tag(*b"ASPC"), space).sub(Tag(*b"SNAM"), sound.to_le_bytes()));
    }
    let mut sounds = Vec::new();
    for (sound, weather) in [(0xA02_u32, 1_u32), (0xA03, 2)] {
        sounds.extend_from_slice(&sound.to_le_bytes());
        sounds.extend_from_slice(&weather.to_le_bytes());
        sounds.extend_from_slice(&1_f32.to_le_bytes());
    }
    plugin.push_top(
        Record::new(Tag(*b"REGN"), 0x902)
            .sub(Tag(*b"RDAT"), [7, 0, 0, 0, 0, 50, 0, 0])
            .sub(Tag(*b"RDSA"), sounds),
    );
    for (object, path) in [
        (0xA00, "sound/a.wav"),
        (0xA01, "sound/b.wav"),
        (0xA02, "c.wav"),
        (0xA03, "d.wav"),
        (0xA04, "sound/missing.wav"),
    ] {
        plugin.push_top(
            Record::new(Tag(*b"SNDR"), object)
                .sub(Tag(*b"CNAM"), 0x1EEF_540A_u32.to_le_bytes())
                .sub(Tag(*b"BNAM"), [0, 0, 50, 0, 0, 0])
                .sub(Tag(*b"ONAM"), 0xB00_u32.to_le_bytes())
                .sub(Tag(*b"LNAM"), [0, 8, 0, 0])
                .zsub(Tag(*b"ANAM"), path),
        );
    }
    plugin.push_top(Record::new(Tag(*b"SOPM"), 0xB00).sub(Tag(*b"MNAM"), 1_u32.to_le_bytes()));
    for master in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let bytes = if master == "Skyrim.esm" {
            plugin.to_bytes()
        } else {
            Plugin::new(FileHeader::default()).to_bytes()
        };
        std::fs::write(data.join(master), bytes).unwrap();
    }
    let voices = write_bsa(
        &[BsaFile {
            path: "sound/voice/placeholder.wav",
            bytes: &[],
        }],
        BsaVersion::SkyrimSe,
    )
    .unwrap();
    std::fs::write(data.join("Skyrim - Voices_en0.bsa"), voices).unwrap();
    for name in ["a", "b", "c", "d"] {
        std::fs::write(data.join(format!("Sound/{name}.wav")), wave()).unwrap();
    }
}

fn run_scene(script: &str, data: &Path) {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("skipping: SOE_GODOT_BIN is not set");
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = isolated_project::project(
        &[script.trim_start_matches("res://")],
        Some(&workspace.join("target/release/libsoe_godot_host.so")),
    );
    let mut command = Command::new(godot);
    if script == "res://aud_13_transition_test.gd" {
        command.arg("--headless");
    }
    let output = command
        .arg("--path")
        .arg(project.path())
        .args(["--resolution", "640x360", "--script"])
        .arg(script)
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot scene starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let logs = workspace.join("target/aud_13");
    std::fs::create_dir_all(&logs).unwrap();
    let name = script.rsplit('/').next().unwrap();
    std::fs::write(logs.join(format!("{name}.stdout.log")), &output.stdout).unwrap();
    std::fs::write(logs.join(format!("{name}.stderr.log")), &output.stderr).unwrap();
    for line in stdout.lines().filter(|line| line.contains("AUD-13")) {
        println!("{line}");
    }
    let failures = stderr
        .lines()
        .filter(|line| line.starts_with("FAIL:") || line.contains("SCRIPT ERROR"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        output.status.success()
            && stdout.contains("AUD-13 RESULT: PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "Godot scene failed: {}\n{failures}\nFull logs: {}",
        output.status,
        logs.display()
    );
}

#[test]
fn aud_13_scene_crossfades_acoustic_and_regional_ambience_and_cancels_stale_decodes() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = tempfile::tempdir_in(workspace).unwrap();
    let data = directory.path().join("Data");
    fixture(&data);
    run_scene("res://aud_13_ambience_test.gd", &data);
}

#[test]
fn aud_13_installed_bannered_mare_to_plains_keeps_master_above_minus_sixty_db_for_twenty_seconds() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    run_scene("res://aud_13_transition_test.gd", &data);
}
