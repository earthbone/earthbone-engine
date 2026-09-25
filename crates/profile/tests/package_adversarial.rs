// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for `Loadout::package`, TOOL-02. The row's own report
//! says `package()` shipped with no new tests. These try to break the
//! documented contract on `Loadout::package` in `crates/profile/src/lib.rs`:
//! a standalone install an engine reads without a mod manager in the loop,
//! reloadable by the recipe the doc comment spells out (`data_roots` at
//! `dest/Data`, `plugins_txt`/`loadorder_txt` at the written files, `ini_dir`
//! and `game_dir` at `dest`).

use std::path::Path;

use soe_esm::plugin::{flags, FileHeader};
use soe_esm::Plugin;
use soe_profile::mo2::Mo2Instance;
use soe_profile::LoadoutInput;

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

/// Round trip through `package()` and the documented reload recipe. Sourced
/// from the doc comment on `Loadout::package` (`crates/profile/src/lib.rs`,
/// lines 77 to 93): "Reload the result with a `LoadoutInput` whose
/// `data_roots` is `[dest.join("Data")]`, `plugins_txt` and `loadorder_txt`
/// point at the files here, `ini_dir` is `dest`, and `game_dir` and `ccc`
/// point at wherever `Skyrim_Default.ini` and the game's own `Skyrim.ccc`
/// live (`dest` itself, for a fully standalone copy)."
///
/// `Skyrim_Default.ini` is explicitly out of scope for `package()` (same
/// comment: "not written here, that file belongs to the game install the
/// package lands in"), so the test copies it by hand, the way a real caller
/// packaging into an existing game install would.
#[test]
fn package_then_reload_reproduces_the_original_resolve() {
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
    let default_ini =
        "[General]\nsLanguage=ENGLISH\n[Archive]\nsResourceArchiveList=Skyrim - Misc.bsa\n[Display]\nbUseTAA=0\n";
    write(&game.join("Skyrim_Default.ini"), default_ini);
    write(&game.join("Skyrim.ccc"), "ccBGSSSE001-Fish.esm\r\n");

    let root = tmp.path().join("mo2");
    let mods = root.join("mods");
    write(&mods.join("Low Mod/Low.esp"), plugin(&["Skyrim.esm"], 0));
    write(
        &mods.join("Low Mod/Low.ini"),
        "[Display]\nbUseTAA=1\n[Archive]\nsResourceArchiveList=Evil.bsa\n",
    );

    let profile = root.join("profiles/Default");
    write(&profile.join("modlist.txt"), "# generated\r\n+Low Mod\r\n");
    write(&profile.join("plugins.txt"), "*Skyrim.esm\r\n*Low.esp\r\n");
    write(&profile.join("loadorder.txt"), "Skyrim.esm\r\nLow.esp\r\n");
    write(
        &profile.join("settings.ini"),
        "[General]\nLocalSettings=true\n",
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
    let original = instance.loadout_input().unwrap().resolve().unwrap();

    let dest = tmp.path().join("packaged");
    original.package(&dest).unwrap();
    // The doc's own reload recipe needs Skyrim_Default.ini at dest, which
    // package() explicitly does not write.
    write(&dest.join("Skyrim_Default.ini"), default_ini);

    let reload_input = LoadoutInput {
        game_dir: dest.clone(),
        data_roots: vec![dest.join("Data")],
        plugins_txt: Some(dest.join("plugins.txt")),
        loadorder_txt: Some(dest.join("loadorder.txt")),
        ini_dir: Some(dest.clone()),
        ccc: None,
    };
    let reloaded = reload_input.resolve().unwrap();

    let orig_active: Vec<(&str, _)> = original
        .load_order
        .active()
        .map(|e| (e.name.as_str(), e.slot))
        .collect();
    let reload_active: Vec<(&str, _)> = reloaded
        .load_order
        .active()
        .map(|e| (e.name.as_str(), e.slot))
        .collect();
    assert_eq!(
        orig_active, reload_active,
        "packaged install must resolve to the same active plugins and slots"
    );
    assert!(
        reloaded.load_order.problems.is_empty(),
        "a packaged install must not gain new load order problems: {:?}",
        reloaded.load_order.problems
    );

    // Every merged setting from the original loadout must come back with the
    // same effective value after packaging and reload.
    for (id, value) in original.settings.iter() {
        assert_eq!(
            reloaded.settings.get(&id.0, &id.1),
            Some(value.value.as_str()),
            "setting {id:?} did not survive package()+reload"
        );
    }
    assert_eq!(
        reloaded.settings.get_bool("Display", "bUseTAA"),
        Some(true),
        "Low.esp's own INI must travel with it and still win"
    );
}

/// `copy_tree` is documented to overwrite a lower root's file with a higher
/// root's ("A file already at the destination is overwritten, so calling
/// this for each data root in low to high order reproduces the shadowing
/// `Loadout::find` does"). This packages two data roots that both carry
/// `textures/a.dds` and checks the higher-priority root's bytes win on disk,
/// matching what `Loadout::find` would have resolved before packaging.
#[test]
fn package_shadows_a_lower_roots_file_with_the_higher_roots_file() {
    let tmp = tempfile::tempdir().unwrap();
    let low = tmp.path().join("low");
    let high = tmp.path().join("high");
    write(&low.join("textures/a.dds"), b"low content");
    write(&high.join("textures/a.dds"), b"high content");

    let input = LoadoutInput {
        game_dir: tmp.path().join("game"),
        data_roots: vec![low.clone(), high.clone()],
        plugins_txt: None,
        loadorder_txt: None,
        ini_dir: None,
        ccc: None,
    };
    let loadout = input.resolve().unwrap();
    assert_eq!(
        loadout.find("textures/a.dds"),
        Some(high.join("textures/a.dds")),
        "the higher root should win before packaging too"
    );

    let dest = tmp.path().join("packaged");
    loadout.package(&dest).unwrap();
    let packaged = std::fs::read(dest.join("Data/textures/a.dds")).unwrap();
    assert_eq!(
        packaged, b"high content",
        "packaged Data must keep the higher-priority root's file, not the lower one's"
    );
}

/// An archive an INI lists but that is on no data root (`path: None`) must
/// not stop packaging, and the INI text that names it must survive
/// unmodified so a caller that inspects `loadout.archives` after reload
/// still sees the same missing reference, not a silently dropped one.
#[test]
fn package_survives_an_ini_listed_archive_with_no_file_on_disk() {
    let tmp = tempfile::tempdir().unwrap();
    let game = tmp.path().join("game");
    let data = game.join("Data");
    write(&data.join("Skyrim.esm"), plugin(&[], flags::MASTER));
    write(
        &game.join("Skyrim_Default.ini"),
        "[Archive]\nsResourceArchiveList=Missing.bsa\n",
    );

    let input = LoadoutInput::vanilla(&game, None, None);
    let loadout = input.resolve().unwrap();
    assert_eq!(loadout.archives.len(), 1);
    assert!(loadout.archives[0].path.is_none());

    let dest = tmp.path().join("packaged");
    loadout.package(&dest).unwrap();

    // package() does not write the Default layer (documented), so reload the
    // way the doc recipe says, with the caller's own Skyrim_Default.ini.
    write(
        &dest.join("Skyrim_Default.ini"),
        "[Archive]\nsResourceArchiveList=Missing.bsa\n",
    );
    let reload_input = LoadoutInput {
        game_dir: dest.clone(),
        data_roots: vec![dest.join("Data")],
        plugins_txt: Some(dest.join("plugins.txt")),
        loadorder_txt: Some(dest.join("loadorder.txt")),
        ini_dir: None,
        ccc: None,
    };
    let reloaded = reload_input.resolve().unwrap();
    assert_eq!(reloaded.archives.len(), 1);
    assert_eq!(reloaded.archives[0].name, "Missing.bsa");
    assert!(reloaded.archives[0].path.is_none());
}

/// A vanilla, unmodded loadout carries no Base/Custom/Prefs settings (all its
/// values come from the Default layer, which the doc says `package()` does
/// not write) and no Creation Club early loader beyond the hardcoded five.
/// `package()` should therefore write no `Skyrim.ini`, `SkyrimCustom.ini`,
/// `SkyrimPrefs.ini` or `Skyrim.ccc` at all, per the `if !ini.sections.is_empty()`
/// and `if !creations.is_empty()` guards in `Loadout::package`.
#[test]
fn package_writes_no_optional_files_for_a_bare_vanilla_loadout() {
    let tmp = tempfile::tempdir().unwrap();
    let game = tmp.path().join("game");
    let data = game.join("Data");
    for m in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        write(&data.join(m), plugin(&[], flags::MASTER));
    }
    write(
        &game.join("Skyrim_Default.ini"),
        "[General]\nsLanguage=ENGLISH\n",
    );

    let loadout = LoadoutInput::vanilla(&game, None, None).resolve().unwrap();
    let dest = tmp.path().join("packaged");
    loadout.package(&dest).unwrap();

    assert!(dest.join("plugins.txt").exists());
    assert!(dest.join("loadorder.txt").exists());
    for name in [
        "Skyrim.ini",
        "SkyrimCustom.ini",
        "SkyrimPrefs.ini",
        "Skyrim.ccc",
    ] {
        assert!(
            !dest.join(name).exists(),
            "{name} should not be written for a bare vanilla loadout with no Base/Custom/Prefs/CC data"
        );
    }
}

