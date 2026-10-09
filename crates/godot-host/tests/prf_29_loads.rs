// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-29 real implementation boundaries and installed startup/Whiterun phases.

use soe_bsa::{write_bsa, BsaFile, BsaVersion, Vfs};
use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_godot_host::data_source::DataSource;
use soe_trace::{LoadCause, LoadScope, LoadTrace};
use std::path::PathBuf;
use std::time::Duration;

fn phase(root: u64, name: &str, cause: LoadCause) -> bool {
    LoadTrace::events()
        .iter()
        .any(|event| event.root == root && event.phase == name && event.cause == Some(cause))
}

#[test]
fn prf_29_plugin_cold_and_cache_hit_report_distinct_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixture.esp");
    let cache = dir.path().join("cache");
    std::fs::write(&path, Plugin::new(FileHeader::default()).to_bytes()).unwrap();
    let cold = LoadScope::operation("cold", "plugin", Duration::from_secs(1));
    let cold_id = cold.id();
    let plugin = soe_esm::read_plugin_cached(&path, &cache).unwrap().unwrap();
    let mut order = LoadOrder::new();
    order.add("fixture.esp", plugin.clone()).unwrap();
    drop(cold);
    assert!(phase(cold_id, "plugin_read", LoadCause::IoWait));
    assert!(phase(cold_id, "plugin_parse", LoadCause::Parse));
    assert!(phase(cold_id, "form_table", LoadCause::CpuBuild));
    let warm = LoadScope::operation("warm", "plugin", Duration::from_secs(1));
    let warm_id = warm.id();
    assert_eq!(
        soe_esm::read_plugin_cached(&path, &cache).unwrap().unwrap(),
        plugin
    );
    drop(warm);
    assert!(phase(warm_id, "plugin_cache_read", LoadCause::IoWait));
    assert!(phase(warm_id, "plugin_cache_decode", LoadCause::Decode));
    assert!(!phase(warm_id, "plugin_parse", LoadCause::Parse));
}

#[test]
fn prf_29_archive_and_loose_reads_keep_source_and_failed_loads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixture.bsa");
    let data = write_bsa(
        &[BsaFile {
            path: "meshes/test.nif",
            bytes: b"payload",
        }],
        BsaVersion::SkyrimSe,
    )
    .unwrap();
    std::fs::write(&path, data).unwrap();
    let load = LoadScope::operation("archive_fixture", "base:7", Duration::ZERO);
    let id = load.id();
    let mut vfs = Vfs::new();
    vfs.add_archive_path(&path).unwrap();
    assert_eq!(vfs.read("meshes/test.nif").unwrap(), b"payload");
    std::fs::create_dir(dir.path().join("meshes")).unwrap();
    std::fs::write(dir.path().join("meshes/test.nif"), b"loose").unwrap();
    vfs.add_loose_root(dir.path()).unwrap();
    assert_eq!(vfs.read("meshes/test.nif").unwrap(), b"loose");
    std::fs::remove_file(dir.path().join("meshes/test.nif")).unwrap();
    assert!(vfs.read("meshes/test.nif").is_err());
    drop(load);
    for (name, cause) in [
        ("archive_open", LoadCause::IoWait),
        ("archive_index", LoadCause::Parse),
        ("archive_index_read", LoadCause::IoWait),
        ("archive_read", LoadCause::IoWait),
        ("archive_decode", LoadCause::Decode),
        ("loose_read", LoadCause::IoWait),
    ] {
        assert!(phase(id, name, cause), "missing {name}");
    }
    let events = LoadTrace::events();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.root == id && event.phase == "loose_read")
            .count(),
        2
    );
    assert!(events
        .iter()
        .any(|event| event.root == id && event.asset.contains("fixture.bsa")));
}

#[test]
fn prf_29_engine_save_success_and_failure_are_reported() {
    let mut game = soe_game::Game::new(LoadOrder::new());
    let bytes = game.save_state().unwrap();
    let load = LoadScope::operation("save_fixture", "fixture", Duration::ZERO);
    let id = load.id();
    game.load_state(&bytes).unwrap();
    assert!(game.load_state(b"broken").is_err());
    drop(load);
    let events = LoadTrace::events();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.root == id && event.phase == "engine_save_load")
            .count(),
        2
    );
    assert!(phase(id, "engine_save_restore", LoadCause::Parse));
    assert_eq!(game.save_state().unwrap(), bytes);
}

