// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! An MO2-shaped instance on a temp tree resolves to the load order, INI
//! values and archives the engine should use.

use std::path::Path;

use soe_esm::plugin::{flags, FileHeader};
use soe_esm::Plugin;
use soe_profile::mo2::Mo2Instance;
use soe_profile::{Layer, Problem, Slot};

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
fn an_mo2_profile_resolves_to_a_playable_loadout() {
    let tmp = tempfile::tempdir().unwrap();
    let game = tmp.path().join("Skyrim Special Edition");
    let data = game.join("Data");
    for m in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        write(&data.join(m), plugin(&["Skyrim.esm"], flags::MASTER));
    }
    write(
        &data.join("ccBGSSSE001-Fish.esm"),
        plugin(&["Skyrim.esm"], flags::MASTER),
    );
    write(&data.join("Skyrim - Misc.bsa"), b"bsa");
    write(&data.join("Skyrim - Textures0.bsa"), b"bsa");
    write(
        &game.join("Skyrim_Default.ini"),
        "[General]\nsLanguage=ENGLISH\nuGridsToLoad=5\n[Archive]\nsResourceArchiveList=Skyrim - Misc.bsa\nsResourceArchiveList2=Skyrim - Textures0.bsa, Missing.bsa\n[Display]\nbUseTAA=0\n",
    );
    write(
        &game.join("Skyrim.ccc"),
        "ccBGSSSE001-Fish.esm\r\nccNotOwned.esl\r\n",
    );

    let root = tmp.path().join("mo2");
    let mods = root.join("mods");
    write(&mods.join("Low Mod/Low.esp"), plugin(&["Skyrim.esm"], 0));
    write(
        &mods.join("Low Mod/Low.ini"),
        "[Display]\nbUseTAA=1\n[Archive]\nsResourceArchiveList=Evil.bsa\n",
    );
    write(&mods.join("Low Mod/Low.bsa"), b"bsa");
    write(&mods.join("Low Mod/Low - Textures.bsa"), b"bsa");
    write(&mods.join("Low Mod/textures/a.dds"), b"dds");
    write(
        &mods.join("High Mod/High.esl"),
        plugin(&["Skyrim.esm", "Low.esp"], 0),
    );
    write(
        &mods.join("High Mod/Flagged.esp"),
        plugin(&["Skyrim.esm"], flags::MASTER),
    );
    write(&mods.join("High Mod/textures/a.dds"), b"dds high");
    write(&mods.join("Off Mod/Off.esp"), plugin(&["Skyrim.esm"], 0));
    write(&mods.join("Off Mod/Off.ini"), "[Display]\nbUseTAA=0\n");
    write(&root.join("overwrite/textures/b.dds"), b"dds");

    let profile = root.join("profiles/Default");
    write(
        &profile.join("modlist.txt"),
        "# generated\r\n+High Mod\r\n-Off Mod\r\n+Low Mod\r\n*DLC: Dawnguard\r\n",
    );
    write(
        &profile.join("plugins.txt"),
        "*Skyrim.esm\r\n*Update.esm\r\n*Low.esp\r\n*High.esl\r\nFlagged.esp\r\nOff.esp\r\n",
    );
    write(
        &profile.join("loadorder.txt"),
        "Skyrim.esm\r\nUpdate.esm\r\nLow.esp\r\nHigh.esl\r\nFlagged.esp\r\n",
    );
    write(
        &profile.join("settings.ini"),
        "[General]\nLocalSettings=true\nLocalSaves=true\n",
    );
    write(
        &profile.join("Skyrim.ini"),
        "[General]\nuGridsToLoad=7\n[Display]\nfShadowDistance=3000\n",
    );
    write(
        &profile.join("SkyrimPrefs.ini"),
        "[Display]\nfShadowDistance=8000\n",
    );

    let instance = Mo2Instance::portable(&root, &game, "Default");
    let input = instance.loadout_input().unwrap();
    assert_eq!(
        input.data_roots,
        vec![
            data.clone(),
            mods.join("Low Mod"),
            mods.join("High Mod"),
            root.join("overwrite")
        ]
    );
    let loadout = input.resolve().unwrap();

    let names: Vec<&str> = loadout
        .load_order
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "Skyrim.esm",
            "Update.esm",
            "Dawnguard.esm",
            "HearthFires.esm",
            "Dragonborn.esm",
            "ccBGSSSE001-Fish.esm",
            "High.esl",
            "Flagged.esp",
            "Low.esp",
        ]
    );
    let order = &loadout.load_order;
    assert_eq!(
        order.find("Dawnguard.esm").unwrap().slot,
        Some(Slot::Regular(2))
    );
    assert_eq!(
        order.find("ccBGSSSE001-Fish.esm").unwrap().slot,
        Some(Slot::Regular(5))
    );
    assert_eq!(order.find("High.esl").unwrap().slot, Some(Slot::Light(0)));
    assert_eq!(order.find("Low.esp").unwrap().slot, Some(Slot::Regular(6)));
    assert!(!order.find("Flagged.esp").unwrap().enabled);
    assert!(
        order.find("Off.esp").is_none(),
        "a disabled mod's plugin is not on disk for the engine"
    );
    assert_eq!(
        order.problems,
        vec![
            Problem::Missing {
                name: "Off.esp".into()
            },
            Problem::MasterLater {
                plugin: "High.esl".into(),
                master: "Low.esp".into()
            },
        ]
    );
    assert!(order.broken());

    let s = &loadout.settings;
    assert_eq!(s.get_uint("General", "uGridsToLoad"), Some(7));
    assert_eq!(s.get_float("Display", "fShadowDistance"), Some(8000.0));
    assert_eq!(s.source("Display", "fShadowDistance"), Some(&Layer::Prefs));
    assert_eq!(
        s.get_bool("Display", "bUseTAA"),
        Some(true),
        "the active plugin's INI applies"
    );
    assert_eq!(
        s.source("Display", "bUseTAA"),
        Some(&Layer::Plugin("Low.esp".into()))
    );
    assert_eq!(s.refused.len(), 1, "its archive list is refused");
    assert_eq!(s.get("General", "sLanguage"), Some("ENGLISH"));

    let archives: Vec<(&str, Option<&str>)> = loadout
        .archives
        .iter()
        .map(|a| (a.name.as_str(), a.owner.as_deref()))
        .collect();
    assert_eq!(
        archives,
        vec![
            ("Skyrim - Misc.bsa", None),
            ("Skyrim - Textures0.bsa", None),
            ("Missing.bsa", None),
            ("Low - Textures.bsa", Some("Low.esp")),
            ("Low.bsa", Some("Low.esp")),
        ]
    );
    assert!(loadout.archives[2].path.is_none());
    assert_eq!(
        loadout.archives[3].path.as_deref(),
        Some(mods.join("Low Mod/Low - Textures.bsa").as_path())
    );

    assert_eq!(
        loadout.find("textures/a.dds"),
        Some(mods.join("High Mod/textures/a.dds"))
    );
    assert_eq!(
        loadout.find("textures/b.dds"),
        Some(root.join("overwrite/textures/b.dds"))
    );
    assert_eq!(loadout.find("textures/none.dds"), None);
}