/// Two mods that use the same relative path for different kinds of content
/// (one ships a loose file, a higher-priority one ships a folder with that
/// name) is an ordinary Data-folder collision, the same shadowing `find` and
/// `copy_tree`'s doc comment both claim to handle by letting the higher root
/// win. `package()` must still produce a usable install, not an `Err` that
/// aborts packaging partway through.
#[test]
fn package_handles_a_file_becoming_a_directory_across_roots() {
    let tmp = tempfile::tempdir().unwrap();
    let low = tmp.path().join("low");
    let high = tmp.path().join("high");
    write(&low.join("Stuff"), b"a loose file named Stuff");
    write(&high.join("Stuff/inner.txt"), b"a folder named Stuff");

    let input = LoadoutInput {
        game_dir: tmp.path().join("game"),
        data_roots: vec![low, high],
        plugins_txt: None,
        loadorder_txt: None,
        ini_dir: None,
        ccc: None,
    };
    let loadout = input.resolve().unwrap();
    let dest = tmp.path().join("packaged");
    let result = loadout.package(&dest);
    assert!(
        result.is_ok(),
        "a file-to-directory collision across data roots must not abort packaging: {result:?}"
    );
    assert!(
        dest.join("Data/Stuff/inner.txt").exists(),
        "the higher root's directory should win, matching the shadowing rule"
    );
}

/// The reverse collision: a higher-priority root replaces a lower root's
/// directory with a plain file of the same name.
#[test]
fn package_handles_a_directory_becoming_a_file_across_roots() {
    let tmp = tempfile::tempdir().unwrap();
    let low = tmp.path().join("low");
    let high = tmp.path().join("high");
    write(&low.join("Stuff/inner.txt"), b"a folder named Stuff");
    write(&high.join("Stuff"), b"a loose file named Stuff");

    let input = LoadoutInput {
        game_dir: tmp.path().join("game"),
        data_roots: vec![low, high],
        plugins_txt: None,
        loadorder_txt: None,
        ini_dir: None,
        ccc: None,
    };
    let loadout = input.resolve().unwrap();
    let dest = tmp.path().join("packaged");
    let result = loadout.package(&dest);
    assert!(
        result.is_ok(),
        "a directory-to-file collision across data roots must not abort packaging: {result:?}"
    );
    let packaged = std::fs::read(dest.join("Data/Stuff"));
    assert_eq!(
        packaged.as_deref().ok(),
        Some(b"a loose file named Stuff".as_slice()),
        "the higher root's file should win, matching the shadowing rule"
    );
}
