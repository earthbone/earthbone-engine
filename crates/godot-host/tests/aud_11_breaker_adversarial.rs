// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]

//! Checks that the walkthrough's ambience count tracks successful starts.

#[path = "common/aud_11.rs"]
mod aud_11;

use std::path::PathBuf;
use std::process::Command;

fn report_value<'a>(report: &'a str, key: &str) -> &'a str {
    report
        .split_once(&format!("{key}="))
        .unwrap_or_else(|| panic!("missing {key} in {report}"))
        .1
        .split_whitespace()
        .next()
        .unwrap_or("")
}

#[test]
fn aud_11_ambience_counter_ignores_missing_assets_and_counts_restarts() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: SOE_GODOT_BIN is not set");
        return;
    };
    let project = aud_11::project(&[]);

    let script_file = tempfile::Builder::new()
        .prefix("aud_11_breaker_")
        .suffix(".gd")
        .tempfile_in(project.path())
        .expect("temporary Godot test script is created");
    std::fs::write(
        script_file.path(),
        r#"extends SceneTree

func _initialize() -> void:
	call_deferred("run")

func run() -> void:
	var bank = ClassDB.instantiate("SoundBank")
	get_root().add_child(bank)
	await process_frame
	if not bank.call("open_data", OS.get_environment("SKYRIM_DATA")):
		printerr("could not open installed audio data: ", bank.call("last_error"))
		quit(1)
		return
	if bank.call("play_ambience", "sound/missing/aud_11.wav"):
		printerr("missing ambience unexpectedly started")
		quit(1)
		return
	if int(bank.call("ambience_start_count")) != 0:
		printerr("failed ambience changed the start count")
		quit(1)
		return
	var path := "sound/fx/fst/dirt/jumpdown/fst_dirt_jumpdown_01.wav"
	for expected in [1, 2]:
		if not bank.call("play_ambience", path):
			printerr("valid ambience failed: ", bank.call("last_error"))
			quit(1)
			return
		if int(bank.call("ambience_start_count")) != expected:
			printerr("ambience count did not match successful start ", expected)
			quit(1)
			return
	print("AUD-11 BREAKER PASS")
	quit(0)
"#,
    )
    .expect("temporary Godot script is written");

    let output = Command::new(godot)
        .args(["--headless", "--path"])
        .arg(project.path())
        .arg("--script")
        .arg(script_file.path())
        .env("SKYRIM_DATA", data)
        .env("XDG_DATA_HOME", project.path())
        .env("XDG_CACHE_HOME", project.path())
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("AUD-11 BREAKER PASS"),
        "ambience counter check failed with {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
}

#[test]
fn aud_11_window_run_has_all_four_audio_paths_and_mixer_signal() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: SOE_GODOT_BIN is not set");
        return;
    };
    let project = aud_11::project(&["walkthrough.gd", "render_warmup.gd", "system_overlay.gd"]);

    let output = Command::new(godot)
        .args(["--path"])
        .arg(project.path())
        .args(["--windowed", "--resolution", "640x360", "--script"])
        .arg(project.path().join("walkthrough.gd"))
        .env("SKYRIM_DATA", data)
        .env("XDG_DATA_HOME", project.path())
        .env("XDG_CACHE_HOME", project.path())
        .env("SOE_AUD_11_TEST", "1")
        .output()
        .expect("windowed Godot walkthrough starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report = stdout
        .lines()
        .find(|line| line.contains("AUD-11 systems"))
        .unwrap_or_else(|| panic!("missing audio report\nstdout:\n{stdout}\nstderr:\n{stderr}"));
    let counts = ["footsteps", "ambience", "music", "voices"]
        .map(|key| (key, report_value(report, key).parse::<u64>()));
    let peak = report_value(report, "master_peak_db").parse::<f32>();
    let seconds = report_value(report, "walk_seconds").parse::<f32>();
    let distance = report_value(report, "walk_distance").parse::<f32>();
    assert!(
        output.status.success()
            && stdout.contains("AUD-11 RESULT: PASS")
            && counts
                .iter()
                .all(|(_, count)| count.as_ref().is_ok_and(|count| *count > 0))
            && peak.is_ok_and(|peak| peak > -60.0)
            && seconds.is_ok_and(|seconds| seconds >= 19.9)
            && distance.is_ok_and(|distance| distance > 10.0),
        "windowed audio check failed\nreport: {report}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
