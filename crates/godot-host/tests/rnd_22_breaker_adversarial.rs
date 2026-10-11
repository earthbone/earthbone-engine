// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Source-backed edge checks for RND-22 effect rendering.

use std::{path::PathBuf, process::Command};

use soe_godot_host::{
    mesh::{EffectMaterial, MaterialDesc},
    nif_effect,
};
use soe_nif::{blocks::flags1, mesh::EffectParams};

#[test]
fn rnd_22_effect_uv_transform_applies_authored_scale_and_offset() {
    let material = MaterialDesc {
        shader_flags_1: flags1::ZBUFFER_TEST,
        ..Default::default()
    };
    let effect = EffectMaterial {
        params: EffectParams::default(),
        blend: None,
        alpha_test: None,
        uv_offset: [0.125, 0.25],
        uv_scale: [2.0, 0.5],
        controllers: Vec::new(),
    };
    let shader = nif_effect::shader(&material, &effect);

    let scale = shader
        .find("vec2 centered = UV * animated_scale - vec2(0.5);")
        .unwrap();
    let rotation = shader[scale..]
        .find("mat2(vec2(c, s), vec2(-s, c))")
        .map(|relative| scale + relative)
        .unwrap();
    let offset = shader[rotation..]
        .find("+ animated_offset;")
        .map(|relative| rotation + relative)
        .unwrap();
    let sample = shader.find("texture(source_texture, uv)").unwrap();
    assert!(scale < rotation && rotation < offset && offset < sample);
}

#[test]
fn rnd_22_installed_effect_capture_renders_mist_and_window() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = workspace.join("godot");
    assert!(workspace
        .join("target/release/libsoe_godot_host.so")
        .is_file());

    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "rnd_22_effect_material_test.gd",
            "--",
            "--vanilla-only",
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot capture process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("rnd_22_effect_material_capture: PASS"),
        "Godot installed effect capture failed\n{stdout}\n{stderr}"
    );
    for name in ["mist", "window"] {
        assert!(stdout.contains(&format!("rnd_22_vanilla {name} pixels=")));
        assert!(project.join(format!(".godot/rnd_22-{name}.png")).is_file());
    }
}
