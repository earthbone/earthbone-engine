// SPDX-License-Identifier: GPL-3.0-or-later

//! Persistent full models, using local UESP REFR and xEdit STAT header flags.

use super::*;
use soe_esm::{Entry, FileHeader, Group, GroupKind, Plugin};
use std::collections::{BTreeMap, BTreeSet};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn marker_only_asset(source: &DataSource, model: &str, cache: &mut HashMap<String, bool>) -> bool {
    *cache.entry(soe_bsa::normalize(model)).or_insert_with(|| {
        let bytes = source.vfs.read(&format!("meshes/{model}")).unwrap();
        let file = soe_nif::parse(&bytes).unwrap();
        let meshes = file.meshes().unwrap();
        !meshes.is_empty()
            && meshes.iter().all(|mesh| {
                let name = mesh.name.to_ascii_lowercase();
                name.starts_with("editormarker") || name.starts_with("visibilityeditormarker")
            })
    })
}

fn gate_landing(source: &DataSource) -> (GlobalFormId, GlobalFormId, [f32; 3]) {
    let city = source
        .exterior_world(source.resolve_cell("WhiterunOrigin").unwrap())
        .unwrap();
    let mut cells: Vec<_> = source
        .cell_index()
        .cells()
        .map(|(cell, _)| cell)
        .filter(|cell| source.exterior_world(*cell) == Some(city))
        .collect();
    cells.sort_unstable();
    for cell in cells {
        for &id in source.cell_index().references(cell) {
            let owner = source.order.winner_ref(id).unwrap();
            let record = source.order.record(owner);
            if record.tag != tag::REFR
                || !soe_game::World::authored_reference_enabled(&source.order, id)
            {
                continue;
            }
            let reference = Refr::from_record(record).unwrap();
            let Some(teleport) = reference.teleport.filter(|_| reference.lock.is_none()) else {
                continue;
            };
            let far = source.order.resolve(owner.file, teleport.door);
            let far_owner = source.order.winner_ref(far).unwrap();
            let far_record = source.order.record(far_owner);
            let far_cell = source
                .order
                .resolve(far_owner.file, far_record.placement.cell.unwrap());
            let Some(world) = source.exterior_world(far_cell) else {
                continue;
            };
            if source.order.winner(world).unwrap().editor_id().as_deref() == Some("Tamriel") {
                return (world, far_cell, teleport.destination.position);
            }
        }
    }
    panic!("Whiterun gate to Tamriel missing");
}

