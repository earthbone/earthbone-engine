// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Whiterun loaded-cell handoff, following xEdit WRLD flags and OpenMW's active grid.

use soe_esm::{records::refr::Refr, tag};
use soe_godot_host::data_source::DataSource;
use std::path::Path;

#[cfg(feature = "godot")]
#[path = "common/isolated_project.rs"]
mod isolated_project;

#[cfg(feature = "godot")]
#[test]
fn loaded_empty_child_cell_releases_object_lod_before_the_next_update() {
    use soe_esm::{Entry, FileHeader, Group, GroupKind, Plugin, Record};
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let library = root.join("target/release/libsoe_godot_host.so");
    let project = isolated_project::project(&["whiterun_lod_residency_test.gd"], Some(&library));
    let data = tempfile::tempdir().unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let mut worlds = Group::new(GroupKind::Top(tag::WRLD));
    for (object, name) in [(0x100u32, "Parent"), (0x200, "Child")] {
        let mut world = Record::new(tag::WRLD, object).zsub(tag::EDID, name);
        if object == 0x200 {
            world = world
                .sub(b"WNAM", 0x100u32.to_le_bytes())
                .sub(b"PNAM", 2u16.to_le_bytes());
        }
        worlds
            .entries
            .push(Entry::Record(plugin.push_record(world)));
    }
    let mut children = Group::new(GroupKind::WorldChildren(0x200));
    let mut block = Group::new(GroupKind::ExteriorBlock { x: 0, y: 0 });
    let mut sub = Group::new(GroupKind::ExteriorSubBlock { x: 1, y: 0 });
    sub.entries.push(Entry::Record(
        plugin.push_record(
            Record::new(tag::CELL, 0x201)
                .zsub(tag::EDID, "EmptyChildCell")
                .sub(b"DATA", [0u8, 0])
                .sub(
                    b"XCLC",
                    [8i32, 0, 0]
                        .into_iter()
                        .flat_map(i32::to_le_bytes)
                        .collect::<Vec<_>>(),
                ),
        ),
    ));
    block.entries.push(Entry::Group(sub));
    children.entries.push(Entry::Group(block));
    worlds.entries.push(Entry::Group(children));
    plugin.groups.push(worlds);
    std::fs::write(data.path().join("Skyrim.esm"), plugin.to_bytes()).unwrap();
    for name in [
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            data.path().join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
    let objects = data.path().join("meshes/terrain/parent/objects");
    std::fs::create_dir_all(&objects).unwrap();
    let mesh = soe_nif::Mesh {
        name: "resident handoff".into(),
        block: 0,
        transform: soe_nif::Transform::IDENTITY,
        bound: soe_nif::Bound {
            center: soe_nif::Vec3::new(0.0, 0.0, 0.0),
            radius: 100.0,
        },
        positions: vec![
            [32768.0, 0.0, 0.0],
            [32868.0, 0.0, 0.0],
            [32768.0, 100.0, 0.0],
        ],
        normals: vec![],
        tangents: vec![],
        uv_sets: vec![],
        uvs: vec![],
        colors: vec![],
        indices: vec![0, 1, 2],
        bone_weights: vec![],
        bone_indices: vec![],
        skin: None,
        material: soe_nif::mesh::Material::default(),
    };
    std::fs::write(
        objects.join("parent.4.8.0.bto"),
        soe_nif::static_mesh::export_nif(&[mesh]).unwrap(),
    )
    .unwrap();
    let output = std::process::Command::new(godot)
        .arg("--headless")
        .arg("--path")
        .arg(project.path())
        .args(["--script", "whiterun_lod_residency_test.gd"])
        .env("SOE_LOD_FIXTURE", data.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("LOD_RESIDENCY_RESULT PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "{stdout}\n{stderr}"
    );
    eprintln!("{stdout}");
}

#[test]
fn whiterun_lod_installed_record_flags() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&data), 1).unwrap();
    let (city, _) = source.resolve_world("WhiterunWorld").unwrap();
    let order = source.shared_order();
    let owner = order.winner_ref(city).unwrap();
    eprintln!(
        "WHITERUN_RECORD world={:?}",
        soe_esm::records::wrld::World::from_record(order.record(owner), false).unwrap()
    );
    for (cell, _) in source.cell_index().cells() {
        let Some(grid) = source
            .cell_index()
            .grid_of(cell)
            .filter(|g| g.world == city)
        else {
            continue;
        };
        let record = order.winner(cell).unwrap();
        let view = soe_esm::records::cell::Cell::from_record(record, false).unwrap();
        eprintln!(
            "WHITERUN_RECORD cell={cell} grid={},{} flags={:?}",
            grid.x, grid.y, view.grid
        );
        for &id in source.cell_index().references(cell) {
            let owner = order.winner_ref(id).unwrap();
            let record = order.record(owner);
            if record.tag != tag::REFR {
                continue;
            }
            let reference = Refr::from_record(record).unwrap();
            let base = order
                .winner(order.resolve(owner.file, reference.base))
                .unwrap();
            let model = base
                .find(soe_esm::Tag::new(b"MODL"))
                .map(soe_esm::cursor::zstring)
                .unwrap_or_default();
            if ["water", "pond", "canal", "walkway", "bridge"]
                .iter()
                .any(|s| model.to_ascii_lowercase().contains(s))
            {
                eprintln!(
                    "WHITERUN_RECORD water={id} base={} model={model} position={:?}",
                    base.editor_id().unwrap_or_default(),
                    reference.transform
                );
            }
        }
    }
}

