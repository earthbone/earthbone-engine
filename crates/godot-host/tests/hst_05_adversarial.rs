// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::{Command, Output};

struct Harness {
    godot: PathBuf,
    project: PathBuf,
    script: PathBuf,
    data: PathBuf,
}

fn harness() -> Option<Harness> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    let godot = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from)?;
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let extension = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"))
        .join("release/libsoe_godot_host.so");
    if !extension.exists() {
        return None;
    }
    Some(Harness {
        godot,
        project: workspace.join("godot"),
        script: workspace.join("godot/hst_05_audio_output.gd"),
        data,
    })
}

impl Harness {
    fn run(&self, args: &[&str], unset_data: bool) -> Output {
        let mut command = Command::new(&self.godot);
        command
            .args(["--headless", "--audio-driver", "Dummy", "--path"])
            .arg(&self.project)
            .args(["--script"])
            .arg(&self.script)
            .arg("--")
            .args(args)
            .env("SKYRIM_DATA", &self.data);
        if unset_data {
            command.env_remove("SKYRIM_DATA");
        }
        command.output().expect("Godot process starts")
    }

    fn output_text(output: &Output) -> String {
        format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }
}

fn run_or_skip(args: &[&str], unset_data: bool) -> Option<(Output, Harness)> {
    let harness = harness()?;
    let output = harness.run(args, unset_data);
    Some((output, harness))
}

fn missing_env_skip() {
    eprintln!("skipping: set SKYRIM_DATA, SOE_GODOT_BIN and build the release extension");
}

#[test]
fn hst_05_named_nonexistent_device_fails_instead_of_claiming_playback() {
    let Some((output, _)) = run_or_skip(&["__soe_missing_output_device__"], false) else {
        missing_env_skip();
        return;
    };
    let text = Harness::output_text(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("audio output device was not found"), "{text}");
    assert!(!text.contains("AUDIO_CHECK device="), "{text}");
}

#[test]
fn hst_05_missing_sound_fails_without_an_audio_check_marker() {
    let Some((output, _)) = run_or_skip(&["Default", "no/such/sound.wav"], false) else {
        missing_env_skip();
        return;
    };
    let text = Harness::output_text(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains("SoundBank could not play no/such/sound.wav"),
        "{text}"
    );
    assert!(!text.contains("AUDIO_CHECK device="), "{text}");
}

#[test]
fn hst_05_missing_installed_data_fails_before_claiming_playback() {
    let Some((output, _)) = run_or_skip(&[], true) else {
        missing_env_skip();
        return;
    };
    let text = Harness::output_text(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("SKYRIM_DATA is not set"), "{text}");
    assert!(!text.contains("AUDIO_CHECK device="), "{text}");
}

#[test]
fn hst_05_default_output_runs_all_three_soundbank_starts() {
    let Some((output, harness)) = run_or_skip(&[], false) else {
        missing_env_skip();
        return;
    };
    let text = Harness::output_text(&output);
    assert!(output.status.success(), "{text}");
    assert!(text.contains("AUDIO_SELECTED device=Default"), "{text}");
    for played in 1..=3 {
        let marker = format!(
            "AUDIO_CHECK device=Default sound=sound/fx/fst/dirt/jumpdown/fst_dirt_jumpdown_01.wav played={played}"
        );
        assert!(text.contains(&marker), "missing {marker}\n{text}");
    }
    assert!(harness.data.join("Skyrim - Sounds.bsa").exists());
}
