// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Stock Godot shader validation in a standalone project with no installed world.

#[test]
fn water_shader_compiles_on_stock_godot_forward_renderer() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("SOE_GODOT_BIN unset, skipping live water shader check");
        return;
    };
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("project.godot"),
        "config_version=5\n[rendering]\nrenderer/rendering_method=\"forward_plus\"\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("water.gdshader"),
        include_str!("../src/water.gdshader"),
    )
    .unwrap();
    std::fs::write(
        project.path().join("check.gd"),
        include_str!("water_shader_check.gd"),
    )
    .unwrap();
    let output = std::process::Command::new(godot)
        .env("DISPLAY", ":99")
        .args(["--headless", "--rendering-method", "forward_plus", "--path"])
        .arg(project.path())
        .args(["--script", "check.gd"])
        .output()
        .expect("start stock Godot shader check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("water shader: PASS")
            && !stderr.contains("SHADER ERROR")
            && !stderr.contains("SCRIPT ERROR")
            && !stderr.contains("Shader compilation failed"),
        "{stdout}\n{stderr}"
    );
    eprintln!("{stdout}");
}
