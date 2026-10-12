// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[test]
fn prf_13_windowed_host_profile_covers_f4_audio_render_and_plugin_counters() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("prf-13-adversarial-{}", std::process::id()));
    let fixture = scratch.join("Data");
    std::fs::create_dir_all(&scratch).unwrap();
    std::fs::create_dir_all(&fixture).unwrap();
    let empty_master = soe_esm::Plugin::new(soe_esm::FileHeader::default()).to_bytes();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(fixture.join(name), &empty_master).unwrap();
    }
    let output = std::process::Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "prf_13_entities_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .env("SOE_PRF_13_FIXTURE", &fixture)
        .env("XDG_DATA_HOME", scratch.join("user"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(
        stdout.contains("prf_13_f4_input_presentation_and_session_export: PASS"),
        "{stdout}\n{stderr}"
    );
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
    std::fs::remove_dir_all(&scratch).unwrap();
}
