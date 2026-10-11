// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used)]

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

const LIMIT: Duration = Duration::from_secs(115);
const FIRST: &str = "Sound/Voice/Dialogue.esm/VoiceHuman/First_00000A00_3.fuz";
const SECOND: &str = "Sound/Voice/Dialogue.esm/VoiceHuman/Second_00000B00_3.fuz";
static GODOT_LOCK: Mutex<()> = Mutex::new(());

struct Run {
    output: Output,
    timed_out: bool,
}

fn fixture_wave() -> Vec<u8> {
    let rate = 22_050_u32;
    let frames = rate as usize;
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
        let sample = if frame < frames / 4 {
            0
        } else {
            ((frame as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 12_000.0) as i16
        };
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn write_fixture(data: &Path) {
    use soe_bsa::{write_bsa, BsaFile, BsaVersion};
    use soe_esm::{FileHeader, Plugin};

    std::fs::create_dir_all(data).expect("fixture data directory is created");
    std::fs::write(
        data.parent()
            .expect("fixture has a parent")
            .join("Skyrim_Default.ini"),
        "[General]\nsLanguage=ENGLISH\n[Archive]\nsResourceArchiveList=Skyrim - Voices_en0.bsa\n",
    )
    .expect("fixture selects English voices");
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
        .expect("vanilla master fixture is written");
    }
    let wave = fixture_wave();
    let fuz = soe_audio::fuz::write(1, &[], &wave);
    let archive = write_bsa(
        &[
            BsaFile {
                path: "sound/voice/dialogue.esm/voicehuman/first_00000a00_3.fuz",
                bytes: &fuz,
            },
            BsaFile {
                path: "sound/voice/dialogue.esm/voicehuman/second_00000b00_3.fuz",
                bytes: &fuz,
            },
        ],
        BsaVersion::SkyrimSe,
    )
    .expect("voice archive is encoded");
    std::fs::write(data.join("Skyrim - Voices_en0.bsa"), archive)
        .expect("voice archive is written");
}

fn read_pipe<R: Read + Send + 'static>(mut pipe: R) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    })
}

fn bounded(mut child: Child) -> Run {
    let stdout = read_pipe(child.stdout.take().expect("stdout is piped"));
    let stderr = read_pipe(child.stderr.take().expect("stderr is piped"));
    let deadline = Instant::now() + LIMIT;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("Godot process status is readable") {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait().expect("timed out Godot process is reaped");
        }
        thread::sleep(Duration::from_millis(20));
    };
    Run {
        output: Output {
            status,
            stdout: stdout.join().expect("stdout reader completes"),
            stderr: stderr.join().expect("stderr reader completes"),
        },
        timed_out,
    }
}

fn launch(data: Option<&Path>) -> Run {
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN points to Godot");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = root.join("godot");
    let mut command = Command::new(godot);
    command
        .arg("--headless")
        .args(["--path"])
        .arg(&project)
        .args(["--script"])
        .arg(project.join("dlg_11_audio_test.gd"))
        .args(["--resolution", "640x360"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(data) = data {
        command
            .env("DLG_11_TEST_DATA", data)
            .env("DLG_11_FIRST_VOICE", FIRST)
            .env("DLG_11_SECOND_VOICE", SECOND);
    } else {
        command
            .env("DLG_11_TEST_DATA", "")
            .env("DLG_11_FIRST_VOICE", "")
            .env("DLG_11_SECOND_VOICE", "");
    }
    bounded(command.spawn().expect("Godot process starts"))
}

fn diagnostic(run: &Run) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.output.stdout),
        String::from_utf8_lossy(&run.output.stderr)
    )
}

#[test]
fn tst_17_headless_mixer_finishes_and_interrupts_voice_within_limit() {
    let _lock = GODOT_LOCK.lock().expect("Godot test lock is available");
    let fixture = tempfile::tempdir().expect("fixture directory is created");
    let data = fixture.path().join("Data");
    write_fixture(&data);

    let run = launch(Some(&data));
    assert!(
        !run.timed_out && run.output.status.success(),
        "Godot mixer did not exit successfully within 115 seconds\n{}",
        diagnostic(&run)
    );
    assert!(
        String::from_utf8_lossy(&run.output.stdout).contains("RESULT: PASS"),
        "Godot mixer omitted its passing completion marker\n{}",
        diagnostic(&run)
    );
}

#[test]
fn tst_17_headless_missing_fixture_exits_with_captured_failure() {
    let _lock = GODOT_LOCK.lock().expect("Godot test lock is available");
    let run = launch(None);
    assert!(
        !run.timed_out && !run.output.status.success(),
        "missing voice fixtures must fail and exit within 115 seconds\n{}",
        diagnostic(&run)
    );
    assert!(
        String::from_utf8_lossy(&run.output.stderr)
            .contains("dialogue voice fixtures were not supplied"),
        "captured failure does not identify missing fixtures\n{}",
        diagnostic(&run)
    );
}

#[test]
fn tst_17_headless_missing_voice_archive_exits_with_captured_failure() {
    let _lock = GODOT_LOCK.lock().expect("Godot test lock is available");
    let fixture = tempfile::tempdir().expect("fixture directory is created");
    let data = fixture.path().join("Data");
    write_fixture(&data);
    std::fs::remove_file(data.join("Skyrim - Voices_en0.bsa"))
        .expect("voice archive is removed for the negative case");

    let run = launch(Some(&data));
    assert!(
        !run.timed_out && !run.output.status.success(),
        "missing voice archive must fail and exit within 115 seconds\n{}",
        diagnostic(&run)
    );
    assert!(
        !run.output.stdout.is_empty() || !run.output.stderr.is_empty(),
        "Godot failure must retain diagnostic output"
    );
}
