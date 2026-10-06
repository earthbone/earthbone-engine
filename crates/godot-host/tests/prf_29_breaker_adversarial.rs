// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use soe_godot_host::data_source::DataSource;
use soe_trace::{LoadCause, LoadScope, LoadTrace};

#[test]
fn prf_29_installed_startup_and_whiterun_load_report_names_and_causes() {
    let data = std::env::var_os("SKYRIM_DATA").expect("SKYRIM_DATA is required");
    let operation = LoadScope::operation(
        "prf_29_installed",
        "startup and Whiterun",
        Duration::from_secs(18),
    );
    let id = operation.id();
    let source = DataSource::open_with_threads_and_cache(&PathBuf::from(data), 4, None).unwrap();
    let game = source.localized_game().unwrap();
    let cell = source.resolve_cell("WhiterunOrigin").unwrap();
    assert!(source.stream_cell_id(cell).unwrap().stats.meshes_parsed > 0);
    assert!(!source.cell_colliders(cell).unwrap().0.is_empty());
    soe_game::ai::nav_graph(&mut game.world_mut());
    drop(operation);
    let events = LoadTrace::events();
    for plugin in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let per_plugin = events.iter().any(|event| {
            event.root == id
                && event.asset.ends_with(plugin)
                && matches!(event.phase, "plugin_parse" | "plugin_cache_decode")
        });
        assert!(
            per_plugin,
            "startup omitted parse/cache decode for {plugin}"
        );
    }
    for (phase, cause) in [
        ("form_table", LoadCause::CpuBuild),
        ("archive_index", LoadCause::Parse),
        ("string_table_parse", LoadCause::Parse),
        ("mesh_parse", LoadCause::Parse),
        ("mesh_build", LoadCause::CpuBuild),
        ("texture_prepare", LoadCause::Decode),
        ("collision_build", LoadCause::CpuBuild),
        ("navmesh_build", LoadCause::CpuBuild),
        ("worker_queue", LoadCause::WorkerQueue),
    ] {
        assert!(
            events.iter().any(|event| event.root == id
                && event.phase == phase
                && event.cause == Some(cause)),
            "startup omitted {phase} {cause:?}"
        );
    }
    assert!(
        events.iter().any(|event| event.root == id
            && event.phase == "mesh_parse"
            && event.object.starts_with("base:")),
        "mesh parse lacks base form attribution"
    );
}

#[test]
fn prf_29_compile_observations_report_counter_delta_and_frame_interval() {
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN is required");
    let data = std::env::var_os("SKYRIM_DATA").expect("SKYRIM_DATA is required");
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .expect("crate parent")
        .parent()
        .expect("workspace root");
    let project = root.join("godot");
    let cache = tempfile::tempdir().expect("isolated Godot cache");
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "prf_29_loads_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .env("XDG_DATA_HOME", cache.path())
        .output()
        .expect("Godot compile observation scene should start");
    let report = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        report.contains("LOAD_COMPILE_OBSERVATION"),
        "no compile observation: {report}"
    );
    assert!(
        report.contains("\"cause\":\"pipeline_compile_observed\""),
        "compile cause missing: {report}"
    );
    assert!(
        report.contains("\"interval_ms\":"),
        "observed frame interval missing: {report}"
    );
    assert!(
        report.contains("\"sources\":"),
        "pipeline counter delta missing: {report}"
    );
}

#[test]
fn prf_29_all_installed_user_saves_report_load_io_and_parse_phases() {
    let root = std::env::var_os("SOE_TEST_SAVES").expect("SOE_TEST_SAVES is required");
    let mut pending = vec![PathBuf::from(root)];
    let mut saves = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).expect("read save directory") {
            let path = entry.expect("save directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ess"))
            {
                saves.push(path);
            }
        }
    }
    saves.sort();
    assert!(!saves.is_empty(), "SOE_TEST_SAVES contained no ESS files");
    for path in saves {
        let operation = LoadScope::operation(
            "prf_29_user_save",
            path.display().to_string(),
            Duration::ZERO,
        );
        let id = operation.id();
        let save = soe_save::Save::open(&path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        drop(operation);
        let events = LoadTrace::events();
        for (phase, cause) in [
            ("save_read", LoadCause::IoWait),
            ("save_header", LoadCause::Parse),
            ("save_body", LoadCause::Parse),
        ] {
            assert!(
                events.iter().any(|event| event.root == id
                    && event.phase == phase
                    && event.cause == Some(cause)),
                "{} missing {phase} {cause:?}",
                path.display()
            );
        }
        if save.header.compression != soe_save::Compression::None {
            assert!(
                events.iter().any(|event| event.root == id
                    && event.phase == "save_decompress"
                    && event.cause == Some(LoadCause::Decode)),
                "{} missing decompression cause",
                path.display()
            );
        }
    }
}
