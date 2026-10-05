// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! Installed host checks build a current release extension before running scripts.
//! Project registration follows the shared isolated Godot check helper.

#[path = "isolated_project.rs"]
mod isolated_project;

pub fn project(scripts: &[&str]) -> tempfile::TempDir {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = std::env::current_exe().expect("test executable path");
    let target = executable
        .parent()
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::parent)
        .expect("Cargo target directory");
    // CONTEXT: Debug gameplay startup can exceed the automated Godot time limit.
    let build = std::process::Command::new(env!("CARGO"))
        .current_dir(&workspace)
        .args(["build", "-p", "soe-godot-host", "--release", "--target-dir"])
        .arg(target)
        .output()
        .expect("release extension build starts");
    assert!(
        build.status.success(),
        "release extension build failed\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let mut scripts = scripts.to_vec();
    if scripts.contains(&"walkthrough.gd") {
        for dependency in [
            "record_sky.gd",
            "record_sky.gdshader",
            "record_fog.gdshader",
        ] {
            if !scripts.contains(&dependency) {
                scripts.push(dependency);
            }
        }
    }
    isolated_project::project(&scripts, Some(&target.join("release/libsoe_godot_host.so")))
}
