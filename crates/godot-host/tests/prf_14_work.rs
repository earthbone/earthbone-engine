// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed host counter checks, after the extension build and import.

#[test]
fn prf_14_godot_counts_live_render_instances_and_resets_work_windows() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return;
    }
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // CONTEXT: Cargo test does not refresh the release library Godot loads.
    let build = std::process::Command::new(env!("CARGO"))
        .current_dir(&workspace)
        .args(["build", "-p", "soe-godot-host", "--release", "--target-dir"])
        .arg(workspace.join("target"))
        .output()
        .expect("release extension build starts");
    assert!(
        build.status.success(),
        "release extension build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let project = workspace.join("godot");
    // CONTEXT: A fresh checkout needs an import to register its GDExtension.
    let import = std::process::Command::new(&godot)
        .args(["--headless", "--path"])
        .arg(&project)
        .arg("--import")
        .output()
        .expect("Godot project import starts");
    assert!(
        import.status.success(),
        "Godot project import failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&import.stdout),
        String::from_utf8_lossy(&import.stderr)
    );
    let output = std::process::Command::new(godot)
        .args(["--headless", "--path"])
        .arg(project)
        .args(["--script", "prf_14_work_test.gd"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("RESULT: PASS"), "{stdout}\n{stderr}");
    assert!(stdout.contains("PRF_14_COUNTS"), "{stdout}");
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}
