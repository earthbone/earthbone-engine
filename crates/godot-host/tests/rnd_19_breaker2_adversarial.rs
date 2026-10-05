// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-19 mixed WRLD parent flags and malformed LOD inheritance cases.

use soe_esm::{tag, Entry, FileHeader, GlobalFormId, Group, GroupKind, Plugin, Record};
use soe_godot_host::data_source::DataSource;
use std::path::PathBuf;
use std::process::Command;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn source(child_flags: u16, grandchild_parent: u32) -> (tempfile::TempDir, DataSource) {
    let directory = tempfile::tempdir().unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let mut worlds = Group::new(GroupKind::Top(tag::WRLD));
    for record in [
        Record::new(tag::WRLD, 0x100).zsub(tag::EDID, "Root"),
        Record::new(tag::WRLD, 0x101)
            .zsub(tag::EDID, "Middle")
            .sub(b"WNAM", 0x100u32.to_le_bytes())
            .sub(b"PNAM", child_flags.to_le_bytes()),
        Record::new(tag::WRLD, 0x102)
            .zsub(tag::EDID, "Leaf")
            .sub(b"WNAM", grandchild_parent.to_le_bytes())
            .sub(b"PNAM", 2u16.to_le_bytes()),
    ] {
        worlds
            .entries
            .push(Entry::Record(plugin.push_record(record)));
    }
    plugin.groups.push(worlds);
    std::fs::write(directory.path().join("Skyrim.esm"), plugin.to_bytes()).unwrap();
    for name in ["Update.esm", "Dawnguard.esm", "HearthFires.esm", "Dragonborn.esm"] {
        std::fs::write(
            directory.path().join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
    let source = DataSource::open(directory.path()).unwrap();
    (directory, source)
}

#[test]
fn rnd_19_each_parent_must_opt_in_to_lod_inheritance() {
    let (_directory, source) = source(0, 0x101);
    assert_eq!(source.lod_worldspace(id(0x102)), Some((id(0x101), "Middle".into())));
}

#[test]
fn rnd_19_use_land_alone_does_not_continue_the_lod_parent_chain() {
    let (_directory, source) = source(1, 0x101);
    assert_eq!(source.lod_worldspace(id(0x102)), Some((id(0x101), "Middle".into())));
}

#[test]
fn rnd_19_missing_parent_record_cannot_supply_lod_assets() {
    let (_directory, source) = source(2, 0x999);
    assert_eq!(source.lod_worldspace(id(0x102)), None);
}

#[test]
fn rnd_19_installed_gameplay_capture_shows_terrain_and_tracks_lod() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let data = PathBuf::from(
        "/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
    );
    let godot = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from("/run/media/pengolodh/linux-data-games/tmp/soe-run/bin/godot")
    });
    assert!(data.join("Skyrim.esm").is_file(), "installed Skyrim data missing");
    let capture = project.join(".godot/rnd-19-whiterun-exterior-lod.png");
    let output = Command::new(godot)
        .args(["--path"])
        .arg(&project)
        .args(["--resolution", "640x360", "--script"])
        .arg(project.join("rnd_19_player_lod_capture.gd"))
        .env("SKYRIM_DATA", &data)
        .env("SOE_RND19_CAPTURE_PATH", &capture)
        .output()
        .expect("Godot capture process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("RESULT: PASS: PlayerNode streams terrain LOD around Whiterun")
            && capture.is_file(),
        "Godot RND-19 capture failed\nstdout tail:\n{}\nstderr tail:\n{}",
        stdout.lines().rev().take(30).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n"),
        stderr.lines().rev().take(30).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n"),
    );
}
