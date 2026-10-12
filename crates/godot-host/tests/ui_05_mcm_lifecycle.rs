// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Windowed lifecycle and page presentation through the production PlayerNode.

#[path = "../../game/tests/support/ui_05_fixture.rs"]
mod compiled;

#[test]
fn ui_05_windowed_unmodified_scripts_render_and_select_declared_pages() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("UI-05 host check skipped: SOE_GODOT_BIN is unset");
        return;
    };
    let Some(fixture) = compiled::load() else {
        return;
    };
    let game = fixture.game();
    assert!(
        !game
            .vm
            .log
            .messages()
            .iter()
            .any(|message| message.starts_with("error:")),
        "{:?}",
        game.vm.log.messages()
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = root.join("target/ui_05_host");
    fixture.write(&directory);
    let output = std::process::Command::new(godot)
        .args([
            "--path",
            "godot",
            "--resolution",
            "640x360",
            "--script",
            "res://ui_05_mcm_lifecycle_test.gd",
        ])
        .current_dir(&root)
        .env("SOE_UI_05_FIXTURE_DIR", &directory)
        .output()
        .unwrap();
    std::fs::write(directory.join("stdout.txt"), &output.stdout).unwrap();
    std::fs::write(directory.join("stderr.txt"), &output.stderr).unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{}\n{stdout}\n{stderr}",
        output.status
    );
    assert!(
        stdout.contains("PASS ui_05 windowed MCM lifecycle"),
        "{stdout}\n{stderr}"
    );
}
