#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::process::{Command, Output};

use tempfile::tempdir;

fn run_route(route: &str, output_dir: &std::path::Path) -> Output {
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN must name Godot");
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let route_path = output_dir.join("route.json");
    std::fs::write(&route_path, route).expect("write route fixture");
    Command::new(godot)
        .args(["--path"])
        .arg(project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "ply_04_route_test.gd",
        ])
        .env(
            "SKYRIM_DATA",
            std::env::var_os("SKYRIM_DATA").expect("SKYRIM_DATA"),
        )
        .env("SOE_PLY_04_ROUTE", route_path)
        .env("SOE_PLY_04_OUTPUT_DIR", output_dir)
        .output()
        .expect("launch Godot route")
}

#[test]
fn ply_04_rejects_checkpoint_paths_that_escape_capture_directory() {
    let output = tempdir().expect("temporary output directory");
    let result = run_route(
        r#"{"name":"path-check","start":{"cell":"WhiterunBanneredMare","position":[0,0,0]},"steps":[{"command":{"action":"wait"},"frames":1,"checkpoint":"../outside"}]}"#,
        output.path(),
    );
    assert_eq!(result.status.code(), Some(1));
    let log = std::fs::read_to_string(output.path().join("frame_times.json")).unwrap();
    assert!(log.contains("\"route_result\": \"fail\""), "{log}");
    assert!(!output.path().parent().unwrap().join("outside.png").exists());
}

#[test]
fn ply_04_windowed_single_checkpoint_writes_capture_and_frame_result() {
    let output = tempdir().expect("temporary output directory");
    let result = run_route(
        r#"{"name":"single-checkpoint","start":{"cell":"WhiterunBanneredMare","position":[0,0,0]},"steps":[{"command":{"action":"wait"},"frames":1,"checkpoint":"one"}]}"#,
        output.path(),
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let log = std::fs::read_to_string(output.path().join("frame_times.json")).unwrap();
    assert!(log.contains("\"route_result\": \"pass\""), "{log}");
    assert_eq!(log.matches("\"worst_frame_ms\"").count(), 1, "{log}");
    let capture = std::fs::read(output.path().join("one.png")).unwrap();
    assert!(capture.starts_with(b"\x89PNG\r\n\x1a\n"));
}
