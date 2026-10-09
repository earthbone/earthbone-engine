// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! WRLD parent LOD data controls distant water height independently of water data.

use std::{path::PathBuf, process::Command};

use soe_esm::{Entry, FileHeader, Group, GroupKind, Plugin, Record, tag};
use soe_godot_host::data_source::DataSource;
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel};
use soe_godot_host::units::METRES_PER_UNIT;

fn inherited_lod_water() -> (tempfile::TempDir, DataSource, soe_esm::GlobalFormId) {
    let directory = tempfile::tempdir().unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let mut worlds = Group::new(GroupKind::Top(tag::WRLD));
    let parent = Record::new(tag::WRLD, 0x100)
        .zsub(tag::EDID, "Parent")
        .sub(b"DATA", [0u8])
        .sub(
            b"DNAM",
            [0.0f32.to_le_bytes(), 900.0f32.to_le_bytes()].concat(),
        )
        .sub(b"NAM4", 700.0f32.to_le_bytes());
    worlds
        .entries
        .push(Entry::Record(plugin.push_record(parent)));
    let child = Record::new(tag::WRLD, 0x101)
        .zsub(tag::EDID, "Child")
        .sub(b"DATA", [0u8])
        .sub(
            b"DNAM",
            [0.0f32.to_le_bytes(), 500.0f32.to_le_bytes()].concat(),
        )
        .sub(b"WNAM", 0x100u32.to_le_bytes())
        .sub(b"PNAM", 0x0002u16.to_le_bytes());
    worlds
        .entries
        .push(Entry::Record(plugin.push_record(child)));
    let mut children = Group::new(GroupKind::WorldChildren(0x101));
    let mut block = Group::new(GroupKind::ExteriorBlock { x: 0, y: 0 });
    let mut sub = Group::new(GroupKind::ExteriorSubBlock { x: 0, y: 0 });
    let cell = Record::new(tag::CELL, 0x200)
        .zsub(tag::EDID, "OpenWater")
        .sub(b"DATA", 0x0002u16.to_le_bytes())
        .sub(b"XCLC", [0i32.to_le_bytes(), 0i32.to_le_bytes()].concat())
        .sub(b"XCLW", f32::MAX.to_le_bytes());
    sub.entries.push(Entry::Record(plugin.push_record(cell)));
    block.entries.push(Entry::Group(sub));
    children.entries.push(Entry::Group(block));
    worlds.entries.push(Entry::Group(children));
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
    let world = source.resolve_world("Child").unwrap().0;
    (directory, source, world)
}

#[test]
fn rnd_29_lod_water_uses_parent_nam4_when_pnam_uses_lod_data() {
    let (_directory, source, world) = inherited_lod_water();
    let surfaces = source
        .lod_water_surfaces(
            world,
            LodBlockCoord {
                level: LodLevel::L4,
                x: 0,
                y: 0,
            },
        )
        .unwrap();
    assert_eq!(surfaces.len(), 1);
    let expected = 700.0 * METRES_PER_UNIT;
    let observed = surfaces[0].arrays.positions[0][1];
    assert!(
        (observed - expected).abs() < 0.001,
        "PNAM Use LOD Data expects parent NAM4 {expected}, observed {observed}"
    );
}

#[test]
fn rnd_29_installed_capture_has_no_lod_overlap_or_boundary_gap() {
    let data = PathBuf::from(std::env::var_os("SKYRIM_DATA").expect("SKYRIM_DATA"));
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN");
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = workspace.join("godot");
    assert!(
        workspace
            .join("target/release/libsoe_godot_host.so")
            .is_file()
    );

    let source = DataSource::open(&data).unwrap();
    let order = source.shared_order();
    let cell = source.resolve_cell("WhiterunWatchtowerExterior").unwrap();
    let height = order
        .winners_of(soe_esm::tag::LAND)
        .find_map(|(id, record)| {
            let owner = order.winner_ref(id)?;
            let raw_cell = record.placement.cell?;
            if order.resolve(owner.file, raw_cell) != cell {
                return None;
            }
            let land = soe_esm::records::land::Land::from_record(record).ok()?;
            Some(land.heights?.decode()[16 * 33 + 16])
        })
        .expect("watchtower plains have LAND");
    let eye_height = height * METRES_PER_UNIT + 1.7;
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "rnd_29_terrain_join_capture.gd",
        ])
        .env("SKYRIM_DATA", data)
        .env("SOE_RND29_EYE_HEIGHT", eye_height.to_string())
        .output()
        .expect("Godot capture starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout
                .contains("RESULT: PASS: RND-29 terrain joins loaded cells without overlap or gap"),
        "RND-29 capture failed: {stdout}\n{stderr}"
    );
    for image in [
        "rnd-29-plains.png",
        "rnd-29-coverage.png",
        "rnd-29-standing-plains.png",
    ] {
        assert!(
            project.join(".godot").join(image).is_file(),
            "missing {image}"
        );
    }
}