#[test]
fn whiterun_lod_installed_coverage_survey() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&data), 1).unwrap();
    let (city, _) = source.resolve_world("WhiterunWorld").unwrap();
    let order = source.shared_order();
    eprintln!("WHITERUN_LOD parent={:?}", source.lod_worldspace(city));
    for (cell, _) in source.cell_index().cells() {
        let owner = order.winner_ref(cell).unwrap();
        let record = order.record(owner);
        if record
            .placement
            .world
            .map(|raw| order.resolve(owner.file, raw))
            != Some(city)
        {
            continue;
        }
        for &id in source.cell_index().references(cell) {
            let owner = order.winner_ref(id).unwrap();
            let record = order.record(owner);
            if record.tag != tag::REFR {
                continue;
            }
            let reference = Refr::from_record(record).unwrap();
            let base = order
                .winner(order.resolve(owner.file, reference.base))
                .unwrap();
            let model = base
                .find(soe_esm::Tag::new(b"MODL"))
                .map(soe_esm::cursor::zstring)
                .unwrap_or_default();
            if model.to_ascii_lowercase().contains("lod") {
                eprintln!(
                    "WHITERUN_LOD placed={id} base={} model={model} position={:?} flags={:x}",
                    base.editor_id().unwrap_or_default(),
                    reference.transform,
                    record.flags
                );
            }
        }
    }
    let loaded = source.stream_exterior("WhiterunOrigin", 2).unwrap();
    let mut near = std::collections::HashMap::new();
    for (cell, loaded) in &loaded.cells {
        for surface in &loaded.surfaces {
            if surface
                .arrays
                .material
                .diffuse
                .as_deref()
                .is_some_and(|p| p.starts_with("terrain:"))
            {
                near.insert(
                    soe_godot_host::streaming::CellCoord::new(cell.x, cell.y),
                    surface
                        .arrays
                        .positions
                        .iter()
                        .map(|p| p[1])
                        .collect::<Vec<_>>(),
                );
            }
        }
    }
    let (parent, name) = source.lod_worldspace(city).unwrap();
    let mut raw_overlap = 0;
    let mut joined_overlap = 0;
    let mut above = 0;
    let mut highest = 0.0f32;
    for block in source.scenery_lod_blocks(&name, soe_godot_host::terrain_lod::LodKind::Terrain) {
        if !near.keys().any(|cell| block.ring_distance(*cell) == 0) {
            continue;
        }
        let lod = source
            .load_lod_block(parent, &name, block)
            .unwrap()
            .unwrap();
        for surface in lod
            .surfaces
            .iter()
            .filter(|s| s.arrays.material.water.is_none())
        {
            let joined =
                soe_godot_host::terrain_lod_join::join_surface(&surface.arrays, block, &near)
                    .unwrap();
            for (arrays, clipped) in [(&surface.arrays, false), (&joined, true)] {
                for t in arrays.indices.as_chunks::<3>().0 {
                    let point: [f32; 3] = std::array::from_fn(|axis| {
                        t.iter()
                            .map(|i| arrays.positions[*i as usize][axis])
                            .sum::<f32>()
                            / 3.0
                    });
                    let x = point[0] / soe_godot_host::units::CELL_METRES;
                    let y = -point[2] / soe_godot_host::units::CELL_METRES;
                    let cell = soe_godot_host::streaming::CellCoord::new(
                        x.floor() as i32,
                        y.floor() as i32,
                    );
                    if let Some(heights) = near.get(&cell) {
                        if clipped {
                            joined_overlap += 1;
                        } else {
                            raw_overlap += 1;
                            let c = (x.fract() * 32.0).clamp(0.0, 32.0) as usize;
                            let r = ((y - y.floor()) * 32.0).clamp(0.0, 32.0) as usize;
                            let excess = point[1] - heights[r * 33 + c];
                            if excess > 0.001 {
                                above += 1;
                                highest = highest.max(excess);
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!("WHITERUN_LOD terrain cells={} raw_overlap={raw_overlap} above={above} highest_metres={highest} joined_overlap={joined_overlap}", near.len());
    assert!(raw_overlap > 0);
    assert_eq!(joined_overlap, 0);
    for (cell, loaded) in &loaded.cells {
        for surface in &loaded.surfaces {
            if surface
                .arrays
                .material
                .diffuse
                .as_deref()
                .is_some_and(|p| p.starts_with("terrain:"))
                || surface.arrays.material.shader_flags_2 & soe_nif::blocks::flags2::LOD_LANDSCAPE
                    != 0
            {
                eprintln!(
                    "WHITERUN_LOD grid={},{} ref={} vertices={} bounds={:?} diffuse={:?}",
                    cell.x,
                    cell.y,
                    surface.reference,
                    surface.arrays.positions.len(),
                    surface.arrays.bounds,
                    surface.arrays.material.diffuse
                );
            }
        }
    }
}

#[cfg(feature = "godot")]
#[test]
fn whiterun_lod_installed_plains_and_walls_have_no_resident_overlap() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = root.join("target/whiterun-lod");
    std::fs::create_dir_all(&directory).unwrap();
    let source = DataSource::open_with_threads(Path::new(&data), 1).unwrap();
    let (city, _) = source.resolve_world("WhiterunWorld").unwrap();
    let parent = source.lod_worldspace(city).unwrap().0;
    let order = source.shared_order();
    let mut references = std::collections::BTreeMap::new();
    let mut exterior_cells = std::collections::BTreeMap::new();
    for (cell, _) in source.cell_index().cells() {
        let owner = order.winner_ref(cell).unwrap();
        let world = order
            .record(owner)
            .placement
            .world
            .map(|raw| order.resolve(owner.file, raw));
        if world != Some(city) && world != Some(parent) {
            continue;
        }
        if let Some(grid) = source.cell_index().grid_of(cell) {
            exterior_cells.insert(cell.to_string(), [grid.x, grid.y]);
        }
        for &id in source.cell_index().references(cell) {
            let record = order.winner(id).unwrap();
            if record.tag != tag::REFR {
                continue;
            }
            if let Some(transform) = Refr::from_record(record).unwrap().transform {
                let x = (transform.position[0] / soe_godot_host::units::CELL_UNITS).floor() as i32;
                let y = (transform.position[1] / soe_godot_host::units::CELL_UNITS).floor() as i32;
                if world == Some(parent) && (x.abs_diff(5) > 10 || y.abs_diff(-2) > 10) {
                    continue;
                }
                references.insert(
                    id.to_string(),
                    [
                        (transform.position[0] / soe_godot_host::units::CELL_UNITS).floor() as i32,
                        (transform.position[1] / soe_godot_host::units::CELL_UNITS).floor() as i32,
                    ],
                );
            }
        }
    }
    let manifest = directory.join("references.json");
    let full_lod_references: std::collections::BTreeMap<_, _> = source
        .distant_candidates(parent)
        .iter()
        .map(|candidate| {
            let record = order.winner(candidate.id).unwrap();
            let reference = Refr::from_record(record).unwrap();
            (
                candidate.id.to_string(),
                reference.transform.unwrap().position,
            )
        })
        .collect();
    std::fs::write(
        &manifest,
        serde_json::to_vec(&serde_json::json!({
            "references": references,
            "cells": exterior_cells,
            "full_lod_references": full_lod_references,
        }))
        .unwrap(),
    )
    .unwrap();
    drop(order);
    drop(source);
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN required");
    let imported = std::process::Command::new(&godot)
        .current_dir(&root)
        .args(["--headless", "--editor", "--path", "godot", "--quit"])
        .env("SOE_GODOT_TIMEOUT", "900")
        .output()
        .unwrap();
    std::fs::write(
        directory.join("import.log"),
        [imported.stdout, imported.stderr].concat(),
    )
    .unwrap();
    assert!(
        imported.status.success(),
        "Godot import failed, see target/whiterun-lod/import.log"
    );
    let log = std::fs::File::create(directory.join("process.log")).unwrap();
    let status = std::process::Command::new(godot)
        .current_dir(&root)
        .args([
            "--path",
            "godot",
            "--resolution",
            "640x360",
            "--script",
            "whiterun_lod_test.gd",
        ])
        .arg("--log-file")
        .arg(directory.join("godot.log"))
        .env("SOE_WHITERUN_LOD_REFERENCES", manifest)
        .env("SOE_GODOT_TIMEOUT", "900")
        .env("SOE_LOG", "error")
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .status()
        .unwrap();
    let output = std::fs::read_to_string(directory.join("process.log")).unwrap();
    for line in output
        .lines()
        .filter(|line| line.starts_with("WHITERUN_LOD"))
    {
        eprintln!("{line}");
    }
    assert!(
        status.success()
            && !output.contains("SCRIPT ERROR")
            && [
                "plains",
                "pond",
                "walls",
                "outside-walls",
                "full-lod-handoff",
                "walking"
            ]
            .iter()
            .all(|label| output.contains(&format!("WHITERUN_LOD_COVERAGE {label} position=")))
            && output.contains("WHITERUN_LOD_RESULT PASS"),
        "Godot {status}, see target/whiterun-lod/process.log"
    );
}
