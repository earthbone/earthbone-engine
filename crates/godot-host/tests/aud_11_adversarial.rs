// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Independently checks each audio path reported by the installed-data window run.

use std::path::PathBuf;
use std::process::Command;

fn report_value<'a>(report: &'a str, key: &str) -> &'a str {
    let marker = format!("{key}=");
    let value = report
        .split_once(&marker)
        .unwrap_or_else(|| panic!("missing {key} in {report}"))
        .1;
    value.split_whitespace().next().unwrap_or("")
}

#[test]
fn aud_11_window_run_reports_each_nonzero_audio_path_and_full_duration() {
    let data = std::env::var_os("SKYRIM_DATA")
        .map(PathBuf::from)
        .expect("SKYRIM_DATA is required for the installed-data check");
    let godot = std::env::var_os("SOE_GODOT_BIN")
        .map(PathBuf::from)
        .expect("SOE_GODOT_BIN is required for the windowed check");
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"));
    let extension = target.join("release/libsoe_godot_host.so");
    assert!(
        extension.exists(),
        "missing release extension: {}",
        extension.display()
    );

    let output = Command::new(godot)
        .args(["--path"])
        .arg(&project)
        .args(["--windowed", "--resolution", "640x360", "--script"])
        .arg(workspace.join("godot/walkthrough.gd"))
        .env("SKYRIM_DATA", data)
        .env("SOE_AUD_11_TEST", "1")
        .output()
        .expect("Godot walkthrough process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report = stdout
        .lines()
        .find(|line| line.contains("AUD-11 systems"))
        .unwrap_or_else(|| {
            panic!(
                "walkthrough did not emit its per-system report, status {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                output.status
            )
        });
    let mut failures = Vec::new();
    for key in ["footsteps", "ambience", "music", "voices"] {
        match report_value(report, key).parse::<u64>() {
            Ok(count) if count > 0 => {}
            Ok(count) => failures.push(format!("{key} started {count} times")),
            Err(_) => failures.push(format!("{key} count is invalid")),
        }
    }
    match report_value(report, "master_peak_db").parse::<f32>() {
        Ok(peak_db) if peak_db > -60.0 => {}
        Ok(peak_db) => failures.push(format!("master peak was {peak_db} dB")),
        Err(_) => failures.push("master peak is invalid".to_owned()),
    }
    match report_value(report, "walk_seconds").parse::<f32>() {
        Ok(seconds) if seconds >= 19.9 => {}
        Ok(seconds) => failures.push(format!("walk lasted {seconds} seconds")),
        Err(_) => failures.push("walk duration is invalid".to_owned()),
    }
    match report_value(report, "walk_distance").parse::<f32>() {
        Ok(distance) if distance > 10.0 => {}
        Ok(distance) => failures.push(format!("walk moved {distance} units")),
        Err(_) => failures.push("walk distance is invalid".to_owned()),
    }
    if !output.status.success() || !stdout.contains("AUD-11 RESULT: PASS") {
        failures.push(format!("walkthrough exited {}", output.status));
    }
    assert!(
        failures.is_empty(),
        "{}; report: {report}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        failures.join(", ")
    );
}
