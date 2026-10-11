// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! RND-38 regression for RND-33 through the playable walkthrough.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn authored_palettes(data: &Path) -> serde_json::Value {
    let loadout = soe_profile::LoadoutInput {
        game_dir: data.parent().expect("game directory").to_path_buf(),
        data_roots: vec![data.to_path_buf()],
        plugins_txt: None,
        loadorder_txt: None,
        ini_dir: None,
        ccc: None,
    }
    .resolve()
    .expect("installed loadout");
    let vfs = soe_godot_host::data_source::vfs_from_loadout(&loadout).expect("installed archives");
    let mut palettes = std::collections::BTreeMap::new();
    for model in ["fireplacewood01burning", "campfire01burning"] {
        let bytes = vfs
            .read(&format!("meshes/clutter/woodfires/{model}.nif"))
            .expect("hearth NIF");
        let file = soe_nif::parse(&bytes).expect("hearth parses");
        for mesh in file.meshes().expect("hearth shapes") {
            let Some(effect) = mesh.material.effect else {
                continue;
            };
            if effect.greyscale_texture.is_empty() {
                continue;
            }
            let path = soe_bsa::normalize(&effect.greyscale_texture);
            let bytes = vfs.read(&path).expect("authored palette DDS");
            let image = soe_dds::Dds::parse(&bytes)
                .expect("palette parses")
                .decode(0, 0)
                .expect("palette decodes");
            let mut samples = Vec::new();
            for (x, y) in [
                (0, 0),
                (image.width / 4, image.height / 4),
                (image.width / 2, image.height / 2),
                (image.width - 1, image.height - 1),
            ] {
                let index = ((y * image.width + x) * 4) as usize;
                samples.push(serde_json::json!([
                    x,
                    y,
                    image.rgba[index],
                    image.rgba[index + 1],
                    image.rgba[index + 2],
                    image.rgba[index + 3]
                ]));
            }
            palettes.insert(path, serde_json::json!({"width": image.width, "height": image.height, "samples": samples}));
        }
    }
    serde_json::to_value(palettes).expect("palette samples serialize")
}

#[test]
fn rnd_38_rnd_33_capture_covers_the_walkthrough_hearth() {
    let (Some(data), Some(godot)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        return;
    };
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let artifacts = project.join("../target/rnd_38");
    std::fs::create_dir_all(&artifacts).expect("capture artifact directory");
    let palettes = artifacts.join("authored_palettes.json");
    std::fs::write(&palettes, authored_palettes(Path::new(&data)).to_string())
        .expect("authored palette samples");
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "res://rnd_33_hearth_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .env("SOE_LOG", "off")
        .env("SOE_RND_38_PALETTES", &palettes)
        .env_remove("SOE_CAPTURE_PATH")
        .env_remove("SOE_AUD_11_TEST")
        .output()
        .expect("run RND-33 installed hearth capture");
    std::fs::write(artifacts.join("walkthrough.stdout.txt"), &output.stdout)
        .expect("capture stdout");
    std::fs::write(artifacts.join("walkthrough.stderr.txt"), &output.stderr)
        .expect("capture stderr");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("RESULT: PASS rnd_33_bannered_mare_hearth_capture"),
        "RND-33 hearth capture failed\n{stdout}\n{stderr}"
    );
    println!("{stdout}");
}
