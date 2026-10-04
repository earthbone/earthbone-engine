// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! UI-20 form snapshots and native scene checks use external game art in place.

use std::path::PathBuf;
use std::process::Command;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::item_menu;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"MISC"), 0x800)
            .sub(Tag(*b"FULL"), b"\xc9bony ore\0".to_vec())
            .zsub(Tag(*b"DESC"), "An ore sample")
            .sub(
                Tag(*b"DATA"),
                [25_i32.to_le_bytes(), 2.0_f32.to_le_bytes()].concat(),
            ),
    );
    plugin.push_top(
        Record::new(Tag(*b"ARMO"), 0x801)
            .zsub(Tag(*b"FULL"), "Iron armor")
            .sub(
                Tag(*b"DATA"),
                [125_i32.to_le_bytes(), 30.0_f32.to_le_bytes()].concat(),
            )
            .sub(Tag(*b"DNAM"), 2500_u32.to_le_bytes()),
    );
    let mut spit = vec![0; 36];
    spit[..4].copy_from_slice(&12_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"SPEL"), 0x802)
            .zsub(Tag(*b"FULL"), "Flame")
            .sub(Tag(*b"SPIT"), spit),
    );
    let mut order = LoadOrder::new();
    order.add("Ui20.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut().add_item(player, id(0x800), 3);
    game.world_mut().add_item(player, id(0x801), 1);
    game.world_mut()
        .actor(player)
        .unwrap()
        .spells
        .insert(id(0x802));
    game
}

#[test]
fn ui_20_item_card_reads_names_counts_value_weight_armor_and_description() {
    let mut game = game();
    let player = game.player();
    let rows = item_menu::inventory(&game, player);
    assert_eq!(rows[0].name, "Ébony ore");
    assert_eq!(rows[0].count, 3);
    assert_eq!(rows[0].value, 25);
    assert_eq!(rows[0].weight, Some(2.0));
    assert_eq!(rows[0].description, "An ore sample");
    assert_eq!(rows[1].armor, Some(25.0));
    assert_eq!(rows[1].category, "Apparel");
    assert!(!rows[1].icon.is_empty());
}

#[test]
fn ui_20_magic_card_reads_known_spell_cost_and_shared_equipment() {
    let mut game = game();
    let player = game.player();
    assert_eq!(item_menu::magic(&game)[0].cost, Some(12));
    game.world_mut()
        .equip_hand_spell(player, soe_game::combat::Hand::Left, id(0x802))
        .unwrap();
    assert!(item_menu::magic(&game)[0].equipped);
}

#[test]
fn ui_20_favorite_actions_use_existing_actor_state_and_reject_unowned_forms() {
    let mut game = game();
    let player = game.player();
    assert!(item_menu::toggle_favorite(&game, id(0x800)));
    assert!(item_menu::toggle_favorite(&game, id(0x802)));
    assert!(game
        .world()
        .actor_ref(player)
        .unwrap()
        .favorites
        .contains(&id(0x800)));
    assert!(item_menu::magic(&game)[0].favorite);
    assert!(!item_menu::toggle_favorite(&game, id(0xdead)));
    assert!(item_menu::toggle_favorite(&game, id(0x800)));
    assert!(!item_menu::inventory(&game, player)[0].favorite);
}

#[test]
fn ui_20_invalid_and_nonfinite_weight_remains_missing_in_the_card() {
    let mut plugin = Plugin::new(FileHeader::default());
    for (object, data) in [
        (0x900, vec![0]),
        (
            0x901,
            [1_i32.to_le_bytes(), f32::NAN.to_le_bytes()].concat(),
        ),
    ] {
        plugin.push_top(Record::new(Tag(*b"MISC"), object).sub(Tag(*b"DATA"), data));
    }
    let mut order = LoadOrder::new();
    order.add("Invalid.esm", plugin).unwrap();
    let game = Game::new(order);
    assert_eq!(item_menu::item(&game, id(0x900), 1).weight, None);
    assert_eq!(item_menu::item(&game, id(0x901), 1).weight, None);
}

#[test]
fn ui_20_vanilla_records_supply_inventory_cards() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipped");
        return;
    };
    let mut order = LoadOrder::new();
    order
        .add(
            "Skyrim.esm",
            soe_esm::read_plugin(PathBuf::from(data).join("Skyrim.esm"))
                .unwrap()
                .unwrap(),
        )
        .unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    for name in ["IronSword", "ArmorIronCuirass", "Lockpick"] {
        let form = game.form(name).unwrap();
        game.world_mut().add_item(player, form, 2);
        let row = item_menu::item(&game, form, 2);
        assert!(!row.name.contains(':'));
        assert!(row.weight.is_some(), "{name}: no weight");
        assert!(row.value > 0, "{name}: no value");
    }
    assert!(item_menu::inventory(&game, player).len() >= 3);
}

fn run_scene(cache: Option<&std::path::Path>) {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("SOE_GODOT_BIN unset, skipped");
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let capture = workspace.join("target/ui-20");
    std::fs::create_dir_all(&capture).unwrap();
    let output = Command::new(godot)
        .args(["--path"])
        .arg(workspace.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "ui_20_item_menu_test.gd",
        ])
        .env(
            "SOE_INTERFACE_CACHE",
            cache.unwrap_or_else(|| std::path::Path::new("")),
        )
        .env("SOE_UI_20_CAPTURE", capture.join("scene.png"))
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
    assert!(capture.join("scene.png").exists());
}

#[test]
fn ui_20_godot_scene_checks_columns_sort_search_tabs_card_and_capture() {
    run_scene(None);
}

#[test]
fn ui_20_skyui_install_supplies_external_converted_art() {
    let Some(root) = std::env::var_os("SOE_TEST_SKYUI") else {
        eprintln!("SOE_TEST_SKYUI unset, skipped");
        return;
    };
    let root = PathBuf::from(root);
    assert!(root.join("SkyUI_SE.esp").is_file());
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_archive(soe_bsa::Archive::open(&root.join("SkyUI_SE.bsa")).unwrap());
    let external = tempfile::Builder::new()
        .prefix("soe-ui20-")
        .tempdir_in("/tmp")
        .unwrap();
    let cache = soe_swf_convert::build_cache(
        &vfs,
        &["SkyUI_SE.esp".into()],
        external.path(),
        &soe_swf_convert::Decompiler::from_env(),
    )
    .unwrap();
    assert!(cache
        .movies
        .keys()
        .any(|path| path.contains("inventorymenu")));
    assert!(cache.movies.keys().any(|path| path.contains("itemicons")));
    run_scene(Some(&cache.directory));
}
