// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for UI-20 item classification, saved favorites and menu reuse.

use std::path::PathBuf;
use std::process::Command;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::item_menu;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"ALCH"), 0x800)
            .zsub(Tag(*b"FULL"), "Apple")
            .sub(Tag(*b"DATA"), 0.1_f32.to_le_bytes())
            .sub(
                Tag(*b"ENIT"),
                [
                    25_i32.to_le_bytes().to_vec(),
                    2_u32.to_le_bytes().to_vec(),
                    vec![0; 12],
                ]
                .concat(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("Ui20Adversarial.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut().add_item(player, id(0x800), 2);
    game
}

#[test]
fn ui_20_food_flag_selects_food_category_and_food_icon() {
    let mut game = fixture();
    let player = game.player();
    let row = item_menu::inventory(&game, player)
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(row.category, "Food");
    assert_eq!(row.icon, "default_food");
    let card = item_menu::item(&game, id(0x800), 1);
    assert_eq!((card.category, card.icon), (row.category, row.icon));
}

#[test]
fn ui_20_favorite_toggle_survives_game_save_and_load() {
    let game = fixture();
    assert!(item_menu::toggle_favorite(&game, id(0x800)));
    let saved = game.save_state().unwrap();
    let mut restored = fixture();
    restored.load_state(&saved).unwrap();
    let player = restored.player();
    let row = item_menu::inventory(&restored, player)
        .into_iter()
        .next()
        .unwrap();
    assert!(row.favorite);
}

#[test]
fn ui_20_reopening_favorites_reconfigures_the_existing_item_menu() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        panic!("SOE_GODOT_BIN must be set for the UI-20 host lifecycle check");
    };
    let data = std::env::var_os("SKYRIM_DATA").map_or_else(
        || PathBuf::from("/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data"),
        PathBuf::from,
    );
    assert!(
        data.join("Skyrim.esm").is_file(),
        "installed Skyrim data is unavailable"
    );
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = r#"extends SceneTree
var failures := 0
func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("UI-20 adversarial: " + message)
func _initialize() -> void:
	call_deferred("run")
func run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	check(streamer.call("load_data", data), "installed data failed to load")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	check(player.call("use_streamer_data", streamer), "shared data was rejected")
	check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0, "player cell failed to load")
	check(player.call("start_gameplay"), "game did not start")
	check(player.call("open_inventory_menu"), "inventory did not open")
	var menu: Control = player.call("menu_content").get_node("ItemMenu")
	check(menu.mode == "inventory", "initial presenter mode is not inventory")
	player.call("close_menu")
	check(player.call("open_favorites_menu"), "favorites did not reopen")
	menu = player.call("menu_content").get_node("ItemMenu")
	check(menu.mode == "favorites", "reused presenter kept its previous mode")
	check(menu.categories.get_child_count() == 4, "favorites categories were not rebuilt")
	player.call("close_menu")
	player.free()
	streamer.free()
	await process_frame
	print("UI-20 adversarial ", "PASS" if failures == 0 else "FAIL")
	quit(0 if failures == 0 else 1)
"#;
    let directory = tempfile::tempdir_in("/tmp").unwrap();
    let script_path = directory.path().join("ui_20_adversarial.gd");
    std::fs::write(&script_path, script).unwrap();
    let output = Command::new(godot)
        .args(["--path"])
        .arg(workspace.join("godot"))
        .arg("--headless")
        .arg("--script")
        .arg(&script_path)
        .env("SKYRIM_DATA", &data)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("UI-20 adversarial PASS"),
        "{stdout}\n{stderr}"
    );
}

#[test]
fn ui_20_skyui_archive_supplies_external_item_icons_to_the_scene() {
    let Some(root) = std::env::var_os("SOE_TEST_SKYUI") else {
        eprintln!("SOE_TEST_SKYUI unset, skipped");
        return;
    };
    if std::env::var_os("SOE_FFDEC_JAR").is_none() && std::env::var_os("SOE_FFDEC").is_none() {
        eprintln!("SOE_FFDEC_JAR or SOE_FFDEC must be set for AS2 export, skipped");
        return;
    }
    let root = PathBuf::from(root);
    let esp = root.join("SkyUI_SE.esp");
    let bsa = root.join("SkyUI_SE.bsa");
    assert!(
        esp.is_file() && bsa.is_file(),
        "installed SkyUI files are unavailable"
    );
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_archive(soe_bsa::Archive::open(&bsa).unwrap());
    let cache_root = tempfile::tempdir_in("/tmp").unwrap();
    let cache = soe_swf_convert::build_cache(
        &vfs,
        &["SkyUI_SE.esp".into()],
        cache_root.path(),
        &soe_swf_convert::Decompiler::from_env(),
    )
    .unwrap();
    assert!(
        cache
            .movies
            .keys()
            .any(|path| path.to_ascii_lowercase().contains("inventorymenu")),
        "converted movies: {:?}",
        cache.movies.keys().collect::<Vec<_>>()
    );
    assert!(
        cache
            .movies
            .keys()
            .any(|path| path.to_ascii_lowercase().contains("icons_item")),
        "converted movies: {:?}",
        cache.movies.keys().collect::<Vec<_>>()
    );

    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN must be set");
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(godot)
        .args(["--path"])
        .arg(workspace.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "ui_20_item_menu_test.gd",
        ])
        .env("SOE_INTERFACE_CACHE", &cache.directory)
        .env("SOE_UI_20_CAPTURE", cache_root.path().join("scene.png"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("UI-20 PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "{stdout}\n{stderr}"
    );
}
