// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run_probe(source: &str, cache: Option<&Path>, name: &str) {
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN is required");
    let directory = tempfile::tempdir_in(workspace().join("target")).unwrap();
    let script = directory.path().join("probe.gd");
    std::fs::write(&script, source).unwrap();
    let result = Command::new(godot)
        .args(["--path"])
        .arg(workspace().join("godot"))
        .args(["--resolution", "640x360", "--script"])
        .arg(&script)
        .env("SOE_UI_24_CACHE", cache.unwrap_or_else(|| Path::new("")))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        result.status.success() && stdout.contains(name) && !stderr.contains("SCRIPT ERROR"),
        "{stdout}\n{stderr}"
    );
}

#[test]
fn ui_24_category_and_unknown_item_lookups_resolve_real_theme_art() {
    if std::env::var_os("SOE_GODOT_BIN").is_none() {
        eprintln!("SOE_GODOT_BIN unset, skipped");
        return;
    }
    run_probe(
        r#"extends SceneTree
const T = preload("res://ui/theme/theme.gd")
const I = preload("res://ui/item_icon.gd")
func _initialize():
	var expected = {"All":"category_all", "Favorites":"category_favorites", "Weapons":"default_weapon", "Apparel":"default_armor", "Potions":"default_potion", "Scrolls":"default_scroll", "Food":"default_food", "Ingredients":"default_ingredient", "Books":"default_book", "Keys":"default_key", "Misc":"default_misc", "Spells":"default_spell", "Lights":"default_misc", "Shouts":"default_spell", "Gear":"category_gear", "Aid":"category_aid", "Magic":"category_magic"}
	for category in expected:
		var name = T.category_icon(category)
		assert(name == expected[category], "wrong category icon: " + category)
		var icon = I.new()
		icon.icon_name = name
		root.add_child(icon)
		await process_frame
		assert(not icon.meshes.is_empty() or icon.texture != null, "missing theme art: " + category)
		assert(icon.art_source == T.resource_path(name), "wrong theme resource: " + category)
		icon.free()
	assert(T.category_icon("unknown") == "default_misc")
	assert(T.resource_path("unknown") == T.resource_path("default_misc"))
	print("UI-24 ADVERSARIAL CATEGORY PASS")
	quit()
"#,
        None,
        "UI-24 ADVERSARIAL CATEGORY PASS",
    );
}

#[test]
fn ui_24_every_menu_mode_renders_default_categories_items_panels_and_markers() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("SOE_GODOT_BIN unset, skipped");
        return;
    };
    let capture = workspace().join("target/ui-24-adversarial-default.png");
    let result = Command::new(godot)
        .args(["--path"])
        .arg(workspace().join("godot"))
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
fn ui_24_skyui_category_art_wins_for_a_real_magic_spell_row() {
    let (Some(_godot), Some(skyui)) = (
        std::env::var_os("SOE_GODOT_BIN"),
        std::env::var_os("SOE_TEST_SKYUI"),
    ) else {
        eprintln!("SOE_GODOT_BIN or SOE_TEST_SKYUI unset, skipped");
        return;
    };
    let archive = soe_bsa::Archive::open(
        &PathBuf::from(skyui).join("SkyUI_SE.bsa"),
    )
    .unwrap();
    let cache = tempfile::tempdir_in(workspace().join("target")).unwrap();
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
    run_probe(
        r#"extends SceneTree
const Menu = preload("res://ui/item_menu.gd")
func _initialize():
	call_deferred("check_spell")
func check_spell():
	var menu = Menu.new()
	root.add_child(menu)
	menu.configure("magic", [{"id":"spell", "name":"Test spell", "category":"Spells", "icon":"default_spell", "count":1, "side":0}], OS.get_environment("SOE_UI_24_CACHE"))
	await process_frame
	await process_frame
	assert(menu.rows.get_child_count() == 1, "magic menu lost its spell row")
	var icon = menu.rows.get_child(0).get_child(0).get_child(1)
	assert(icon.icon_name == "default_spell", "spell row lost its icon mapping")
	assert(icon.art_source == "interface/skyui/icons_category_psychosteve.swf", "SkyUI mag_all did not override the default spell icon")
	assert(not icon.meshes.is_empty(), "SkyUI spell icon has no art")
	print("UI-24 ADVERSARIAL SPELL PASS")
	quit()
"#,
        Some(cache.path()),
        "UI-24 ADVERSARIAL SPELL PASS",
    );
}

fn reference_art(bytes: &[u8]) -> Vec<String> {
    fn visit(mut tags: &[u8], version: u8, hashes: &mut Vec<String>) {
        while !tags.is_empty() {
            let mut reader = swf::read::Reader::new(tags, version);
            let (code, size) = reader.read_tag_code_and_length().unwrap();
            let payload = &reader.get_ref()[..size];
            if code == 0 {
                break;
            }
            if code == 39 {
                visit(&payload[4..], version, hashes);
            }
            if matches!(code, 2 | 22 | 32 | 83 | 6 | 20 | 21 | 35 | 36 | 90) {
                let mut hash = sha2::Sha256::new();
                use sha2::Digest;
                hash.update(code.to_le_bytes());
                hash.update(&payload[2..]);
                hashes.push(format!("{:x}", hash.finalize()));
            }
            tags = &reader.get_ref()[size..];
        }
    }
    let movie = swf::decompress_swf(bytes).unwrap();
    let mut hashes = Vec::new();
    visit(&movie.data, movie.header.version(), &mut hashes);
    hashes
}

#[test]
fn ui_24_extracted_nordic_art_has_no_definition_in_either_reference_archive() {
    let (Some(nordic), Some(skyui), Some(data)) = (
        std::env::var_os("SOE_TEST_NORDIC"),
        std::env::var_os("SOE_TEST_SKYUI"),
        std::env::var_os("SKYRIM_DATA"),
    ) else {
        eprintln!("SOE_TEST_NORDIC, SOE_TEST_SKYUI or SKYRIM_DATA unset, skipped");
        return;
    };
    let skyui = soe_bsa::Archive::open(&PathBuf::from(skyui).join("SkyUI_SE.bsa")).unwrap();
    let vanilla = soe_bsa::Archive::open(
        &PathBuf::from(data).join("Skyrim - Interface.bsa"),
    )
    .unwrap();
    let output = tempfile::tempdir_in(workspace().join("target")).unwrap();
    let manifest = soe_godot_host::nordic_theme::extract(
        &PathBuf::from(nordic),
        &skyui,
        &vanilla,
        output.path(),
    )
    .unwrap();
    let reference: std::collections::BTreeSet<_> = [&skyui, &vanilla]
        .into_iter()
        .flat_map(|archive| {
            archive
                .files()
                .iter()
                .filter(|path| {
                    Path::new(path).extension().is_some_and(|extension| {
                        extension.eq_ignore_ascii_case("swf")
                            || extension.eq_ignore_ascii_case("gfx")
                    })
                })
                .flat_map(|path| reference_art(&archive.read(path).unwrap()))
        })
        .collect();
    let elements = manifest["elements"].as_object().unwrap();
    assert!(!elements.is_empty(), "NORDIC install yielded no owned art");
    for (path, definition) in elements {
        let fingerprint = definition["sha256"].as_str().unwrap();
        assert!(!reference.contains(fingerprint), "reference art escaped: {path}");
        assert!(output.path().join(path).is_file(), "missing extracted art: {path}");
        assert!(definition["source"].as_str().unwrap().starts_with("interface/skyui/"));
    }
}
