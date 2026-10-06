// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]

//! Independently checks each audio path reported by the installed-data window run.

#[path = "common/aud_11.rs"]
mod aud_11;

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
    if report_value(report, "voices").parse::<u64>().is_err() {
        failures.push("voice count is invalid".to_owned());
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
