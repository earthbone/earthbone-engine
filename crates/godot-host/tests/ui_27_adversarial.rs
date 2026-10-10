// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! UI-27 adversarial checks for default FFDec discovery and menu fallback.

use std::path::{Path, PathBuf};
use std::process::Command;

fn data_path() -> PathBuf {
    std::env::var_os("SKYRIM_DATA").map_or_else(
        || {
            PathBuf::from(
                "/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
            )
        },
        PathBuf::from,
    )
}

fn run_container_scene(label: &str) -> String {
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN is required");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(godot)
        .arg("--path")
        .arg(workspace.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "ui_27_container_panel_test.gd",
        ])
        .env("SKYRIM_DATA", data_path())
        .env("SOE_LOG", "info")
        .env("SOE_UI_27_TOOLS", "")
        .env("SOE_UI_27_CAPTURE", format!("/tmp/ui-27-{label}.png"))
        .env("SOE_UI_27_BUNDLED", "0")
        .env("SOE_INTERFACE_CACHE_ROOT", format!("/tmp/ui-27-cache-{label}"))
        .env_remove("SOE_FFDEC_JAR")
        .env_remove("SOE_FFDEC")
        .env_remove("SOE_TOOLS_PATH")
        .env_remove("SOE_INTERFACE_CACHE")
        .output()
        .expect("Godot scene should start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("UI-27 PASS") && !stderr.contains("SCRIPT ERROR"),
        "{label}: status={}\n{stdout}\n{stderr}",
        output.status
    );
    format!("{stdout}\n{stderr}")
}

#[test]
fn ui_27_discovers_local_ffdec_without_environment_or_project_setup() {
    let log = run_container_scene("unconfigured");
    assert!(
        log.contains("interface art: FFDec") && log.contains("ffdec-cli.jar"),
        "default discovery did not find the local FFDec installation: {log}"
    );
}

#[test]
fn ui_27_missing_ffdec_keeps_the_reopened_container_panel_bounded() {
    let log = run_container_scene("no-ffdec");
    assert!(
        log.contains("FFDec unavailable, using bundled menu theme"),
        "missing FFDec did not select the bundled theme: {log}"
    );
    let bounds: Vec<_> = log
        .lines()
        .filter(|line| line.starts_with("UI-27 PANEL "))
        .collect();
    assert_eq!(bounds.len(), 1, "container scene did not report its final panel");
    assert!(
        bounds[0].contains("P: (25.6, 28.8)")
            && bounds[0].contains("S: (588.8, 302.4)"),
        "reopened panel does not match the SkyUI layout: {}",
        bounds[0]
    );
}
