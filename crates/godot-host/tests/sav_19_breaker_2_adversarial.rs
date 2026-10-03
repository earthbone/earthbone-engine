// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Repeated restored host attachment must keep the same player and camera live.

use std::{path::PathBuf, process::Command};

#[test]
fn sav_19_repeated_attach_keeps_cell_camera_and_advances_each_success() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: set SOE_GODOT_BIN");
        return;
    };
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: set SKYRIM_DATA");
        return;
    };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let source = project.join("sav_19_host_attach_test.gd");
    let mut script = std::fs::read_to_string(source).unwrap();
    script = script
        .replace("var saved :=", "var saved: Dictionary =")
        .replace("var rejected :=", "var rejected: Dictionary =")
        .replace("var attached :=", "var attached: Dictionary =");
    script = script.replace(
        "var attached: Dictionary = destination.call(\"attach_restored_generation\", directory, 2)",
        "_check(_camera(destination) != null, \"destination camera missing before attach\")\n\tvar attached: Dictionary = destination.call(\"attach_restored_generation\", directory, 2)",
    );
    script = script.replace(
        "\"verified generation did not attach\"",
        "\"verified generation did not attach: \" + str(attached)",
    );
    script = script.replace(
        "await process_frame\n\tdestination.free()",
        r#"var days_after_first: float = destination.call("gameplay_days")
	var repeated: Dictionary = destination.call("attach_restored_generation", directory, 2)
	_check(repeated.get("ok", false), "second verified attach failed")
	_check(destination.call("gameplay_cell") == expected_cell, "second attach changed the cell")
	_check(destination.call("facing").distance_to(expected_facing) < 0.01,
		"second attach changed camera facing")
	var repeated_camera: Camera3D = _camera(destination)
	_check(repeated_camera != null and repeated_camera.is_current(), "second attach lost current camera")
	_check(destination.call("gameplay_days") == days_after_first and days_after_first > before_days,
		"second attach did not restore the stepped generation")
	await process_frame
	destination.free()"#,
    );
    let temp = tempfile::tempdir().unwrap();
    let script_path = temp.path().join("sav_19_repeated_attach.gd");
    std::fs::write(&script_path, script).unwrap();
    let save_dir = tempfile::tempdir().unwrap();
    let output = Command::new(godot)
        .args(["--path"])
        .arg(project)
        .args(["--script"])
        .arg(script_path)
        .args(["--resolution", "640x360"])
        .env("SKYRIM_DATA", data)
        .env("SAV_19_SAVE_DIR", save_dir.path())
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS sav_19 host attach"),
        "repeated SAV-19 host attach failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
