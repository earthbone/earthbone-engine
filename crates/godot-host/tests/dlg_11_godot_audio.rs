// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Runs the dialogue voice path in Godot and captures samples from its mixer.
//! Set `SOE_GODOT_BIN` to enable this test, which builds the release extension.

use soe_audio::{fuz, SoundSource};
use soe_bsa::{write_bsa, BsaFile, BsaVersion, Vfs};
use soe_esm::{FileHeader, Plugin};
use soe_godot_host::data_source::vfs_from_loadout;
use soe_profile::LoadoutInput;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const FIRST: &str = "Sound/Voice/Dialogue.esm/VoiceHuman/First_00000A00_3.fuz";
const SECOND: &str = "Sound/Voice/Dialogue.esm/VoiceHuman/Second_00000B00_3.fuz";
static GODOT_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
const GODOT_TIMEOUT: Duration = Duration::from_secs(110);

struct BoundedOutput {
    output: Output,
    timed_out: bool,
}

fn output_reader<R: Read + Send + 'static>(mut stream: R) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stream.read_to_end(&mut bytes);
        bytes
    })
}

fn run_bounded(mut child: Child, timeout: Duration) -> BoundedOutput {
    let stdout = output_reader(child.stdout.take().expect("stdout is piped"));
    let stderr = output_reader(child.stderr.take().expect("stderr is piped"));
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("Godot process status is readable") {
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
            stdout: stdout.join().expect("stdout reader completes after exit"),
            stderr: stderr.join().expect("stderr reader completes after exit"),
        },
        timed_out,
    }
}

fn failure_message(output: &BoundedOutput) -> String {
    format!(
        "Godot dialogue audio test {}\nstdout:\n{}\nstderr:\n{}",
        if output.timed_out {
            "exceeded its wall-clock limit and was killed"
        } else {
            "failed"
        },
        String::from_utf8_lossy(&output.output.stdout),
        String::from_utf8_lossy(&output.output.stderr)
    )
}

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
        // Voice assets can begin with silence longer than a mixer buffer.
        let sample = if frame < frames / 4 {
            0
        } else {
            ((frame as f32 * 440.0 * std::f32::consts::TAU / sample_rate as f32).sin() * 12_000.0)
                as i16
        };
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn write_fixture_data(data: &Path, voices: Option<(Vec<u8>, Vec<u8>)>) {
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
    let (first, second) = voices.unwrap_or_else(|| {
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
fn installed_voice_pair(data: &Path) -> Option<(Vec<u8>, Vec<u8>)> {
    use soe_audio::wma::Native;
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
    let preferred = names
        .iter()
        .find(|name| name.contains("maleelfhaughty/voicepowers_shout01b"));
    for name in preferred
        .into_iter()
        .chain(names.iter().step_by(stride).take(60))
    {
        let Ok(voice) = vfs.read(name) else {
            continue;
        };
        if !matches!(soe_audio::Fuz::split(&voice), Ok(fuz) if fuz.audio_kind() == soe_audio::AudioKind::Xwma)
        {
            continue;
        }
        let Ok(sound) = source.read(name) else {
            continue;
        };
        if sound.pcm.peak() > 0.01 {
            return Some((voice.clone(), voice));
        }
    }
    None
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

fn run_godot_audio_test(voices: Option<(Vec<u8>, Vec<u8>)>) {
    let _lock = GODOT_TEST_LOCK.lock().unwrap();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let Some(godot) = godot_binary() else {
        eprintln!("skipping: set SOE_GODOT_BIN to a Godot 4.4 or newer executable");
        return;
    };

    let fixture = tempfile::tempdir().unwrap();
    let data = fixture.path().join("Data");
    write_fixture_data(&data, voices);
    let script = project.join("dlg_11_audio_test.gd");
    let child = Command::new(godot)
        .arg("--headless")
        .args(["--path"])
        .arg(&project)
        .args(["--script"])
        .arg(script)
        .args(["--resolution", "640x360"])
        .env("DLG_11_TEST_DATA", &data)
        .env("DLG_11_FIRST_VOICE", FIRST)
        .env("DLG_11_SECOND_VOICE", SECOND)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Godot process starts");
    let output = run_bounded(child, GODOT_TIMEOUT);
    assert!(
        !output.timed_out
            && output.output.status.success()
            && String::from_utf8_lossy(&output.output.stdout).contains("RESULT: PASS"),
        "{}",
        failure_message(&output)
    );
}

#[test]
fn dlg_11_voice_archive_fixture_resolves_and_decodes_both_responses() {
    let fixture = tempfile::tempdir().unwrap();
    let data = fixture.path().join("Data");
    write_fixture_data(&data, None);
    let loadout = LoadoutInput::vanilla(fixture.path(), None, None)
        .resolve()
        .unwrap();
    let vfs = vfs_from_loadout(&loadout).unwrap();
    let source = SoundSource::new(&vfs);
    for path in [FIRST, SECOND] {
        let sound = source.read(path).unwrap();
        assert_eq!(sound.pcm.sample_rate, 22_050);
        assert!(sound.pcm.frames() > 20_000);
        assert!(sound.pcm.peak() > 0.1);
    }
}

#[test]
fn tst_17_dlg_11_godot_mixer_plays_and_interrupts_voice_at_speaker() {
    run_godot_audio_test(None);
}

#[test]
fn tst_17_godot_script_quits_on_missing_fixture() {
    let _lock = GODOT_TEST_LOCK.lock().unwrap();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let Some(godot) = godot_binary() else {
        eprintln!("skipping: set SOE_GODOT_BIN to a Godot 4.4 or newer executable");
        return;
    };
    let child = Command::new(godot)
        .arg("--headless")
        .args(["--path"])
        .arg(&project)
        .args(["--script"])
        .arg(project.join("dlg_11_audio_test.gd"))
        .args(["--resolution", "640x360"])
        .env("DLG_11_TEST_DATA", "")
        .env("DLG_11_FIRST_VOICE", "")
        .env("DLG_11_SECOND_VOICE", "")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Godot process starts");
    let output = run_bounded(child, GODOT_TIMEOUT);

    assert!(
        !output.timed_out
            && !output.output.status.success()
            && String::from_utf8_lossy(&output.output.stderr)
                .contains("dialogue voice fixtures were not supplied"),
        "{}",
        failure_message(&output)
    );
}

#[test]
fn dlg_11_installed_xwma_voice_plays_in_headless_godot_mixer() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is unset");
        return;
    };
    let voices = installed_voice_pair(&data)
        .expect("installed Skyrim voice archive must provide a decodable xWMA voice");
    run_godot_audio_test(Some(voices));
}

#[test]
#[cfg(unix)]
fn tst_17_godot_child_timeout_kills_process_and_keeps_captured_output() {
    let child = Command::new("sh")
        .args(["-c", "printf 'child started\\n'; exec sleep 5"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("test child starts");
    let result = run_bounded(child, Duration::from_millis(100));

    assert!(result.timed_out, "child should be stopped at its deadline");
    assert!(
        String::from_utf8_lossy(&result.output.stdout).contains("child started"),
        "{}",
        failure_message(&result)
    );
    assert!(failure_message(&result).contains("child started"));
}
