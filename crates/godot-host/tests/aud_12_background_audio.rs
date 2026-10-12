// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AUD-12 worker playback and installed PRF-12 walkthrough frame acceptance.

use std::path::{Path, PathBuf};
use std::process::Command;

use soe_audio::{SoundSource, Xwma};
use soe_bsa::{write_bsa, BsaFile, BsaVersion, Vfs};
use soe_esm::{cursor::zstring, FileHeader, LoadOrder, Plugin, Tag};

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
        for frequency in [220.0, 330.0] {
            let sample = ((frame as f32 * frequency * std::f32::consts::TAU / rate as f32).sin()
                * 8_000.0) as i16;
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
    std::fs::write(data.join("music/short.wav"), wave(0.1)).unwrap();
    std::fs::write(data.join("music/broken.wav"), b"invalid audio").unwrap();
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

fn godot(script: &str) -> Option<Command> {
    let Some(binary) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("skipping: SOE_GODOT_BIN is not set");
        return None;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert!(
        workspace
            .join("target/release/libsoe_godot_host.so")
            .exists(),
        "build the release extension first"
    );
    let mut command = Command::new(binary);
    command
        .arg("--path")
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360", "--script"])
        .arg(workspace.join("godot").join(script));
    Some(command)
}

fn run(mut command: Command, marker: &str) {
    let output = command.output().expect("Godot process starts and exits");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stdout.lines().filter(|line| {
        line.contains("AUD-12")
            || line.starts_with("aud_12_")
            || line.starts_with("audio ")
            || line.starts_with("SOE systems:")
            || line.starts_with("STARTUP")
            || line.starts_with("WALKTHROUGH stopped")
    }) {
        eprintln!("{line}");
    }
    assert!(
        output.status.success() && stdout.contains(marker),
        "Godot AUD-12 failed: {}\n{stdout}\n{stderr}",
        output.status
    );
}

#[test]
fn aud_12_generator_playback_cache_cancellation_and_errors() {
    let Some(mut command) = godot("aud_12_audio_test.gd") else {
        return;
    };
    let fixture_root = tempfile::tempdir().unwrap();
    let data = fixture_root.path().join("Data");
    fixture(&data);
    command.env("AUD_12_TEST_DATA", data);
    run(
        command,
        "aud_12_generator_playback_cache_cancellation_and_errors: PASS",
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
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let mut vfs = Vfs::new();
    vfs.add_archive_path(&data.join("Skyrim - Sounds.bsa"))
        .unwrap();
    vfs.add_loose_root(data).unwrap();
    let source = SoundSource::new(&vfs);
    let mut paths: Vec<_> = order
        .winners_of(Tag(*b"MUST"))
        .filter_map(|(_, record)| {
            record
                .subrecords
                .iter()
                .find(|sub| sub.tag == Tag(*b"ANAM"))
        })
        .filter_map(|sub| source.resolve(&zstring(&sub.data)))
        .collect();
    paths.sort_unstable();
    paths.dedup();
    paths
        .into_iter()
        .filter(|path| {
            let bytes = vfs.read(path).unwrap();
            Xwma::parse(&bytes).is_ok_and(|track| {
                // CONTEXT: two short real tracks fit together in the declared PCM
                // cache, so the return transition checks reuse rather than eviction.
                (4.0..40.0).contains(&track.duration_seconds())
                    && track.decoded_bytes() <= 8 * 1024 * 1024
            })
        })
        .take(2)
        .collect()
}

#[test]
fn aud_12_installed_walkthrough_music_starts_and_changes_under_two_ms() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let Some(mut command) = godot("aud_12_walkthrough_test.gd") else {
        return;
    };
    let paths = installed_tracks(&data);
    assert_eq!(
        paths.len(),
        2,
        "installed MUST records supply two cacheable xWMA tracks"
    );
    eprintln!("AUD-12 installed transition tracks: {paths:?}");
    command
        .env("SKYRIM_DATA", data)
        .env("AUD_12_MUSIC_A", &paths[0])
        .env("AUD_12_MUSIC_B", &paths[1]);
    run(
        command,
        "aud_12_installed_walkthrough_music_starts_and_changes_under_two_ms: PASS",
    );
}
