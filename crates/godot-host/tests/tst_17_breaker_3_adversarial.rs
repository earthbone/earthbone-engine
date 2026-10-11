// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_audio::fuz;
use soe_bsa::{write_bsa, BsaFile, BsaVersion};
use soe_esm::{FileHeader, Plugin};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const FIRST: &str = "Sound/Voice/Dialogue.esm/VoiceHuman/First_00000A00_3.fuz";
const SECOND: &str = "Sound/Voice/Dialogue.esm/VoiceHuman/Second_00000B00_3.fuz";
const CHILD_LIMIT: Duration = Duration::from_secs(110);
static GODOT_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct BoundedOutput {
    output: Output,
    timed_out: bool,
}

fn read_pipe<R: Read + Send + 'static>(mut pipe: R) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    })
}

fn run_bounded(mut child: Child) -> BoundedOutput {
    let stdout = read_pipe(child.stdout.take().expect("stdout is piped"));
    let stderr = read_pipe(child.stderr.take().expect("stderr is piped"));
    let deadline = Instant::now() + CHILD_LIMIT;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("Godot status is readable") {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait().expect("killed Godot process is reaped");
        }
        thread::sleep(Duration::from_millis(20));
    };
    BoundedOutput {
        output: Output {
            status,
            stdout: stdout.join().expect("stdout reader completes"),
            stderr: stderr.join().expect("stderr reader completes"),
        },
        timed_out,
    }
}

fn failure_message(result: &BoundedOutput) -> String {
    format!(
        "Godot process {}\nstdout:\n{}\nstderr:\n{}",
        if result.timed_out {
            "timed out and was killed"
        } else {
            "exited"
        },
        String::from_utf8_lossy(&result.output.stdout),
        String::from_utf8_lossy(&result.output.stderr)
    )
}

fn wave() -> Vec<u8> {
    let rate = 22_050_u32;
    let frames = rate as usize;
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
        let sample = if frame < frames / 4 {
            0
        } else {
            ((frame as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 12_000.0) as i16
        };
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn fixture(data: &Path) {
    std::fs::create_dir_all(data).unwrap();
    std::fs::write(
        data.parent().unwrap().join("Skyrim_Default.ini"),
        "[General]\nsLanguage=ENGLISH\n[Archive]\nsResourceArchiveList=Skyrim - Voices_en0.bsa\n",
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
    let audio = wave();
    let first = fuz::write(1, &[], &audio);
    let second = fuz::write(1, &[], &audio);
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

fn godot_binary() -> Option<PathBuf> {
    std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from)
}

fn godot_command(godot: &Path, project: &Path, script: &Path, headless: bool) -> Command {
    let mut command = Command::new(godot);
    if headless {
        command.arg("--headless");
    }
    command
        .args(["--path"])
        .arg(project)
        .args(["--script"])
        .arg(script)
        .args(["--resolution", "640x360"]);
    command
}

#[test]
fn tst_17_headless_mixer_run_plays_interrupts_and_quits_before_deadline() {
    let _lock = GODOT_TEST_LOCK.lock().unwrap();
    let Some(godot) = godot_binary() else {
        eprintln!("skipping: SOE_GODOT_BIN is unset");
        return;
    };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project = manifest.join("../../godot");
    let script = project.join("dlg_11_audio_test.gd");
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("Data");
    fixture(&data);
    let result = run_bounded(
        godot_command(&godot, &project, &script, true)
            .env("DLG_11_TEST_DATA", &data)
            .env("DLG_11_FIRST_VOICE", FIRST)
            .env("DLG_11_SECOND_VOICE", SECOND)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("headless Godot process starts"),
    );
    let stdout = String::from_utf8_lossy(&result.output.stdout);
    assert!(
        !result.timed_out && result.output.status.success() && stdout.contains("RESULT: PASS"),
        "{}",
        failure_message(&result)
    );
    assert!(
        stdout.contains("dialogue audio playback"),
        "{}",
        failure_message(&result)
    );
}

#[test]
fn tst_17_audio_server_mixer_run_plays_interrupts_and_quits_before_deadline() {
    let _lock = GODOT_TEST_LOCK.lock().unwrap();
    let Some(godot) = godot_binary() else {
        eprintln!("skipping: SOE_GODOT_BIN is unset");
        return;
    };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project = manifest.join("../../godot");
    let script = project.join("dlg_11_audio_test.gd");
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("Data");
    fixture(&data);
    let result = run_bounded(
        godot_command(&godot, &project, &script, false)
            .env("DLG_11_TEST_DATA", &data)
            .env("DLG_11_FIRST_VOICE", FIRST)
            .env("DLG_11_SECOND_VOICE", SECOND)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("displayed Godot process starts"),
    );
    let stdout = String::from_utf8_lossy(&result.output.stdout);
    assert!(
        !result.timed_out && result.output.status.success() && stdout.contains("RESULT: PASS"),
        "{}",
        failure_message(&result)
    );
}

#[test]
fn tst_17_missing_voice_fixtures_quit_with_captured_diagnostic() {
    let _lock = GODOT_TEST_LOCK.lock().unwrap();
    let Some(godot) = godot_binary() else {
        eprintln!("skipping: SOE_GODOT_BIN is unset");
        return;
    };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project = manifest.join("../../godot");
    let script = project.join("dlg_11_audio_test.gd");
    let result = run_bounded(
        godot_command(&godot, &project, &script, true)
            .env("DLG_11_TEST_DATA", "")
            .env("DLG_11_FIRST_VOICE", "")
            .env("DLG_11_SECOND_VOICE", "")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Godot process starts"),
    );
    let stderr = String::from_utf8_lossy(&result.output.stderr);
    assert!(
        !result.timed_out
            && !result.output.status.success()
            && stderr.contains("dialogue voice fixtures were not supplied"),
        "{}",
        failure_message(&result)
    );
}