fn fixture() -> (tempfile::TempDir, DataSource) {
    let directory = tempfile::tempdir().unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    for (object, kind, flags) in [
        (0x900, tag::STAT, 0),
        (0x901, tag::STAT, 4),
        (0x902, Tag::new(b"MISC"), 4),
    ] {
        let mut base = Record::new(kind, object).zsub(b"MODL", "missing.nif");
        base.flags = flags;
        plugin.push_top(base);
    }
    let mut top = Group::new(GroupKind::Top(tag::WRLD));
    for world in [0x100u32, 0x200] {
        top.entries.push(Entry::Record(plugin.push_record(
            Record::new(tag::WRLD, world).zsub(tag::EDID, &format!("World{world}")),
        )));
        let mut children = Group::new(GroupKind::WorldChildren(world));
        children.entries.push(Entry::Record(
            plugin.push_record(Record::new(tag::CELL, world + 1).sub(b"DATA", [0, 0])),
        ));
        let mut persistent = Group::new(GroupKind::CellPersistent(world + 1));
        for (offset, base, flags, x) in [
            (0, 0x900u32, 0x400, 0.0),
            (1, 0x900, 0x10400, 0.0),
            (2, 0x901, 0x400, 20.0),
            (3, 0x901, 0x10400, -20.0),
            (4, 0x902, 0x400, 0.0),
        ] {
            let mut reference = Record::new(tag::REFR, world + 0x10 + offset)
                .sub(b"NAME", base.to_le_bytes())
                .sub(
                    b"DATA",
                    [x * crate::units::CELL_UNITS, 0.0, 0.0, 0.0, 0.0, 0.0]
                        .into_iter()
                        .flat_map(f32::to_le_bytes)
                        .collect::<Vec<_>>(),
                );
            reference.flags = flags;
            persistent
                .entries
                .push(Entry::Record(plugin.push_record(reference)));
        }
        let mut cell = Group::new(GroupKind::CellChildren(world + 1));
        cell.entries.push(Entry::Group(persistent));
        children.entries.push(Entry::Group(cell));
        let mut block = Group::new(GroupKind::ExteriorBlock { x: 0, y: 0 });
        let mut sub = Group::new(GroupKind::ExteriorSubBlock { x: 0, y: 0 });
        sub.entries.push(Entry::Record(
            plugin.push_record(
                Record::new(tag::CELL, world + 2)
                    .zsub(tag::EDID, &format!("Grid{world}"))
                    .sub(b"DATA", [0, 0])
                    .sub(b"XCLC", [0; 12]),
            ),
        ));
        block.entries.push(Entry::Group(sub));
        children.entries.push(Entry::Group(block));
        top.entries.push(Entry::Group(children));
    }
    plugin.groups.push(top);
    std::fs::write(directory.path().join("Skyrim.esm"), plugin.to_bytes()).unwrap();
    for name in &BASE_MASTERS[1..] {
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
fn persistent_full_lod_residency_is_world_scoped_and_disjoint_from_grid() {
    let (_directory, source) = fixture();
    for world in [0x100, 0x200] {
        assert_eq!(
            source.presentation_references(id(world + 1)),
            &[id(world + 0x11), id(world + 0x12), id(world + 0x13)]
        );
        assert_eq!(
            source.presentation_references(id(world + 2)),
            &[id(world + 0x10), id(world + 0x14)]
        );
        let loaded = source.load_location(id(world + 2), [0.0; 3], 0).unwrap();
        assert_eq!(loaded.cells.len(), 2);
        assert_eq!(
            loaded
                .cells
                .iter()
                .map(|(_, cell)| cell.stats.meshes_failed)
                .sum::<usize>(),
            5
        );
        assert_eq!(source.cell_index().references(id(world + 1)).len(), 5);
        assert_eq!(source.physical_references(id(world + 1)), &[]);
        assert_eq!(
            source.physical_references(id(world + 2)),
            &[id(world + 0x10), id(world + 0x11), id(world + 0x14)]
        );
        assert_eq!(loaded.collision.stats.references, 3);
    }
}

#[test]
fn persistent_full_lod_installed_survey() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKIP persistent_full_lod_installed_survey: SKYRIM_DATA unset");
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let (gate_world, _, gate_position) = gate_landing(&source);
    let gate_x = (gate_position[0] / crate::units::CELL_UNITS).floor() as i32;
    let gate_y = (gate_position[1] / crate::units::CELL_UNITS).floor() as i32;
    eprintln!("FULL_LOD_LANDING {gate_position:?} grid={gate_x},{gate_y}");
    let index = source.cell_index();
    let mut worlds = BTreeMap::new();
    for (persistent, _) in index.cells() {
        let Some(world) = index.persistent_cell_world(persistent) else {
            continue;
        };
        worlds.insert(world, persistent);
    }
    for (world, persistent) in worlds {
        let name = source.order.winner(world).unwrap().editor_id().unwrap();
        let mut full = 0;
        let mut never = 0;
        let mut both = 0;
        let mut rows = Vec::new();
        for &id in index.references(persistent) {
            let owner = source.order.winner_ref(id).unwrap();
            let record = source.order.record(owner);
            if record.tag != tag::REFR {
                continue;
            }
            let reference = Refr::from_record(record).unwrap();
            if !reference.is_persistent() {
                continue;
            }
            let base_id = source.order.resolve(owner.file, reference.base);
            let base = source.order.winner(base_id).unwrap();
            let is_full = record.flags & 0x0001_0000 != 0;
            let is_never = base.tag == tag::STAT
                && base.flags & soe_esm::records::stat::flags::NEVER_FADES != 0;
            full += usize::from(is_full);
            never += usize::from(is_never);
            both += usize::from(is_full && is_never);
            if !is_full && !is_never {
                continue;
            }
            if world != gate_world {
                continue;
            }
            let position = reference.transform.unwrap().position;
            let x = (position[0] / crate::units::CELL_UNITS).floor() as i32;
            let y = (position[1] / crate::units::CELL_UNITS).floor() as i32;
            if x.abs_diff(gate_x) > 20 || y.abs_diff(gate_y) > 20 {
                continue;
            }
            let selected = source.presentation_references(persistent).contains(&id)
                || (x.abs_diff(gate_x) <= 2
                    && y.abs_diff(gate_y) <= 2
                    && index
                        .exterior_cell(world, x, y)
                        .is_some_and(|cell| source.presentation_references(cell).contains(&id)));
            rows.push((id, format!(
                "{id} base={} grid={x},{y} full={is_full} never={is_never} enabled={} model={} gate_selected={selected}",
                base.editor_id().unwrap_or_default(),
                soe_game::World::authored_reference_enabled(&source.order, id),
                models::render_model_for(base).is_some(),
            )));
        }
        eprintln!(
            "FULL_LOD_WORLD {world} {name} full={full} never={never} both={both} union={}",
            full + never - both
        );
        rows.sort_by_key(|row| row.0);
        for (_, row) in rows {
            eprintln!("FULL_LOD_GATE {row}");
        }
    }
}

#[cfg(feature = "godot")]
#[test]
fn persistent_full_lod_installed_whiterun_draws_and_releases() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!(
            "SKIP persistent_full_lod_installed_whiterun_draws_and_releases: SKYRIM_DATA unset"
        );
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let (world, _, position) = gate_landing(&source);
    let x = (position[0] / crate::units::CELL_UNITS).floor() as i32;
    let y = (position[1] / crate::units::CELL_UNITS).floor() as i32;
    let persistent = source.cell_index().world_persistent_cell(world).unwrap();
    let mut expected = BTreeSet::new();
    let mut near = BTreeSet::new();
    let mut markers = BTreeSet::new();
    let mut marker_assets = HashMap::new();
    for &id in source.cell_index().references(persistent) {
        let owner = source.order.winner_ref(id).unwrap();
        let record = source.order.record(owner);
        if record.tag != tag::REFR {
            continue;
        }
        let reference = Refr::from_record(record).unwrap();
        let base = source
            .order
            .winner(source.order.resolve(owner.file, reference.base))
            .unwrap();
        if reference.is_persistent()
            && (record.flags & 0x0001_0000 != 0 || (base.tag == tag::STAT && base.flags & 4 != 0))
        {
            assert!(
                source.presentation_references(persistent).contains(&id),
                "full model not world-resident: {id}"
            );
        }
        if !reference.is_persistent()
            || (record.flags & 0x0001_0000 == 0 && !(base.tag == tag::STAT && base.flags & 4 != 0))
            || !soe_game::World::authored_reference_enabled(&source.order, id)
            || models::render_model_for(base).is_none()
        {
            continue;
        }
        let model = models::render_model_for(base).unwrap();
        if marker_only_asset(&source, &model, &mut marker_assets) {
            markers.insert(id.to_string());
        } else {
            expected.insert(id.to_string());
        }
        let p = reference.transform.unwrap().position;
        let rx = (p[0] / crate::units::CELL_UNITS).floor() as i32;
        let ry = (p[1] / crate::units::CELL_UNITS).floor() as i32;
        if rx.abs_diff(x) <= 20 && ry.abs_diff(y) <= 20 && expected.contains(&id.to_string()) {
            near.insert(id.to_string());
        }
    }
    assert!(!near.is_empty());
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let directory = root.join("target/full-lod");
    std::fs::create_dir_all(&directory).unwrap();
    let manifest = directory.join("expected.json");
    std::fs::write(
        &manifest,
        serde_json::to_vec(&serde_json::json!({"all": expected, "near": near, "markers": markers}))
            .unwrap(),
    )
    .unwrap();
    drop(source);
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN must name the test Godot");
    let output = std::process::Command::new(godot)
        .current_dir(root)
        .args([
            "--path",
            "godot",
            "--resolution",
            "640x360",
            "--script",
            "persistent_full_lod_test.gd",
        ])
        .env("SOE_FULL_LOD_EXPECT", &manifest)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    std::fs::write(directory.join("stdout.log"), stdout.as_bytes()).unwrap();
    std::fs::write(directory.join("stderr.log"), stderr.as_bytes()).unwrap();
    for line in stdout.lines().filter(|line| line.starts_with("FULL_LOD")) {
        eprintln!("{line}");
    }
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
    assert!(
        output.status.success(),
        "Godot exited {}: {stderr}",
        output.status
    );
    assert!(stdout.contains("FULL_LOD_RESULT PASS"));
}

