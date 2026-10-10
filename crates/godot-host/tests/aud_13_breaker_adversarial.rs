// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AUD-13 rejects broken music links and keeps audio alive across the installed transition.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_godot_host::audio::music_type_for_cell;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world(cell_music: Option<u32>, region_music: Option<u32>, world_music: Option<u32>) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut cell = Record::new(Tag(*b"CELL"), 0x800).sub(Tag(*b"DATA"), [0]);
    cell.placement.world = Some(0x900);
    if let Some(music) = cell_music {
        cell = cell.sub(Tag(*b"XCMO"), music.to_le_bytes());
    }
    if region_music.is_some() {
        cell = cell.sub(Tag(*b"XCLR"), 0x801_u32.to_le_bytes());
    }
    plugin.push_top(cell);
    let mut world = Record::new(Tag(*b"WRLD"), 0x900);
    if let Some(music) = world_music {
        world = world.sub(Tag(*b"ZNAM"), music.to_le_bytes());
    }
    plugin.push_top(world);
    if let Some(music) = region_music {
        plugin.push_top(
            Record::new(Tag(*b"REGN"), 0x801)
                .sub(Tag(*b"RDAT"), [7, 0, 0, 0, 0, 10, 0, 0])
                .sub(Tag(*b"RDMO"), music.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Aud13Breaker.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn aud_13_breaker_dangling_cell_music_link_is_rejected() {
    assert!(music_type_for_cell(&world(Some(0xBAD), None, Some(0xA00)), id(0x800)).is_err());
}

#[test]
fn aud_13_breaker_cell_music_link_must_resolve_to_musc() {
    let wrong_type = Record::new(Tag(*b"SNDR"), 0xA00);
    let mut plugin = Plugin::new(FileHeader::default());
    let mut cell = Record::new(Tag(*b"CELL"), 0x800).sub(Tag(*b"DATA"), [0]);
    cell.placement.world = Some(0x900);
    cell = cell.sub(Tag(*b"XCMO"), 0xA00_u32.to_le_bytes());
    plugin.push_top(cell);
    plugin.push_top(Record::new(Tag(*b"WRLD"), 0x900));
    plugin.push_top(wrong_type);
    let mut order = LoadOrder::new();
    order.add("Aud13WrongCellMusic.esm", plugin).unwrap();
    assert!(music_type_for_cell(&World::new(order), id(0x800)).is_err());
}

#[test]
fn aud_13_breaker_dangling_region_music_link_is_rejected() {
    assert!(music_type_for_cell(&world(None, Some(0xBAD), Some(0xA00)), id(0x800)).is_err());
}

#[test]
fn aud_13_breaker_dangling_world_music_link_is_rejected() {
    assert!(music_type_for_cell(&world(None, None, Some(0xBAD)), id(0x800)).is_err());
}

#[test]
fn aud_13_breaker_installed_bannered_mare_to_plains_audio_survives_twenty_seconds() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        panic!("SOE_GODOT_BIN must be set for the installed scene check");
    };
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let extension = workspace.join("target/release/libsoe_godot_host.so");
    assert!(extension.is_file(), "build the release extension before this check");
    let output = std::process::Command::new(godot)
        .arg("--path")
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360", "--script", "res://aud_13_transition_test.gd"])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot transition scene starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("AUD-13 RESULT: PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "installed transition failed: status={}\nstdout={}\nstderr={}",
        output.status,
        stdout,
        stderr
    );
}
