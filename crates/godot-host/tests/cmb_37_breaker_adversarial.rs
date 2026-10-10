// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn cmb_37_installed_destruction_stage_creates_host_model_instances() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping CMB-37 host model instance check");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("SOE_GODOT_BIN not set, skipping CMB-37 host model instance check");
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let extension = workspace.join("target/release/libsoe_godot_host.so");
    let build = Command::new(env!("CARGO"))
        .current_dir(&workspace)
        .args([
            "build",
            "-p",
            "soe-godot-host",
            "--release",
            "--features",
            "cmb-37-test",
            "--target-dir",
        ])
        .arg(workspace.join("target"))
        .output()
        .expect("release extension build starts");
    assert!(
        build.status.success(),
        "release extension build failed\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(extension.exists(), "release GDExtension was not built");
    let import = Command::new(&godot)
        .args(["--headless", "--path"])
        .arg(workspace.join("godot"))
        .arg("--import")
        .output()
        .expect("Godot project import starts");
    assert!(
        import.status.success(),
        "Godot project import failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&import.stdout),
        String::from_utf8_lossy(&import.stderr)
    );
    let output = Command::new(godot)
        .args(["--path"])
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360", "--script"])
        .arg("cmb_37_destruction_render_test.gd")
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot starts the installed-data destruction check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("RESULT: PASS CMB-37 destruction render")
            && stdout.contains("CMB_37_RENDER")
            && stdout.contains("instances="),
        "CMB-37 model replacement failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