#[test]
fn a_vanilla_install_without_lists_loads_only_the_early_files() {
    let tmp = tempfile::tempdir().unwrap();
    let game = tmp.path().join("game");
    let data = game.join("Data");
    for m in ["Skyrim.esm", "Update.esm"] {
        write(&data.join(m), plugin(&[], flags::MASTER));
    }
    write(&data.join("Stray.esp"), plugin(&["Skyrim.esm"], 0));
    write(
        &data.join("Broken.esp"),
        b"not a plugin at all, just some bytes here",
    );
    let loadout = soe_profile::LoadoutInput::vanilla(&game, None, None)
        .resolve()
        .unwrap();
    assert_eq!(
        loadout.load_order.active_names(),
        vec!["Skyrim.esm", "Update.esm"]
    );
    assert!(!loadout.load_order.find("Stray.esp").unwrap().enabled);
    assert!(loadout
        .load_order
        .problems
        .iter()
        .any(|p| matches!(p, Problem::HardcodedMissing { name } if name == "Dawnguard.esm")));
    assert!(loadout
        .load_order
        .problems
        .iter()
        .any(|p| matches!(p, Problem::Unreadable { name, .. } if name == "Broken.esp")));
    assert!(loadout.settings.is_empty());
    assert!(loadout.archives.is_empty());
}
