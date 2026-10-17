// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed PRF-40 traversals use the ordinary walkthrough and stock Godot.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

#[test]
fn prf_40_installed_exterior_uploads_and_physics_are_bounded() {
    let (Some(data), Some(godot)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        return;
    };
    let root = root();
    let directory = root.join("target/prf40-after");
    std::fs::create_dir_all(&directory).unwrap();
    for cell in ["WhiterunExterior04", "WhiterunExterior01"] {
        let report = directory.join(format!("{cell}.json"));
        let stdout = directory.join(format!("{cell}.log"));
        let stderr = directory.join(format!("{cell}.err"));
        let output = std::fs::File::create(&stdout).unwrap();
        let errors = std::fs::File::create(&stderr).unwrap();
        let status = Command::new(&godot)
            .args(["--path"])
            .arg(root.join("godot"))
            .args([
                "--resolution",
                "640x360",
                "--script",
                "prf_40_exterior_test.gd",
            ])
            .env("SKYRIM_DATA", &data)
            .env("SOE_PRF40_CELL", cell)
            .env("SOE_PRF40_REPORT", &report)
            .env("SOE_GODOT_TIMEOUT", "900")
            .env("DISPLAY", ":99")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("SOE_THREADED_SIMULATION")
            .env_remove("SOE_CAPTURE_PATH")
            .env_remove("SOE_AUD_11_TEST")
            .stdin(Stdio::null())
            .stdout(output)
            .stderr(errors)
            .status()
            .unwrap();
        let log = std::fs::read_to_string(&stdout).unwrap();
        let errors = std::fs::read_to_string(&stderr).unwrap();
        assert!(
            status.success(),
            "{cell}: {status}, see {} and {}",
            stdout.display(),
            stderr.display()
        );
        assert!(
            !errors.contains("SCRIPT ERROR:"),
            "{cell}: script failed, see {}",
            stderr.display()
        );
        assert!(log.contains("prf_40_installed_exterior: PASS"));
        assert!(log.contains("WALKTHROUGH stopped: ready=true"));
        let row: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
        assert_eq!(row["frames"].as_array().unwrap().len(), 360);
        assert!(
            row["max_physics_steps"].as_u64().unwrap() <= 1,
            "{cell}: {}",
            row["max_physics_steps"]
        );
        assert!(
            row["upload"]["slices"].as_u64().unwrap() > 1,
            "{cell}: publication did not yield"
        );
        assert!(
            row["upload"]["worst_ms"].as_f64().unwrap() < 33.0,
            "{cell}: upload breached 33 ms: {}",
            row["upload"]
        );
        assert_eq!(
            row["pipelines"]["draw"].as_u64().unwrap(),
            0,
            "{cell}: foreground draw compilation"
        );
        assert!(
            row["horizontal_distance"].as_f64().unwrap() > 128.0,
            "{cell}: no exterior traversal"
        );
        assert_ne!(
            row["start_cell"], row["end_cell"],
            "{cell}: no boundary crossing"
        );
        println!("PRF40 {cell}: streaming p50={} p99={} max={}, upload={}, physics_steps={}, pipelines={}", row["streaming_ms_p50"], row["streaming_ms_p99"], row["streaming_ms_max"], row["upload"], row["max_physics_steps"], row["pipelines"]);
    }
}
