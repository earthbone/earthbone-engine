// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-27 adversarial checks for WRLD flags and the installed Whiterun view.

use std::path::PathBuf;
use std::process::Command;

use soe_esm::{tag, Entry, FileHeader, GlobalFormId, Group, GroupKind, Plugin, Record};
use soe_godot_host::data_source::DataSource;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn source(
    flags: Option<u8>,
    parent_flags: Option<u16>,
    parent: Option<u32>,
) -> (tempfile::TempDir, DataSource) {
    let directory = tempfile::tempdir().unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let mut worlds = Group::new(GroupKind::Top(tag::WRLD));
    let root = Record::new(tag::WRLD, 0x100)
        .zsub(tag::EDID, "Parent")
        .sub(b"DATA", [0]);
    worlds.entries.push(Entry::Record(plugin.push_record(root)));
    let mut child = Record::new(tag::WRLD, 0x101).zsub(tag::EDID, "Child");
    if let Some(flags) = flags {
        child = child.sub(b"DATA", [flags]);
    }
    if let Some(parent) = parent {
        child = child.sub(b"WNAM", parent.to_le_bytes());
    }
    if let Some(parent_flags) = parent_flags {
        child = child.sub(b"PNAM", parent_flags.to_le_bytes());
    }
    worlds
        .entries
        .push(Entry::Record(plugin.push_record(child)));
    worlds.entries.push(Entry::Record(
        plugin.push_record(Record::new(tag::CELL, 0x102).zsub(b"EDID", "NotAWorld")),
    ));
    plugin.groups.push(worlds);
    std::fs::write(directory.path().join("Skyrim.esm"), plugin.to_bytes()).unwrap();
    for name in [
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
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
fn rnd_27_small_world_uses_only_the_wrld_data_bit_zero() {
    for (flags, expected) in [
        (0x00, false),
        (0x02, false),
        (0x80, false),
        (0x01, true),
        (0xff, true),
    ] {
        let (_directory, source) = source(Some(flags), Some(0), Some(0x100));
        assert_eq!(
            source.is_small_world(id(0x101)),
            expected,
            "DATA flags {flags:#04x}"
        );
    }
}

#[test]
fn rnd_27_missing_wrong_type_and_malformed_worlds_do_not_enable_small_world_mode() {
    let (_directory, loaded) = source(Some(0x01), Some(0), Some(0x100));
    assert!(!loaded.is_small_world(id(0xdead)));
    assert!(!loaded.is_small_world(id(0x102)));
    let (_directory, missing_data) = source(None, Some(0), Some(0x100));
    assert!(!missing_data.is_small_world(id(0x101)));
}

#[test]
fn rnd_27_parent_lod_and_land_bits_are_independent_through_nested_worlds() {
    for (flags, expected) in [
        (0, (0x101, "Child")),
        (1, (0x101, "Child")),
        (2, (0x100, "Parent")),
        (3, (0x100, "Parent")),
    ] {
        let (_directory, source) = source(Some(1), Some(flags), Some(0x100));
        let actual = source.lod_worldspace(id(0x101)).unwrap();
        assert_eq!(
            (actual.0.object, actual.1.as_str()),
            expected,
            "PNAM flags {flags}"
        );
    }
    let (_directory, source) = source(Some(1), Some(2), Some(0x101));
    assert_eq!(
        source.lod_worldspace(id(0x101)),
        None,
        "self-parent cycle must not resolve"
    );
}

#[test]
fn rnd_27_installed_whiterun_capture_shows_parent_plains_and_city_silhouette() {
    let data = std::env::var_os("SKYRIM_DATA").expect("installed Skyrim data for RND-27");
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("Godot 4.4 for RND-27 capture");
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "rnd_27_whiterun_capture.gd",
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("start installed RND-27 capture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RND-27 RESULT: PASS"),
        "RND-27 capture failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    for capture in [
        "dragonsreach-plains.png",
        "dragonsreach-no-terrain-lod.png",
        "outside-gate-city.png",
        "outside-gate-no-city.png",
    ] {
        assert!(
            project.join(".godot/rnd_27").join(capture).is_file(),
            "missing {capture}"
        );
    }
}