#[test]
fn prf_29_script_parse_and_first_reference_binding_keep_base_identity() {
    use soe_esm::{GlobalFormId, Record, Tag};
    use soe_game::vmad::{ScriptEntry, Vmad};
    use soe_papyrus::asm::ScriptBuilder;

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("scripts")).unwrap();
    std::fs::write(
        dir.path().join("scripts/prf29.pex"),
        ScriptBuilder::new("Prf29", "ObjectReference").bytes(),
    )
    .unwrap();
    let mut vfs = Vfs::new();
    vfs.add_loose_root(dir.path()).unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_record(Record::new(Tag::new(b"CELL"), 0x800).sub(b"DATA", [1u8, 0]));
    plugin.push_record(Record::new(Tag::new(b"STAT"), 0x802));
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("Prf29")],
        ..Vmad::default()
    }
    .write(Tag::new(b"REFR"));
    let mut reference = Record::new(Tag::new(b"REFR"), 0x801)
        .sub(b"VMAD", vmad)
        .sub(b"NAME", 0x802u32.to_le_bytes())
        .sub(b"DATA", [0u8; 24]);
    reference.placement.cell = Some(0x800);
    plugin.push_record(reference);
    let mut order = LoadOrder::new();
    order.add("fixture.esm", plugin).unwrap();
    let mut game = soe_game::Game::new(order);
    let load = LoadScope::operation("script_fixture", "cell", Duration::ZERO);
    let id = load.id();
    assert_eq!(game.load_scripts_from_vfs(&vfs).scripts_loaded, 1);
    let cell = GlobalFormId {
        file: 0,
        object: 0x800,
    };
    assert_eq!(game.load_cell(cell).scripts_attached, 1);
    assert_eq!(game.bind_cell(cell).scripts_attached, 0);
    drop(load);
    assert!(phase(id, "script_parse", LoadCause::Parse));
    let events = LoadTrace::events();
    let bindings: Vec<_> = events
        .iter()
        .filter(|event| event.root == id && event.phase == "attached_scripts")
        .collect();
    assert_eq!(bindings.len(), 1);
    assert!(
        bindings[0].object.contains("base:0:000802"),
        "{}",
        bindings[0].object
    );
}

#[test]
fn prf_29_real_save_reports_io_header_decode_and_body() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES").map(PathBuf::from) else {
        return;
    };
    let mut paths = Vec::new();
    let mut directories = vec![root];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                directories.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ess"))
            {
                paths.push(path);
            }
        }
    }
    paths.sort();
    let path = paths.first().expect("SOE_TEST_SAVES contains no ESS files");
    let load = LoadScope::operation(
        "prf_29_real_save",
        path.display().to_string(),
        Duration::ZERO,
    );
    let id = load.id();
    let save = soe_save::Save::open(path).unwrap();
    drop(load);
    for (name, cause) in [
        ("save_read", LoadCause::IoWait),
        ("save_header", LoadCause::Parse),
        ("save_body", LoadCause::Parse),
    ] {
        assert!(phase(id, name, cause), "missing {name}");
    }
    if save.header.compression != soe_save::Compression::None {
        assert!(phase(id, "save_decompress", LoadCause::Decode));
    }
}

#[test]
fn prf_29_installed_startup_and_whiterun_report_cpu_and_asset_causes() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        return;
    };
    let load = LoadScope::operation(
        "prf_29_installed",
        "startup and Whiterun",
        Duration::from_secs(18),
    );
    let id = load.id();
    let source = DataSource::open_with_threads_and_cache(&data, 4, None).unwrap();
    let game = source.localized_game().unwrap();
    let cell = source.resolve_cell("WhiterunOrigin").unwrap();
    let loaded = source.stream_cell_id(cell).unwrap();
    assert!(loaded.stats.meshes_parsed > 0);
    let (collision, _) = source.cell_colliders(cell).unwrap();
    assert!(!collision.is_empty());
    soe_game::ai::nav_graph(&mut game.world_mut());
    drop(load);
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        assert!(
            LoadTrace::events().iter().any(|event| event.root == id
                && event.phase == "plugin_parse"
                && event.asset.ends_with(name)),
            "missing {name}"
        );
    }
    for (name, cause) in [
        ("form_table", LoadCause::CpuBuild),
        ("archive_index", LoadCause::Parse),
        ("string_table_parse", LoadCause::Parse),
        ("mesh_parse", LoadCause::Parse),
        ("mesh_build", LoadCause::CpuBuild),
        ("texture_parse", LoadCause::Parse),
        ("texture_prepare", LoadCause::Decode),
        ("collision_build", LoadCause::CpuBuild),
        ("navmesh_build", LoadCause::CpuBuild),
        ("worker_queue", LoadCause::WorkerQueue),
    ] {
        assert!(phase(id, name, cause), "missing {name}");
    }
    assert!(LoadTrace::events().iter().any(|event| event.root == id
        && event.phase == "mesh_parse"
        && event.object.starts_with("base:")));
    let dump = LoadTrace::summary_text();
    assert!(dump.contains("slowest=") && dump.contains("cause="));
}