#[test]
fn persistent_full_lod_installed_whiterun_prepares_models() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKIP persistent_full_lod_installed_whiterun_prepares_models: SKYRIM_DATA unset");
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&data), 1).unwrap();
    let (world, _, position) = gate_landing(&source);
    let persistent = source.cell_index().world_persistent_cell(world).unwrap();
    let loaded = source.stream_cell_id(persistent).unwrap();
    let drawn: BTreeSet<_> = loaded
        .surfaces
        .iter()
        .map(|surface| surface.reference)
        .collect();
    let origin = [
        (position[0] / crate::units::CELL_UNITS).floor() as i32,
        (position[1] / crate::units::CELL_UNITS).floor() as i32,
    ];
    let mut expected = 0;
    let mut missing = Vec::new();
    let mut markers = 0;
    let mut marker_assets = HashMap::new();
    for &id in source.cell_index().references(persistent) {
        let owner = source.order.winner_ref(id).unwrap();
        let record = source.order.record(owner);
        if record.tag != tag::REFR {
            continue;
        }
        let reference = Refr::from_record(record).unwrap();
        let base = source
            .order
            .winner(source.order.resolve(owner.file, reference.base))
            .unwrap();
        if reference.is_persistent()
            && (record.flags & 0x0001_0000 != 0 || (base.tag == tag::STAT && base.flags & 4 != 0))
        {
            assert!(
                source.presentation_references(persistent).contains(&id),
                "full model not world-resident: {id}"
            );
        }
        let p = reference.transform.unwrap().position;
        let grid = [
            (p[0] / crate::units::CELL_UNITS).floor() as i32,
            (p[1] / crate::units::CELL_UNITS).floor() as i32,
        ];
        if !reference.is_persistent()
            || (record.flags & 0x0001_0000 == 0 && !(base.tag == tag::STAT && base.flags & 4 != 0))
            || grid[0].abs_diff(origin[0]) > 20
            || grid[1].abs_diff(origin[1]) > 20
            || !soe_game::World::authored_reference_enabled(&source.order, id)
            || models::render_model_for(base).is_none()
        {
            continue;
        }
        let model = models::render_model_for(base).unwrap();
        if marker_only_asset(&source, &model, &mut marker_assets) {
            markers += 1;
            assert!(!drawn.contains(&id), "editor marker drawn: {id}");
            assert_eq!(loaded.stats.reference_skips.get(&id), Some(&"marker model"));
        } else {
            expected += 1;
        }
        if !marker_only_asset(&source, &model, &mut marker_assets) && !drawn.contains(&id) {
            missing.push(format!(
                "{id} {} {:?}",
                base.editor_id().unwrap_or_default(),
                loaded.stats.reference_skips.get(&id)
            ));
        }
    }
    eprintln!("FULL_LOD_PREPARE expected_near={expected} markers_near={markers} drawn_world={} selected={} logical={} surfaces={} triangles={} failed={} missing={missing:?}",
        drawn.len(), source.presentation_references(persistent).len(), source.cell_index().references(persistent).len(), loaded.surfaces.len(), loaded.stats.triangles, loaded.stats.meshes_failed);
    assert!(expected > 0);
    assert!(
        source.presentation_references(persistent).len()
            < source.cell_index().references(persistent).len()
    );
    assert!(
        missing.is_empty(),
        "eligible full models missing: {missing:?}"
    );
}
