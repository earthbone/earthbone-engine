// SPDX-License-Identifier: GPL-3.0-or-later

const RUST_CHECK: &str = include_str!("dlg_11_godot_audio.rs");
const GODOT_SCRIPT: &str = include_str!("../../../godot/dlg_11_audio_test.gd");

fn voice_wave() -> Vec<u8> {
    let rate = 22_050_u32;
    let frames = rate as usize;
    let bytes = (frames * 2) as u32;
    let mut wave = Vec::with_capacity(44 + bytes as usize);
    wave.extend_from_slice(b"RIFF");
    wave.extend_from_slice(&(36 + bytes).to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16_u32.to_le_bytes());
    wave.extend_from_slice(&1_u16.to_le_bytes());
    wave.extend_from_slice(&1_u16.to_le_bytes());
    wave.extend_from_slice(&rate.to_le_bytes());
    wave.extend_from_slice(&(rate * 2).to_le_bytes());
    wave.extend_from_slice(&2_u16.to_le_bytes());
    wave.extend_from_slice(&16_u16.to_le_bytes());
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&bytes.to_le_bytes());
    for frame in 0..frames {
        let sample = if frame < frames / 4 {
            0
        } else {
            ((frame as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 12_000.0)
                as i16
        };
        wave.extend_from_slice(&sample.to_le_bytes());
    }
    wave
}

fn godot_fixture(data: &std::path::Path) {
    use soe_bsa::{write_bsa, BsaFile, BsaVersion};
    use soe_esm::{FileHeader, Plugin};

    std::fs::create_dir_all(data).expect("fixture data directory is created");
    std::fs::write(
        data.parent().expect("fixture has a parent").join("Skyrim_Default.ini"),
        "[General]\nsLanguage=ENGLISH\n[Archive]\nsResourceArchiveList=Skyrim - Voices_en0.bsa\n",
    )
    .expect("fixture profile selects English voices");
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
        .expect("empty master fixture is written");
    }
    let audio = soe_audio::fuz::write(1, &[], &voice_wave());
    let archive = write_bsa(
        &[
            BsaFile {
                path: "sound/voice/dialogue.esm/voicehuman/first_00000a00_3.fuz",
                bytes: &audio,
            },
            BsaFile {
                path: "sound/voice/dialogue.esm/voicehuman/second_00000b00_3.fuz",
                bytes: &audio,
            },
        ],
        BsaVersion::SkyrimSe,
    )
    .expect("voice archive fixture is encoded");
    std::fs::write(data.join("Skyrim - Voices_en0.bsa"), archive)
        .expect("voice archive fixture is written");
}

fn drain_pipe<R: std::io::Read + Send + 'static>(mut pipe: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    })
}

#[test]
fn tst_17_godot_child_deadline_fits_the_120_second_check() {
    assert!(RUST_CHECK.contains("const GODOT_TIMEOUT: Duration = Duration::from_secs(110);"));
    assert!(RUST_CHECK.contains("let deadline = Instant::now() + timeout;"));
}

#[test]
fn tst_17_timeout_kills_and_reaps_the_godot_child() {
    assert!(RUST_CHECK.contains("timed_out = true;\n            let _ = child.kill();\n            break child.wait()"));
}

#[test]
fn tst_17_both_pipes_are_drained_and_reported_after_timeout() {
    assert!(RUST_CHECK.contains(".stdout(Stdio::piped())"));
    assert!(RUST_CHECK.contains(".stderr(Stdio::piped())"));
    assert!(RUST_CHECK.contains("stdout.join()"));
    assert!(RUST_CHECK.contains("stderr.join()"));
    assert!(RUST_CHECK.contains("stdout:\\n{}\\nstderr:\\n{}"));
}

#[test]
fn tst_17_godot_script_has_watchdog_and_nonzero_failure_exit() {
    assert!(GODOT_SCRIPT.contains("await create_timer(90.0).timeout"));
    assert!(GODOT_SCRIPT.contains("_finish(1)"));
    assert!(GODOT_SCRIPT.contains("quit(exit_code)"));
}

#[test]
fn tst_17_capture_reads_only_available_audio_frames() {
    let available = GODOT_SCRIPT
        .find("var frames := capture.get_frames_available()")
        .expect("capture checks available frames");
    let empty_guard = GODOT_SCRIPT[available..]
        .find("if frames == 0:")
        .expect("capture handles an empty ring buffer");
    let read = GODOT_SCRIPT[available..]
        .find("capture.get_buffer(frames)")
        .expect("capture reads the checked frame count");
    assert!(empty_guard < read);
}

#[test]
fn tst_17_godot_mixer_script_finishes_within_120_seconds() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let godot = std::env::var_os("SOE_GODOT_BIN")
        .expect("SOE_GODOT_BIN points to the installed Godot runtime");
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = root.join("godot");
    let fixture = tempfile::tempdir().expect("temporary fixture directory is created");
    let data = fixture.path().join("Data");
    godot_fixture(&data);
    let mut child = Command::new(godot)
        .arg("--headless")
        .args(["--path"])
        .arg(&project)
        .args(["--script"])
        .arg(project.join("dlg_11_audio_test.gd"))
        .args(["--resolution", "640x360"])
        .env("DLG_11_TEST_DATA", &data)
        .env(
            "DLG_11_FIRST_VOICE",
            "Sound/Voice/Dialogue.esm/VoiceHuman/First_00000A00_3.fuz",
        )
        .env(
            "DLG_11_SECOND_VOICE",
            "Sound/Voice/Dialogue.esm/VoiceHuman/Second_00000B00_3.fuz",
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Godot process starts");
    let stdout = drain_pipe(child.stdout.take().expect("stdout is piped"));
    let stderr = drain_pipe(child.stderr.take().expect("stderr is piped"));
    let deadline = Instant::now() + Duration::from_secs(115);
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
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout_bytes = stdout.join().expect("stdout reader finishes");
    let stderr_bytes = stderr.join().expect("stderr reader finishes");
    let stdout = String::from_utf8_lossy(&stdout_bytes);
    let stderr = String::from_utf8_lossy(&stderr_bytes);
    assert!(
        !timed_out && status.success() && stdout.contains("RESULT: PASS"),
        "Godot mixer failed or exceeded 115 seconds\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
