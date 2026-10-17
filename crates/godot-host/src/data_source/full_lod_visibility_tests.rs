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
    fixture_with_disabled(false)
}

fn fixture_with_disabled(disabled: bool) -> (tempfile::TempDir, DataSource) {
    let directory = tempfile::tempdir().unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    for (object, kind, flags) in [
        (0x900, tag::STAT, 0),
        (0x901, tag::STAT, 4),
        (0x902, Tag::new(b"MISC"), 4),
        (0x903, Tag::new(b"MSTT"), 4),
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
            (5, 0x903, 0x400, 40.0),
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
            reference.flags = flags | if disabled && offset == 1 { 0x800 } else { 0 };
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
fn distant_admitted_live_enable_state_does_not_use_the_authored_disabled_flag() {
    let (_directory, source) = fixture_with_disabled(true);
    assert!(!soe_game::World::authored_reference_enabled(
        source.order(),
        id(0x111)
    ));
    assert_eq!(
        source
            .stream_cell_id(id(0x102))
            .unwrap()
            .stats
            .meshes_failed,
        2
    );
    let admitted = source
        .stream_selected_references(id(0x101), &[id(0x111)])
        .unwrap();
    assert_eq!(
        admitted.stats.meshes_failed, 1,
        "live admitted reference must reach model preparation"
    );
}

#[test]
fn persistent_full_lod_residency_is_world_scoped_and_disjoint_from_grid() {
    let (_directory, source) = fixture();
    for world in [0x100, 0x200] {
        assert_eq!(
            source.presentation_references(id(world + 1)),
            &[
                id(world + 0x11),
                id(world + 0x12),
                id(world + 0x13),
                id(world + 0x15),
            ]
        );
        assert_eq!(
            source.presentation_references(id(world + 2)),
            &[id(world + 0x10), id(world + 0x11), id(world + 0x14)]
        );
        let loaded = source.load_location(id(world + 2), [0.0; 3], 0).unwrap();
        assert_eq!(loaded.cells.len(), 2);
        assert_eq!(
            loaded
                .cells
                .iter()
                .map(|(_, cell)| cell.stats.meshes_failed)
                .sum::<usize>(),
            3
        );
        assert_eq!(source.cell_index().references(id(world + 1)).len(), 6);
        assert_eq!(source.physical_references(id(world + 1)), &[]);
        assert_eq!(
            source.physical_references(id(world + 2)),
            &[id(world + 0x10), id(world + 0x11), id(world + 0x14)]
        );
        assert_eq!(loaded.collision.stats.references, 3);
        assert_eq!(
            source
                .stream_cell_id(id(world + 1))
                .unwrap()
                .stats
                .meshes_failed,
            0
        );
        assert_eq!(source.distant_candidates(id(world)).len(), 4);
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
            let is_never = base_never_fades(base);
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
        if persistent_full_model(&reference, base) {
            assert!(
                source.presentation_references(persistent).contains(&id),
                "full model not world-resident: {id}"
            );
        }
        if !persistent_full_model(&reference, base)
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
    let city = source
        .exterior_world(source.resolve_cell("WhiterunOrigin").unwrap())
        .unwrap();
    let city_cells: Vec<_> = source
        .cell_index()
        .cells()
        .map(|(cell, _)| cell)
        .filter(|cell| source.exterior_world(*cell) == Some(city))
        .collect();
    assert!(!city_cells.is_empty());
    let mut city_expected = BTreeSet::new();
    for cell in city_cells {
        for &id in source.cell_index().references(cell) {
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
            if (record.flags & 0x0001_0000 != 0
                || (reference.is_persistent() && base_never_fades(base)))
                && soe_game::World::authored_reference_enabled(&source.order, id)
                && models::render_model_for(base)
                    .is_some_and(|model| !marker_only_asset(&source, &model, &mut marker_assets))
            {
                city_expected.insert(id.to_string());
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let parent = root.join("target/full-lod");
    std::fs::create_dir_all(&parent).unwrap();
    let directory = tempfile::Builder::new()
        .prefix("run-")
        .tempdir_in(parent)
        .unwrap()
        .keep();
    eprintln!("FULL_LOD_ARTIFACTS {}", directory.display());
    let manifest = directory.join("expected.json");
    let mountain = source
        .load_object_lod_block(
            world,
            "Tamriel",
            crate::terrain_lod::LodBlockCoord {
                level: crate::terrain_lod::LodLevel::L16,
                x: 0,
                y: -16,
            },
        )
        .unwrap()
        .unwrap();
    let peak = mountain
        .surfaces
        .iter()
        .flat_map(|surface| &surface.arrays.positions)
        .max_by(|a, b| a[1].total_cmp(&b[1]))
        .unwrap();
    let peak = [peak[0] * 70.0, -peak[2] * 70.0, peak[1] * 70.0];
    drop(mountain);
    std::fs::write(
        &manifest,
        serde_json::to_vec(
            &serde_json::json!({"all": expected, "city": city_expected, "near": near, "markers": markers, "peak": peak}),
        )
        .unwrap(),
    )
    .unwrap();
    drop(source);
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN must name the test Godot");
    let stdout_file = std::fs::File::create(directory.join("stdout.log")).unwrap();
    let stderr_file = std::fs::File::create(directory.join("stderr.log")).unwrap();
    let mut child = std::process::Command::new(godot)
        .current_dir(root)
        .args([
            "--path",
            "godot",
            "--resolution",
            "640x360",
            "--script",
            "persistent_full_lod_visibility_test.gd",
        ])
        .env("SOE_FULL_LOD_EXPECT", &manifest)
        .env("SOE_VISIBILITY_CAPTURE", &directory)
        .stdout(stdout_file)
        .stderr(stderr_file)
        .spawn()
        .unwrap();
    let mut peak_kib = 0u64;
    let mut godot_pid = 0;
    let status = loop {
        let mut pids = vec![child.id()];
        let mut at = 0;
        while at < pids.len() {
            let pid = pids[at];
            at += 1;
            let children = std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
                .unwrap_or_default();
            pids.extend(
                children
                    .split_whitespace()
                    .filter_map(|pid| pid.parse::<u32>().ok()),
            );
            let command =
                std::fs::read_to_string(format!("/proc/{pid}/cmdline")).unwrap_or_default();
            if command
                .split('\0')
                .next()
                .is_some_and(|name| name.contains("Godot_v4"))
            {
                godot_pid = pid;
                let proc_status =
                    std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
                if let Some(rss) = proc_status
                    .lines()
                    .find(|line| line.starts_with("VmRSS:"))
                    .and_then(|line| line.split_whitespace().nth(1))
                    .and_then(|value| value.parse::<u64>().ok())
                {
                    peak_kib = peak_kib.max(rss);
                }
            }
        }
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    };
    let resident_gib = peak_kib as f64 / 1024.0 / 1024.0;
    eprintln!("FULL_LOD_RSS pid={godot_pid} peak_gib={resident_gib:.6} exit={status}");
    std::fs::write(
        directory.join("rss.json"),
        serde_json::to_vec(&serde_json::json!({"godot_pid":godot_pid,"peak_gib":resident_gib}))
            .unwrap(),
    )
    .unwrap();
    assert!(
        godot_pid > 0 && peak_kib > 0,
        "actual Godot VmRSS was not observed"
    );
    let stdout_bytes = std::fs::read(directory.join("stdout.log")).unwrap();
    let stderr_bytes = std::fs::read(directory.join("stderr.log")).unwrap();
    let stdout = String::from_utf8_lossy(&stdout_bytes);
    let stderr = String::from_utf8_lossy(&stderr_bytes);
    std::fs::write(directory.join("stdout.log"), stdout.as_bytes()).unwrap();
    std::fs::write(directory.join("stderr.log"), stderr.as_bytes()).unwrap();
    for line in stdout.lines().filter(|line| line.starts_with("FULL_LOD")) {
        eprintln!("{line}");
    }
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
    assert!(status.success(), "Godot exited {status}: {stderr}");
    assert!(stdout.contains("FULL_LOD_RESULT PASS"));
    assert!(
        resident_gib < 6.0,
        "Godot exceeds the predeclared 6 GiB budget: {resident_gib}"
    );
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
    assert!(source
        .stream_cell_id(persistent)
        .unwrap()
        .surfaces
        .is_empty());
    let candidates = source.distant_candidates(world);
    let eye = [position[0], position[1], position[2] + 120.0];
    let selected =
        crate::distant_visibility::Selector::default().update(&candidates, eye, 0, |_, _| None);
    assert!(!selected.is_empty() && selected.len() <= 8);
    let loaded = source
        .stream_selected_references(persistent, &selected)
        .unwrap();
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
        if persistent_full_model(&reference, base) {
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
        if !selected.contains(&id)
            || !persistent_full_model(&reference, base)
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
    assert!(expected + markers > 0);
    assert!(
        source.presentation_references(persistent).len()
            < source.cell_index().references(persistent).len()
    );
    assert!(
        missing.is_empty(),
        "eligible full models missing: {missing:?}"
    );
    #[cfg(target_os = "linux")]
    if std::env::var_os("SOE_WHITERUN_PEAK_CHECK").is_some() {
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        let peak_kib = status
            .lines()
            .find_map(|line| {
                line.strip_prefix("VmHWM:")?
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()
            })
            .expect("Linux process peak RSS");
        eprintln!("WHITERUN_PREPARATION_PEAK_KIB={peak_kib}");
        assert!(
            peak_kib <= 2 * 1024 * 1024,
            "isolated Whiterun preparation exceeded two GiB: {peak_kib} KiB"
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn persistent_full_lod_installed_whiterun_preparation_peak_is_bounded() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "data_source::full_lod_visibility_tests::persistent_full_lod_installed_whiterun_prepares_models",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("SOE_WHITERUN_PEAK_CHECK", "1")
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success() && log.contains("WHITERUN_PREPARATION_PEAK_KIB="),
        "{log}"
    );
    eprintln!("{log}");
}

#[cfg(feature = "godot")]
#[test]
fn distant_occlusion_keeps_a_partly_visible_object() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("SKIP distant_occlusion_keeps_a_partly_visible_object: SOE_GODOT_BIN unset");
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("project.godot"),
        "config_version=5\n[application]\nconfig/name=\"Conservative occlusion fixture\"\n",
    )
    .unwrap();
    std::fs::write(
        directory.path().join("test.gd"),
        include_str!("../../../../godot/distant_occlusion_test.gd"),
    )
    .unwrap();
    let output = std::process::Command::new(godot)
        .args(["--path"])
        .arg(directory.path())
        .args(["--resolution", "640x360", "--script", "test.gd"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    eprintln!("{stdout}");
    assert!(
        output.status.success() && stdout.contains("OCCLUSION_RESULT PASS"),
        "{stderr}"
    );
}

#[cfg(feature = "godot")]
#[test]
fn whiterun_lod_installed_authored_hidden_land_has_no_triangles_or_collision() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&data), 1).unwrap();
    let (city, _) = source.resolve_world("WhiterunWorld").unwrap();
    let order = source.shared_order();
    let coordinates = [
        (5, -3),
        (7, -1),
        (6, -1),
        (6, -2),
        (5, -2),
        (6, -3),
        (5, -1),
    ]
    .map(|(x, y)| crate::streaming::CellCoord::new(x, y));
    let cells = source.prepare_grid_cells(city, &coordinates).unwrap();
    assert_eq!(cells.len(), coordinates.len());
    for cell in cells {
        let record = order.winner(cell.id).unwrap();
        let grid = soe_esm::records::cell::Cell::from_record(record, false)
            .unwrap()
            .grid
            .unwrap();
        let hidden = grid.flags.unwrap_or(0) & 15;
        assert_ne!(hidden, 0);
        let terrain = cell
            .loaded
            .surfaces
            .iter()
            .find(|s| {
                s.arrays
                    .material
                    .diffuse
                    .as_deref()
                    .is_some_and(|p| p.starts_with("terrain:"))
            })
            .unwrap();
        let expected = (4 - hidden.count_ones()) as usize * 512;
        eprintln!(
            "WHITERUN_HIDDEN grid={},{} mask={hidden:x} triangles={} expected={expected}",
            grid.x,
            grid.y,
            terrain.arrays.triangle_count()
        );
        assert_eq!(
            terrain.arrays.triangle_count(),
            expected,
            "authored hidden LAND at {},{}",
            grid.x,
            grid.y
        );
        let colliders: Vec<_> = cell
            .colliders
            .iter()
            .filter(|c| c.reference == terrain.reference)
            .collect();
        assert_eq!(colliders.len(), usize::from(expected > 0));
        for collider in colliders {
            let soe_physics::Shape::TriangleMesh { triangles, .. } = &collider.collider.shape
            else {
                panic!("LAND collider must be a triangle mesh")
            };
            assert_eq!(triangles.len(), expected);
            assert!(expected > 0, "fully hidden LAND must not create a collider");
        }
    }
}

#[test]
fn whiterun_lod_installed_object_geometry_extents() {
    use crate::streaming::CellCoord;
    use crate::terrain_lod::{bto_path, LodKind, TerrainLodConfig, TerrainLodStreamer};
    use std::collections::{BTreeMap, HashSet};
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&data), 1).unwrap();
    let (city, _) = source.resolve_world("WhiterunWorld").unwrap();
    let (_, name) = source.lod_worldspace(city).unwrap();
    let full: Vec<_> = source
        .cell_index()
        .cells()
        .filter_map(|(id, _)| {
            let grid = source.cell_index().grid_of(id)?;
            (grid.world == city).then_some(CellCoord::new(grid.x, grid.y))
        })
        .collect();
    let full_set: HashSet<_> = full.iter().copied().collect();
    let focus = CellCoord::new(6, -1);
    let mut planner = TerrainLodStreamer::with_catalog(
        TerrainLodConfig {
            full_res_radius: 2,
            lod_radius: 96,
            ..Default::default()
        },
        source.lod_settings(&name).unwrap().unwrap().sw_cell,
        source.scenery_lod_blocks(&name, LodKind::Object),
    )
    .unwrap();
    planner.set_full_cells(&full);
    let blocks = planner.settle(focus).load;
    let mut intrusions = BTreeMap::new();
    let mut triangles = 0;
    let mut nearby = 0;
    for block in &blocks {
        let file = soe_nif::parse(&source.vfs().read(&bto_path(&name, *block)).unwrap()).unwrap();
        for mesh in file.meshes().unwrap() {
            for indices in mesh.indices.as_chunks::<3>().0 {
                triangles += 1;
                let points = indices.map(|i| {
                    mesh.transform.apply(soe_nif::Vec3::new(
                        mesh.positions[i as usize][0],
                        mesh.positions[i as usize][1],
                        mesh.positions[i as usize][2],
                    ))
                });
                let x = points.iter().map(|p| p.x).sum::<f32>() / 3.0;
                let y = points.iter().map(|p| p.y).sum::<f32>() / 3.0;
                let cell = CellCoord::new((x / 4096.0).floor() as i32, (y / 4096.0).floor() as i32);
                if full_set.contains(&cell) {
                    *intrusions
                        .entry((block.level.cells(), block.x, block.y, cell.x, cell.y))
                        .or_insert(0usize) += 1;
                    nearby += usize::from(cell.ring_distance(focus) <= 2);
                }
            }
        }
    }
    eprintln!("WHITERUN_OBJECT_EXTENTS cells={} blocks={} triangles={triangles} near_pond_intrusions={nearby} intrusions={intrusions:?}", full.len(), blocks.len());
    assert!(triangles > 0);
    assert_eq!(
        nearby, 0,
        "baked object LOD reaches the player's loaded near cells"
    );
}

#[test]
fn whiterun_lod_installed_bto_overhangs_have_no_resident_source() {
    use crate::streaming::CellCoord;
    use crate::terrain_lod::{bto_path, LodKind, TerrainLodConfig, TerrainLodStreamer};
    use soe_esm::records::stat::Static;

    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&data), 1).unwrap();
    let (world, name) = source.resolve_world("Tamriel").unwrap();
    let focus = CellCoord::new(10, -2);
    let full: HashSet<_> = (-2..=2)
        .flat_map(|y| (-2..=2).map(move |x| CellCoord::new(focus.x + x, focus.y + y)))
        .collect();
    let anchor = source.lod_settings(&name).unwrap().unwrap().sw_cell;
    let mut planner = TerrainLodStreamer::with_catalog(
        TerrainLodConfig {
            full_res_radius: 2,
            lod_radius: 96,
            ..Default::default()
        },
        anchor,
        source.scenery_lod_blocks(&name, LodKind::Object),
    )
    .unwrap();
    planner.set_full_cells(&full.iter().copied().collect::<Vec<_>>());
    let blocks = planner.settle(focus).load;
    let mut targets = HashMap::new();
    for block in blocks {
        let file = soe_nif::parse(&source.vfs.read(&bto_path(&name, block)).unwrap()).unwrap();
        for mesh in file.meshes().unwrap() {
            for indices in mesh.indices.as_chunks::<3>().0 {
                let points = indices.map(|i| {
                    mesh.transform.apply(NifVec3::new(
                        mesh.positions[i as usize][0],
                        mesh.positions[i as usize][1],
                        mesh.positions[i as usize][2],
                    ))
                });
                let centre = points
                    .iter()
                    .fold(NifVec3::new(0.0, 0.0, 0.0), |a, b| a + *b)
                    .scale(1.0 / 3.0);
                let cell = CellCoord::new(
                    (centre.x / crate::units::CELL_UNITS).floor() as i32,
                    (centre.y / crate::units::CELL_UNITS).floor() as i32,
                );
                if full.contains(&cell) {
                    targets.entry(block).or_insert_with(Vec::new).push(points);
                }
            }
        }
    }

    // CONTEXT: BTO half-float packing loses precision. Require all three vertices
    // within eight Skyrim units, rather than attributing by an overlapping bound.
    const TOLERANCE: f32 = 8.0;
    const BIN_WIDTH: f32 = 32.0;
    const ORDERS: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let key = |points: &[NifVec3; 3]| -> [i32; 3] {
        std::array::from_fn(|axis| {
            (points.iter().map(|p| [p.x, p.y, p.z][axis]).sum::<f32>() / 3.0 / BIN_WIDTH).floor()
                as i32
        })
    };
    let mut models = HashMap::new();
    let mut matched = 0usize;
    let mut unmatched = 0usize;
    let mut resident = BTreeSet::new();
    let mut counts = BTreeMap::new();
    let mut targets: Vec<_> = targets.into_iter().collect();
    targets.sort_unstable_by_key(|(block, _)| (block.level.cells(), block.x, block.y));
    for (block, triangles) in targets {
        let level = match block.level.cells() {
            4 => 0,
            8 => 1,
            16 => 2,
            _ => 3,
        };
        let mut candidates: HashMap<_, Vec<_>> = HashMap::new();
        for (id, record) in source.order.winners_of(tag::REFR) {
            let owner = source.order.winner_ref(id).unwrap();
            if record
                .placement
                .world
                .map(|raw| source.order.resolve(owner.file, raw))
                != Some(world)
            {
                continue;
            }
            let reference = Refr::from_record(record).unwrap();
            let Some(transform) = reference.transform else {
                continue;
            };
            let coord = CellCoord::new(
                (transform.position[0] / crate::units::CELL_UNITS).floor() as i32,
                (transform.position[1] / crate::units::CELL_UNITS).floor() as i32,
            );
            if crate::terrain_lod::LodBlockCoord::containing_from(coord, block.level, anchor)
                != block
            {
                continue;
            }
            let base_record = source
                .order
                .winner(source.order.resolve(owner.file, reference.base))
                .unwrap();
            if base_record.tag != tag::STAT {
                continue;
            }
            let base = Static::from_record(base_record).unwrap();
            let Some(path) = base
                .lod_models
                .as_ref()
                .map(|paths| &paths[level])
                .filter(|p| !p.is_empty())
            else {
                continue;
            };
            let meshes = models.entry(path.clone()).or_insert_with(|| {
                let path = soe_bsa::normalize(path);
                let path = if path.starts_with("meshes/") {
                    path
                } else {
                    format!("meshes/{path}")
                };
                let Ok(bytes) = source.vfs.read(&path) else {
                    return Vec::new();
                };
                soe_nif::parse(&bytes).unwrap().meshes().unwrap()
            });
            let placement = placement_transform(&reference);
            let owner_cell = record
                .placement
                .cell
                .map(|raw| source.order.resolve(owner.file, raw));
            let resident_cell = owner_cell
                .and_then(|cell| source.cell_index().grid_of(cell))
                .map_or(coord, |grid| CellCoord::new(grid.x, grid.y));
            for mesh in meshes {
                let placement = placement.then(&mesh.transform);
                for indices in mesh.indices.as_chunks::<3>().0 {
                    let points = indices.map(|i| {
                        let p = mesh.positions[i as usize];
                        placement.apply(NifVec3::new(p[0], p[1], p[2]))
                    });
                    candidates
                        .entry(key(&points))
                        .or_default()
                        .push((id, resident_cell, points));
                }
            }
        }
        for triangle in triangles {
            let centre = key(&triangle);
            let mut sources = BTreeSet::new();
            for x in -1..=1 {
                for y in -1..=1 {
                    for z in -1..=1 {
                        let Some(candidates) =
                            candidates.get(&[centre[0] + x, centre[1] + y, centre[2] + z])
                        else {
                            continue;
                        };
                        for (id, cell, points) in candidates {
                            if ORDERS.iter().any(|order| {
                                triangle.iter().zip(order).all(|(a, i)| {
                                    let b = points[*i];
                                    (a.x - b.x)
                                        .abs()
                                        .max((a.y - b.y).abs())
                                        .max((a.z - b.z).abs())
                                        <= TOLERANCE
                                })
                            }) {
                                sources.insert(*id);
                                if full.contains(cell) {
                                    resident.insert(*id);
                                }
                            }
                        }
                    }
                }
            }
            if sources.is_empty() {
                unmatched += 1;
                if unmatched <= 5 {
                    eprintln!("BTO_UNMATCHED block={block:?} triangle={triangle:?}");
                }
            } else {
                matched += 1;
                for id in sources {
                    *counts.entry(id).or_insert(0usize) += 1;
                }
            }
        }
    }
    for (id, count) in counts {
        let owner = source.order.winner_ref(id).unwrap();
        let reference = Refr::from_record(source.order.record(owner)).unwrap();
        let base = source
            .order
            .winner(source.order.resolve(owner.file, reference.base))
            .unwrap();
        eprintln!(
            "BTO_SOURCE reference={id} base={:?} triangles={count} position={:?}",
            base.editor_id(),
            reference.transform
        );
    }
    eprintln!("BTO_ATTRIBUTION matched={matched} unmatched={unmatched} resident={resident:?}");
    assert!(matched > 0, "survey must exercise baked object overhangs");
    assert_eq!(unmatched, 0, "every overhang needs an authored source");
    assert!(
        resident.is_empty(),
        "resident full-detail objects are duplicated"
    );
}

#[test]
fn installed_native_terrain_walkthrough() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let binary = std::env::var_os("SOE_GODOT_BIN").expect("installed renderer requires Godot");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = root.join("target/r4-terrain-regression");
    std::fs::create_dir_all(&directory).unwrap();
    let output = std::process::Command::new(binary)
        .arg("--path")
        .arg(root.join("godot"))
        .args(["--resolution", "640x360", "--script", "r4_terrain_probe.gd"])
        .env("DISPLAY", ":99")
        .env("SKYRIM_DATA", data)
        .env("SOE_GODOT_TIMEOUT", "600")
        .env("SOE_CHARACTER_OUTPUT", &directory)
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(directory.join("test.log"), &log).unwrap();
    assert!(
        output.status.success()
            && log.contains("RESULT: PASS R4 terrain probe")
            && !log.contains("SCRIPT ERROR"),
        "{log}"
    );
    println!("{log}");
}

