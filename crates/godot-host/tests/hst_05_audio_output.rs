// SPDX-License-Identifier: GPL-3.0-or-later

//! Runs the HST-05 Godot script against a short vanilla effect in Skyrim's BSA.
//! The dummy driver verifies Godot and SoundBank routing, not physical output.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_bsa::Vfs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SOUND_FRAGMENT: &str = "sound/fx/fst/dirt/jumpdown/fst_dirt_jumpdown_01.wav";

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

fn vanilla_sound(data: &Path) -> Option<String> {
    let mut vfs = Vfs::new();
    vfs.add_archive_path(&data.join("Skyrim - Sounds.bsa"))
        .ok()?;
    let mut matches: Vec<_> = vfs
        .paths()
        .filter(|path| path.eq_ignore_ascii_case(SOUND_FRAGMENT))
        .collect();
    matches.sort_unstable();
    matches.first().map(|path| (*path).to_owned())
}

#[test]
fn hst_05_dummy_driver_lists_devices_and_plays_vanilla_soundbank_effect() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let Some(sound) = vanilla_sound(&data) else {
        eprintln!("skipping: the selected vanilla sound is missing from Skyrim - Sounds.bsa");
        return;
    };
    let Some(godot) = godot_binary() else {
        eprintln!("skipping: set SOE_GODOT_BIN to a Godot 4.4 or newer executable");
        return;
    };

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"));
    let extension = target.join("release/libsoe_godot_host.so");
    if !extension.exists() {
        eprintln!(
            "skipping: build the release extension at {} first",
            extension.display()
        );
        return;
    }

    let output = Command::new(godot)
        .args(["--headless", "--audio-driver", "Dummy", "--path"])
        .arg(project)
        .args(["--script"])
        .arg(workspace.join("godot/hst_05_audio_output.gd"))
        .args(["--", "Default"])
        .arg(&sound)
        .env("SKYRIM_DATA", &data)
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "HST-05 Godot script failed with {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    assert!(
        stdout.contains("AUDIO_DEVICES count="),
        "{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("AUDIO_SELECTED device=Default"),
        "{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("AUDIO_DEVICE name="),
        "Godot returned no output device entry\n{stdout}\n{stderr}"
    );
    for played in 1..=3 {
        let marker = format!("AUDIO_CHECK device=Default sound={sound} played={played}");
        assert!(
            stdout.contains(&marker),
            "missing {marker}\n{stdout}\n{stderr}"
        );
    }
}
