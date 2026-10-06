// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Adversarial PRF-08 check for an installed, visibly rendered exterior.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

fn godot_binary() -> Option<PathBuf> {
    std::env::var_os("SOE_GODOT_BIN")
        .or_else(|| std::env::var_os("SOE_GODOT"))
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

#[test]
fn windowed_whiterun_radius_one_produces_drawn_work_and_passes_33_ms() {
    if std::env::var_os("SKYRIM_DATA").is_none()
        || std::env::var("SOE_PRF_08_WINDOWED").as_deref() != Ok("1")
    {
        eprintln!("SKYRIM_DATA and SOE_PRF_08_WINDOWED=1 required, skipping windowed check");
        return;
    }
    let Some(godot) = godot_binary() else {
        eprintln!("SOE_GODOT_BIN, SOE_GODOT or godot required, skipping windowed check");
        return;
    };
    let root = workspace();
    let output = Command::new(godot)
        .args(["--path"])
        .arg(root.join("godot"))
        .args([
            "--windowed",
            "--resolution",
            "640x360",
            "--script",
            "bench.gd",
        ])
        .env("SOE_BENCH_CELL", "WhiterunOrigin")
        .env("SOE_BENCH_RADIUS", "1")
        .env("SOE_BENCH_BATCH", "1")
        .env("SOE_BENCH_OCCLUDERS", "1")
        .env("SOE_BENCH_SECONDS", "5")
        .env("SOE_BENCH_P99_MS", "16.7")
        .env("SOE_BENCH_WORST_MS", "33")
        .env("SOE_BENCH_SOAK", "0")
        .env("SOE_BENCH_CAPTURE", "")
        .env("XDG_DATA_HOME", root.join("target/prf-08-breaker-user"))
        .output()
        .expect("Godot windowed exterior benchmark starts");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let summary = log
        .lines()
        .filter(|line| {
            line.starts_with("godot_scene ")
                || line.starts_with("godot_display_driver ")
                || line.starts_with("godot_instances ")
                || line.starts_with("godot_run ")
                || line.starts_with("SOE_SAMPLE ")
                || line.starts_with("RESULT:")
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(log.contains("godot_display_driver X11 driver"), "{summary}");
    let sample = log
        .lines()
        .find(|line| line.starts_with("SOE_SAMPLE "))
        .expect("windowed benchmark emits a measurement row");
    assert!(!sample.contains("\"draw_calls\":0.0"), "{summary}");
    assert!(!sample.contains("\"primitives\":0.0"), "{summary}");
    let report = log
        .lines()
        .find(|line| line.starts_with("godot_run "))
        .expect("windowed benchmark reports its worst frame");
    let worst_ms = report
        .split_whitespace()
        .nth(3)
        .expect("worst frame value exists")
        .parse::<f64>()
        .expect("worst frame is numeric");
    assert!(worst_ms < 33.0, "{summary}");
    assert!(output.status.success(), "{summary}");
}