#[test]
fn r4_tree_installed_whiterun_billboard_placement_survey() {
    use crate::terrain_lod::{btt_path, LodKind};
    use soe_esm::records::land::Land;
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&data), 2).unwrap();
    let (world, name) = source.resolve_world("Tamriel").unwrap();
    let pine = id(0x0C8BB0);
    let owner = source.order.winner_ref(pine).unwrap();
    let reference = Refr::from_record(source.order.record(owner)).unwrap();
    let base = source
        .order
        .winner(source.order.resolve(owner.file, reference.base))
        .unwrap();
    let model = models::render_model_for(base).unwrap();
    let file = placed_model(&source.vfs.read(&format!("meshes/{model}")).unwrap()).unwrap();
    let meshes = file.meshes().unwrap();
    assert!(meshes.len() > 1);
    for mesh in meshes {
        eprintln!(
            "R4 TREE PINE ref={pine} position={:?} shape={} skin={} weights={} flags={:#x}/{:#x}",
            reference.transform.unwrap().position,
            mesh.name,
            mesh.skin.is_some(),
            mesh.bone_indices.len(),
            mesh.material.flags1,
            mesh.material.flags2
        );
    }
    let mut heights = HashMap::new();
    let mut candidates = Vec::new();
    let mut trees = 0;
    for block in source.scenery_lod_blocks(&name, LodKind::Tree) {
        if block.x > 24
            || block.x + block.level.cells() < -8
            || block.y > 16
            || block.y + block.level.cells() < -24
        {
            continue;
        }
        let bytes = source.vfs.read(&btt_path(&name, block)).unwrap();
        for tree in crate::tree_lod::read_references(&bytes).unwrap() {
            let [x, y, z] = tree.position;
            if (x - 30000.0).hypot(y + 5000.0) > 50000.0 {
                continue;
            }
            let grid = ((x / 4096.0).floor() as i32, (y / 4096.0).floor() as i32);
            let land = heights.entry(grid).or_insert_with(|| {
                let cell = source.cell_index().exterior_cell(world, grid.0, grid.1)?;
                let id = source.cell_index().land(cell)?;
                Land::from_record(source.order.winner(id)?)
                    .ok()?
                    .heights
                    .map(|h| h.decode())
            });
            let Some(land) = land else {
                continue;
            };
            let col = ((x.rem_euclid(4096.0) / 128.0).round() as usize).min(32);
            let row = ((y.rem_euclid(4096.0) / 128.0).round() as usize).min(32);
            let gap = z - land[row * 33 + col];
            trees += 1;
            if gap > 500.0 {
                candidates.push((gap, tree));
            }
        }
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (gap, tree) in candidates.iter().take(25) {
        eprintln!(
            "R4 TREE BTT gap={gap} ref={:08X} position={:?} type={} scale={}",
            tree.raw_form_id, tree.position, tree.tree_type, tree.scale
        );
    }
    eprintln!(
        "R4 TREE SURVEY trees={trees} gaps_above_500={}",
        candidates.len()
    );
    assert!(trees > 1000);
    for candidate in source.distant_candidates(world) {
        let owner = source.order.winner_ref(candidate.id).unwrap();
        let record = source.order.record(owner);
        let reference = Refr::from_record(record).unwrap();
        let base = source
            .order
            .winner(source.order.resolve(owner.file, reference.base))
            .unwrap();
        let Some(model) = models::render_model_for(base) else {
            continue;
        };
        if !model.to_ascii_lowercase().contains("tree") {
            continue;
        }
        let position = reference.transform.unwrap().position;
        eprintln!("R4 TREE FULL reference={} base={} model={model} position={position:?} flags={:#x} enabled={}", candidate.id, base.editor_id().unwrap_or_default(), record.flags, soe_game::World::authored_reference_enabled(&source.order, candidate.id));
    }
}

#[test]
fn installed_tree_placement_walkthrough() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let binary = std::env::var_os("SOE_GODOT_BIN").expect("installed renderer requires Godot");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = root.join("target/r4-tree-regression");
    std::fs::create_dir_all(&directory).unwrap();
    let output = std::process::Command::new(binary)
        .arg("--path")
        .arg(root.join("godot"))
        .args(["--resolution", "640x360", "--script", "r4_tree_probe.gd"])
        .env("DISPLAY", ":99")
        .env("SKYRIM_DATA", data)
        .env("SOE_GODOT_TIMEOUT", "900")
        .env("SOE_CHARACTER_OUTPUT", &directory)
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(directory.join("test.log"), &log).unwrap();
    assert!(
        output.status.success()
            && log.contains("RESULT: PASS R4 tree probe")
            && !log.contains("SCRIPT ERROR"),
        "{log}"
    );
}
