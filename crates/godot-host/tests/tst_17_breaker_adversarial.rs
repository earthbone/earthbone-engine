// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used)]

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

const GODOT_LIMIT: Duration = Duration::from_secs(110);
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
            .expect("fixture parent exists")
            .join("Skyrim_Default.ini"),
        "[General]\nsLanguage=ENGLISH\n[Archive]\nsResourceArchiveList=Skyrim - Voices_en0.bsa\n",
    )
    .expect("fixture selects the English archive");
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
    let fuz = soe_audio::fuz::write(1, &[], &fixture_wave());
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
    .expect("fixture voice archive is encoded");
    std::fs::write(data.join("Skyrim - Voices_en0.bsa"), archive)
        .expect("fixture voice archive is written");
}

fn read_pipe<R: Read + Send + 'static>(mut pipe: R) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    })
}

fn run_bounded(mut child: Child) -> Run {
    let stdout = read_pipe(child.stdout.take().expect("stdout is piped"));
    let stderr = read_pipe(child.stderr.take().expect("stderr is piped"));
    let deadline = Instant::now() + GODOT_LIMIT;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("Godot status is readable") {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait().expect("timed out Godot child is reaped");
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

fn godot() -> PathBuf {
    std::env::var_os("SOE_GODOT_BIN")
        .map(PathBuf::from)
        .expect("SOE_GODOT_BIN points to Godot")
}

fn launch(data: Option<&Path>) -> Run {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = root.join("godot");
    let mut command = Command::new(godot());
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
    run_bounded(command.spawn().expect("Godot process starts"))
}

fn diagnostic(run: &Run) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.output.stdout),
        String::from_utf8_lossy(&run.output.stderr)
    )
}

#[test]
fn tst_17_headless_fixture_plays_interrupts_and_exits() {
    let _lock = GODOT_LOCK.lock().expect("Godot test lock is available");
    let temp = tempfile::tempdir().expect("fixture directory is created");
    let data = temp.path().join("Data");
    write_fixture(&data);

    let run = launch(Some(&data));
    assert!(
        !run.timed_out && run.output.status.success(),
        "Godot did not exit successfully within 110 seconds\n{}",
        diagnostic(&run)
    );
    assert!(
        String::from_utf8_lossy(&run.output.stdout).contains("RESULT: PASS"),
        "Godot omitted its passing completion marker\n{}",
        diagnostic(&run)
    );
}

#[test]
fn tst_17_missing_fixture_exits_with_captured_failure() {
    let _lock = GODOT_LOCK.lock().expect("Godot test lock is available");
    let run = launch(None);
    let stderr = String::from_utf8_lossy(&run.output.stderr);
    assert!(
        !run.timed_out && !run.output.status.success(),
        "missing voice fixtures must fail and exit before the wall-clock limit\n{}",
        diagnostic(&run)
    );
    assert!(
        stderr.contains("dialogue voice fixtures were not supplied"),
        "captured failure does not identify missing fixtures\n{}",
        diagnostic(&run)
    );
}
