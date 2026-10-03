#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_audio::fuz;
use soe_bsa::{write_bsa, BsaFile, BsaVersion};
use soe_esm::{FileHeader, Plugin};
use std::path::PathBuf;
use std::process::Command;

const VOICE: &str = "Sound/Voice/Dialogue.esm/VoiceHuman/Lip_00000A00_3.fuz";

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

fn lip() -> Vec<u8> {
    let frames = [[0.1_f32, 0.2], [0.3, 0.4], [0.5, 0.6], [0.7, 0.8]];
    let mut body = Vec::new();
    body.extend_from_slice(&4_u32.to_le_bytes());
    body.extend_from_slice(&(-2_i32).to_le_bytes());
    body.extend_from_slice(&2_u32.to_le_bytes());
    for frame in frames {
        for weight in frame {
            body.extend_from_slice(&weight.to_le_bytes());
        }
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&(body.len() as u32 + 16).to_le_bytes());
    bytes.extend_from_slice(&8_u32.to_le_bytes());
    bytes.extend_from_slice(&body);
    bytes
}

fn write_fixture_data(data: &std::path::Path) {
    std::fs::create_dir_all(data).unwrap();
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
    let voice = fuz::write(1, &lip(), &wave());
    let archive = write_bsa(
        &[BsaFile {
            path: "sound/voice/dialogue.esm/voicehuman/lip_00000a00_3.fuz",
            bytes: &voice,
        }],
        BsaVersion::SkyrimSe,
    )
    .unwrap();
    std::fs::write(data.join("Skyrim - Voices_en0.bsa"), archive).unwrap();
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
fn dlg_20_live_sound_bank_samples_lip_playback_and_resets_on_interrupt() {
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
    let script = project.join("dlg_20_lip_audio_test.gd");
    let output = Command::new(godot)
        .args(["--resolution", "640x360", "--quit-after", "120", "--path"])
        .arg(&project)
        .args(["--script"])
        .arg(script)
        .env("DLG_20_TEST_DATA", &data)
        .env("DLG_20_VOICE", VOICE)
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS"),
        "Godot DLG-20 LIP playback test failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
