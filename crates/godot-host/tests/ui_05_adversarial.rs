// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../game/tests/support/ui_05_fixture.rs"]
mod compiled;

#[test]
fn ui_05_adversarial_host_shows_pages_from_the_live_script() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("UI-05 host adversarial check skipped: SOE_GODOT_BIN is unset");
        return;
    };
    let Some(fixture) = compiled::load() else {
        eprintln!("UI-05 host adversarial check skipped: SkyUI PEX input is unset");
        return;
    };
    let directory =
        std::env::temp_dir().join(format!("soe-ui05-adversarial-{}", std::process::id()));
    fixture.write(&directory);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
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
