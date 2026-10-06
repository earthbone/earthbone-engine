// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use swf::{Sprite, Tag};

fn movie(tags: &[Tag<'_>]) -> Vec<u8> {
    let mut header = swf::Header::default_with_swf_version(9);
    header.num_frames = 0;
    let mut bytes = Vec::new();
    swf::write_swf(&header, tags, &mut bytes).unwrap();
    bytes
}

fn nested_sprite(depth: u16) -> Tag<'static> {
    let tags = if depth == 0 {
        vec![Tag::End]
    } else {
        vec![nested_sprite(depth - 1), Tag::End]
    };
    Tag::DefineSprite(Sprite {
        id: depth,
        num_frames: 0,
        tags,
    })
}

#[test]
fn ui_24_sprite_nesting_beyond_parser_limit_is_rejected() {
    let bytes = movie(&[nested_sprite(65), Tag::End]);
    assert!(soe_godot_host::nordic_theme::own_art(
        &bytes,
        &soe_godot_host::nordic_theme::Exclusions::default()
    )
    .is_err());
}

#[test]
fn ui_24_second_extraction_cannot_reuse_a_populated_output_directory() {
    let (Some(nordic), Some(skyui), Some(data)) = (
        std::env::var_os("SOE_TEST_NORDIC"),
        std::env::var_os("SOE_TEST_SKYUI"),
        std::env::var_os("SKYRIM_DATA"),
    ) else {
        eprintln!("UI-24 installed data unset, skipped");
        return;
    };
    let skyui = soe_bsa::Archive::open(&PathBuf::from(skyui).join("SkyUI_SE.bsa")).unwrap();
    let vanilla =
        soe_bsa::Archive::open(&PathBuf::from(data).join("Skyrim - Interface.bsa")).unwrap();
    let nordic = PathBuf::from(nordic);
    let output =
        tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target")).unwrap();
    let first =
        soe_godot_host::nordic_theme::extract(&nordic, &skyui, &vanilla, output.path()).unwrap();
    let before = std::fs::read(output.path().join("provenance.json")).unwrap();
    assert!(
        soe_godot_host::nordic_theme::extract(&nordic, &skyui, &vanilla, output.path(),).is_err()
    );
    assert_eq!(
        std::fs::read(output.path().join("provenance.json")).unwrap(),
        before
    );
    assert!(first["elements"].as_object().unwrap().len() >= 15);
}

#[test]
fn ui_24_default_theme_scene_renders_the_menu_art() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("SOE_GODOT_BIN unset, skipped");
        return;
    };
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let capture = workspace.join("target/ui-24-breaker-default.png");
    let result = std::process::Command::new(godot)
        .args(["--path"])
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360", "--script", "ui_24_theme_test.gd"])
        .env("SOE_UI_24_CACHE", "")
        .env("SOE_UI_24_CAPTURE", &capture)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        result.status.success()
            && stdout.contains("UI-24 PASS")
            && !stderr.contains("SCRIPT ERROR")
            && capture.is_file(),
        "{stdout}\n{stderr}"
    );
}

#[test]
fn ui_24_installed_skyui_art_wins_across_menu_sections() {
    let (Some(godot), Some(skyui)) = (
        std::env::var_os("SOE_GODOT_BIN"),
        std::env::var_os("SOE_TEST_SKYUI"),
    ) else {
        eprintln!("SOE_GODOT_BIN or SOE_TEST_SKYUI unset, skipped");
        return;
    };
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let archive = soe_bsa::Archive::open(&PathBuf::from(skyui).join("SkyUI_SE.bsa")).unwrap();
    let cache = tempfile::tempdir_in(workspace.join("target")).unwrap();
    let mut movies = serde_json::Map::new();
    for name in [
        "icons_item_psychosteve",
        "icons_category_psychosteve",
        "inventorylists",
        "itemcard",
    ] {
        let path = format!("interface/skyui/{name}.swf");
        let movie = soe_swf_convert::convert(&archive.read(&path).unwrap()).unwrap();
        let directory = cache.path().join(name);
        soe_swf_convert::write_art(&movie, &directory).unwrap();
        std::fs::write(
            directory.join("movie.json"),
            serde_json::to_vec(&movie).unwrap(),
        )
        .unwrap();
        movies.insert(path, format!("{name}/movie.tres").into());
    }
    std::fs::write(
        cache.path().join("manifest.json"),
        serde_json::to_vec(&serde_json::json!({"movies":movies})).unwrap(),
    )
    .unwrap();
    let capture = workspace.join("target/ui-24-breaker-skyui.png");
    let result = std::process::Command::new(godot)
        .args(["--path"])
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360", "--script", "ui_24_theme_test.gd"])
        .env("SOE_UI_24_CACHE", cache.path())
        .env("SOE_UI_24_CAPTURE", &capture)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        result.status.success()
            && stdout.contains("UI-24 PASS")
            && !stderr.contains("SCRIPT ERROR")
            && capture.is_file(),
        "{stdout}\n{stderr}"
    );
}
