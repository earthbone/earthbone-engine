// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::{Command, Output};

fn assert_godot_passed(output: &Output, marker: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains(marker), "{stdout}\n{stderr}");
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}

#[test]
fn anm_59_vanilla_npc_override_changes_rendered_head_geometry() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("skipping: SOE_GODOT_BIN is not set");
        return;
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = root.join("godot/anm_59_vanilla_npc_render_test.gd");
    let output = Command::new(godot)
        .current_dir(&root)
        .env("SKYRIM_DATA", data)
        .args(["--path", "godot", "--resolution", "640x360", "--script"])
        .arg(script)
        .output();
    let Ok(output) = output else {
        panic!("Godot could not be started");
    };
    assert_godot_passed(&output, "PASS ANM-59 rendered vanilla NPC head");
}
