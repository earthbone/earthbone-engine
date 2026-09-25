// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Verifies TOOL-02's done check: a resolved loadout, once packaged with
//! `Loadout::package`, runs without a mod manager in the loop. The test
//! resolves an MO2-shaped profile, packages it to a standalone directory,
//! then reloads that directory as a fresh `LoadoutInput` (game dir, one
//! merged `Data` root, `plugins.txt`, `loadorder.txt`, `Skyrim.ccc`, profile
//! INIs) with no `Mo2Instance` or mods/profiles tree anywhere in the input.
//! The reload must reproduce plugin order, early loaders, profile and
//! plugin-owned settings, archive ownership including a missing archive, and
//! overwrite-folder shadowing.

use std::path::Path;

use soe_esm::plugin::{flags, FileHeader};
use soe_esm::Plugin;
use soe_profile::mo2::Mo2Instance;
use soe_profile::{Layer, LoadoutInput};

fn plugin(masters: &[&str], flags: u32) -> Vec<u8> {
    Plugin::new(FileHeader {
        masters: masters.iter().map(|m| m.to_string()).collect(),
        flags,
        ..Default::default()
    })
    .to_bytes()
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn tool_02_package_reloads_as_a_standalone_loadout() {
    let tmp = tempfile::tempdir().unwrap();

    // Game install.
    let game = tmp.path().join("Skyrim Special Edition");
    let data = game.join("Data");
    write(&data.join("Skyrim.esm"), plugin(&[], flags::MASTER));
    write(
        &data.join("Update.esm"),
        plugin(&["Skyrim.esm"], flags::MASTER),
    );
    write(
        &data.join("ccBGSSSE001-Fish.esm"),
        plugin(&["Skyrim.esm"], flags::MASTER),
    );
    write(&data.join("Skyrim - Misc.bsa"), b"misc bsa");
    write(&game.join("Skyrim.ccc"), "ccBGSSSE001-Fish.esm\r\n");

    // MO2 instance: one mod with its own INI and archive, plus an overwrite
    // file shadowing a file the mod also ships.
    let root = tmp.path().join("mo2");
    let mods = root.join("mods");
    write(&mods.join("Low Mod/Low.esp"), plugin(&["Skyrim.esm"], 0));
    write(&mods.join("Low Mod/Low.ini"), "[Display]\nbUseTAA=1\n");
    write(&mods.join("Low Mod/Low.bsa"), b"low bsa");
    write(&mods.join("Low Mod/textures/a.dds"), b"mod version");
    write(&root.join("overwrite/textures/a.dds"), b"overwrite version");

    let profile = root.join("profiles/Default");
    write(&profile.join("modlist.txt"), "+Low Mod\r\n");
    write(
        &profile.join("settings.ini"),
        "[General]\nLocalSettings=true\n",
    );
    write(
        &profile.join("plugins.txt"),
        "*Skyrim.esm\r\n*Update.esm\r\n*Low.esp\r\n",
    );
    write(
        &profile.join("loadorder.txt"),
        "Skyrim.esm\r\nUpdate.esm\r\nLow.esp\r\n",
    );
    write(&profile.join("Skyrim.ini"), "[General]\nuGridsToLoad=7\n");
    write(
        &profile.join("SkyrimPrefs.ini"),
        "[Display]\nbUseTAA=0\n[Archive]\nsResourceArchiveList=Skyrim - Misc.bsa, Missing.bsa\n",
    );

    let instance = Mo2Instance::portable(&root, &game, "Default");
    let original = instance.loadout_input().unwrap().resolve().unwrap();

    // Package it, then reload the package with no mods/profiles tree, no
    // Mo2Instance, and no Skyrim_Default.ini in the loop: just the
    // standalone directory package() produced.
    let dest = tmp.path().join("packaged");
    original.package(&dest).unwrap();

    let reload_input = LoadoutInput {
        game_dir: dest.clone(),
        data_roots: vec![dest.join("Data")],
        plugins_txt: Some(dest.join("plugins.txt")),
        loadorder_txt: Some(dest.join("loadorder.txt")),
        ini_dir: Some(dest.clone()),
        ccc: None,
    };
    let reloaded = reload_input.resolve().unwrap();

    // Plugin order and early-loader flag survive.
    let orig_names: Vec<&str> = original
        .load_order
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    let reload_names: Vec<&str> = reloaded
        .load_order
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(orig_names, reload_names);
    assert!(
        reloaded
            .load_order
            .find("ccBGSSSE001-Fish.esm")
            .unwrap()
            .early,
        "Skyrim.ccc round-trips the early loader"
    );
    assert!(reloaded.load_order.active().all(|e| e.enabled));

    // Profile-layer settings (written to Skyrim.ini / SkyrimPrefs.ini) and
    // the plugin's own INI (copied along inside the mod's data root) both
    // round-trip with the same values and layer provenance.
    assert_eq!(
        reloaded.settings.get_uint("General", "uGridsToLoad"),
        Some(7)
    );
    assert_eq!(
        reloaded.settings.source("General", "uGridsToLoad"),
        Some(&Layer::Base)
    );
    assert_eq!(
        reloaded.settings.get_bool("Display", "bUseTAA"),
        Some(true),
        "the active plugin's own INI still outranks the profile Prefs INI"
    );
    assert_eq!(
        reloaded.settings.source("Display", "bUseTAA"),
        Some(&Layer::Plugin("Low.esp".into()))
    );

    // Archives: names and owners match, the missing one is still missing,
    // and a present one now resolves inside the packaged Data tree.
    let orig_archives: Vec<(&str, Option<&str>)> = original
        .archives
        .iter()
        .map(|a| (a.name.as_str(), a.owner.as_deref()))
        .collect();
    let reload_archives: Vec<(&str, Option<&str>)> = reloaded
        .archives
        .iter()
        .map(|a| (a.name.as_str(), a.owner.as_deref()))
        .collect();
    assert_eq!(orig_archives, reload_archives);
    let missing = reloaded
        .archives
        .iter()
        .find(|a| a.name == "Missing.bsa")
        .unwrap();
    assert!(missing.path.is_none());
    let misc = reloaded
        .archives
        .iter()
        .find(|a| a.name == "Skyrim - Misc.bsa")
        .unwrap();
    assert_eq!(
        misc.path.as_deref(),
        Some(dest.join("Data/Skyrim - Misc.bsa").as_path())
    );
    assert!(misc.path.as_ref().unwrap().is_file());

    // Overwrite shadowing survives: the packaged Data has the overwrite
    // content, not the mod's own copy of the same relative path.
    let shadowed = reloaded.find("textures/a.dds").unwrap();
    assert_eq!(std::fs::read(&shadowed).unwrap(), b"overwrite version");

    // The package itself needs no manager input: reload_input above named
    // only files package() wrote plus the game dir, nothing from mo2/.
    assert!(!dest.join("mods").exists());
    assert!(!dest.join("profiles").exists());
}
