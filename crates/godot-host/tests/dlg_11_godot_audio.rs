// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Runs the dialogue voice path in Godot and captures samples from its mixer.
//! Build the release extension and set `SOE_GODOT_BIN` to enable this test.

use soe_audio::{fuz, SoundSource};
use soe_bsa::{write_bsa, BsaFile, BsaVersion, Vfs};
use soe_esm::{FileHeader, Plugin};
use std::path::{Path, PathBuf};
use std::process::Command;

const FIRST: &str = "Sound/Voice/Dialogue.esm/VoiceHuman/First_00000A00_3.fuz";
const SECOND: &str = "Sound/Voice/Dialogue.esm/VoiceHuman/Second_00000B00_3.fuz";

fn wave() -> Vec<u8> {
    let sample_rate = 22_050_u32;
    let frames = sample_rate as usize;
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
    std::fs::create_dir_all(data).unwrap();
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
    let (first, second) = installed_voice_pair().unwrap_or_else(|| {
        let audio = wave();
        (fuz::write(1, &[], &audio), fuz::write(1, &[], &audio))
    });
    let archive = write_bsa(
        &[
            BsaFile {
                path: "sound/voice/dialogue.esm/voicehuman/first_00000a00_3.fuz",
                bytes: &first,
            },
            BsaFile {
                path: "sound/voice/dialogue.esm/voicehuman/second_00000b00_3.fuz",
                bytes: &second,
            },
        ],
        BsaVersion::SkyrimSe,
    )
    .unwrap();
    std::fs::write(data.join("Skyrim - Voices_en0.bsa"), archive).unwrap();
}

/// Use a real PC voice when the game data is available so the mixer test also
/// exercises the default xWMA decoder. The generated WAV keeps the test usable
/// on machines without an installed game.
fn installed_voice_pair() -> Option<(Vec<u8>, Vec<u8>)> {
    use soe_audio::wma::Native;
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    let archive = data.join("Skyrim - Voices_en0.bsa");
    if !archive.exists() {
        return None;
    }
    let mut vfs = Vfs::new();
    vfs.add_archive_path(&archive).ok()?;
    let source = SoundSource::new(&vfs).with_wma_decoder(&Native);
    let mut names: Vec<_> = vfs
        .paths()
        .filter(|name| {
            name.contains("sound/voice/skyrim.esm/")
                && Path::new(name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("fuz"))
        })
        .collect();
    names.sort_unstable();
    let stride = (names.len() / 60).max(1);
    let mut loudest: Option<(f32, Vec<u8>)> = None;
    for name in names.iter().step_by(stride).take(60) {
        let Ok(sound) = source.read(name) else {
            continue;
        };
        let peak = sound.pcm.peak();
        if loudest.as_ref().is_none_or(|(best, _)| peak > *best) {
            let Ok(bytes) = vfs.read(name) else { continue };
            loudest = Some((peak, bytes));
        }
    }
    let (peak, voice) = loudest?;
    (peak > 0.01).then(|| (voice.clone(), voice))
}

fn godot_binary() -> Option<PathBuf> {
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
}

#[test]
fn dlg_11_voice_archive_fixture_resolves_and_decodes_both_responses() {
    let fixture = tempfile::tempdir().unwrap();
    let data = fixture.path().join("Data");
    write_fixture_data(&data);
    let mut vfs = Vfs::new();
    vfs.add_archive_path(&data.join("Skyrim - Voices_en0.bsa"))
        .unwrap();
    let source = SoundSource::new(&vfs);
    for path in [FIRST, SECOND] {
        let sound = source.read(path).unwrap();
        assert_eq!(sound.pcm.sample_rate, 22_050);
        assert!(sound.pcm.frames() > 20_000);
        assert!(sound.pcm.peak() > 0.1);
    }
}

#[test]
fn dlg_11_godot_mixer_plays_and_interrupts_voice_at_speaker() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let extension = workspace.join("target/release/libsoe_godot_host.so");
    let Some(godot) = godot_binary() else {
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
    let script = project.join("dlg_11_audio_test.gd");
    let output = Command::new(godot)
        .args(["--headless", "--path"])
        .arg(&project)
        .args(["--script"])
        .arg(script)
        .env("DLG_11_TEST_DATA", &data)
        .env("DLG_11_FIRST_VOICE", FIRST)
        .env("DLG_11_SECOND_VOICE", SECOND)
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS"),
        "Godot dialogue audio test failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
