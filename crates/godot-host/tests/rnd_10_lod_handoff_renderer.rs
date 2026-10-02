// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Runs the RND-10 terrain and object LOD handoff check in Godot with the
//! actual RenderingServer. The script verifies near and far instances exist
//! and each real resident LOD block is disjoint from the streamed near cells.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn rnd_10_renderer_terrain_and_object_lod_do_not_double_draw_near_cells() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let extension = workspace.join("target/release/libsoe_godot_host.so");
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping the live RND-10 renderer check");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("SOE_GODOT_BIN not set, skipping the live RND-10 renderer check");
        return;
    };
    if !extension.exists() {
        eprintln!(
            "release GDExtension missing at {}, skipping live renderer check",
            extension.display()
        );
        return;
    }

    let script = project.join("rnd_10_lod_handoff_check.gd");
    let output = Command::new(godot)
        .args(["--path"])
        .arg(&project)
        .args(["--script"])
        .arg(script)
        .args(["--resolution", "640x360"])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("RESULT: PASS: terrain and object LOD hand off beyond full-resolution cells without double drawing"),
        "Godot RND-10 renderer check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    println!("{stdout}");
}
