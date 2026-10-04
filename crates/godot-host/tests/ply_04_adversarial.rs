#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::process::{Command, Output};
use std::sync::Mutex;

use tempfile::tempdir;

static GODOT_LOCK: Mutex<()> = Mutex::new(());

fn run_route(route: &str, headless: bool, output_dir: &std::path::Path) -> Output {
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN must name Godot 4.4");
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let route_path = output_dir.join("route.json");
    std::fs::write(&route_path, route).expect("write route fixture");

    let mut command = Command::new(godot);
    command
        .arg("--path")
        .arg(project)
        .arg("--script")
        .arg("ply_04_route_test.gd");
    if headless {
        command.arg("--headless");
    } else {
        command.args(["--resolution", "640x360"]);
        command.env(
            "SKYRIM_DATA",
            std::env::var_os("SKYRIM_DATA").expect("SKYRIM_DATA must name installed game data"),
        );
    }
    command
        .env("SOE_PLY_04_ROUTE", route_path)
        .env("SOE_PLY_04_OUTPUT_DIR", output_dir)
        .output()
        .expect("launch Godot route")
}

#[test]
fn ply_04_windowed_route_captures_each_checkpoint_and_reports_worst_frames() {
    let _guard = GODOT_LOCK.lock().expect("Godot test lock");
    let output_dir = tempdir().expect("temporary output directory");
    let route = r#"{
        "name":"two-checkpoints",
        "start":{"cell":"WhiterunBanneredMare","position":[0,0,0]},
        "steps":[
            {"command":{"action":"wait"},"frames":2,"checkpoint":"first"},
            {"command":{"action":"wait"},"frames":3,"checkpoint":"second"}
        ]
    }"#;

    let result = run_route(route, false, output_dir.path());
    assert!(
        result.status.success(),
        "route failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let log = std::fs::read_to_string(output_dir.path().join("frame_times.json"))
        .expect("frame time log");
    assert!(log.contains("\"route_result\": \"pass\""), "{log}");
    assert_eq!(log.matches("\"worst_frame_ms\"").count(), 2, "{log}");
    for (name, frames) in [("first", "2"), ("second", "3")] {
        let capture =
            std::fs::read(output_dir.path().join(format!("{name}.png"))).expect("checkpoint PNG");
        assert!(
            capture.starts_with(b"\x89PNG\r\n\x1a\n"),
            "invalid PNG for {name}"
        );
        assert_eq!(&capture[16..24], &[0, 0, 2, 128, 0, 0, 1, 104]);
        assert!(log.contains(&format!("\"frames\": {frames}")), "{log}");
        assert!(
            log.contains(&format!("\"checkpoint\": \"{name}\"")),
            "{log}"
        );
    }
}

#[test]
fn ply_04_invalid_empty_route_fails_and_records_failure() {
    let _guard = GODOT_LOCK.lock().expect("Godot test lock");
    let output_dir = tempdir().expect("temporary output directory");
    let result = run_route(r#"{"name":"empty","steps":[]}"#, true, output_dir.path());

    assert_eq!(result.status.code(), Some(1));
    let log = std::fs::read_to_string(output_dir.path().join("frame_times.json"))
        .expect("failure frame time log");
    assert!(log.contains("\"route_result\": \"fail\""), "{log}");
    assert!(String::from_utf8_lossy(&result.stdout).contains("route_result=fail"));
}
