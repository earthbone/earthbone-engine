// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for PRF-20 cell, asset and memory attribution.

use soe_trace::{
    FrameSnapshot, LoadMeasure, LoadRecorder, LoadReport, LoadStage, MemorySnapshot, System,
    SystemScope,
};
use std::collections::BTreeMap;
use std::hint::black_box;

#[test]
fn prf_20_adversarial_nested_captures_restore_after_unwind() {
    let outer = LoadRecorder::default();
    let inner = LoadRecorder::default();
    {
        let _outer_capture = outer.enter();
        let _outer_before = LoadMeasure::archive("meshes/outer.nif", "Base.bsa");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _inner_capture = inner.enter();
            let _inner_measure = LoadMeasure::new(LoadStage::NifParse, "meshes/inner.nif");
            panic!("exercise capture restoration");
        }));
        assert!(result.is_err());
        assert!(LoadRecorder::current().is_some());
        let _outer_after = LoadMeasure::new(LoadStage::CollisionBuild, "meshes/outer.nif");
    }
    assert!(LoadRecorder::current().is_none());
    let outer_report = outer.report();
    let inner_report = inner.report();
    assert_eq!(
        outer_report.asset_ns.keys().collect::<Vec<_>>(),
        [&"meshes/outer.nif"]
    );
    assert!(outer_report.stage_ns[LoadStage::CollisionBuild as usize] > 0);
    assert!(inner_report.stage_ns[LoadStage::NifParse as usize] > 0);
    assert_eq!(inner_report.archive_ns.len(), 0);
}

#[test]
fn prf_20_adversarial_stage_totals_aggregate_and_asset_ties_are_stable() {
    let recorder = LoadRecorder::default();
    {
        let _capture = recorder.enter();
        for stage in LoadStage::ALL {
            let _measure = LoadMeasure::new(stage, "textures/shared.dds");
            black_box(stage.name());
        }
        let _first = LoadMeasure::archive("textures/shared.dds", "Textures.bsa");
        let _second = LoadMeasure::archive("textures/shared.dds", "Patch.bsa");
    }
    let report = recorder.report();
    assert!(LoadStage::ALL
        .iter()
        .all(|stage| report.stage_ns[*stage as usize] > 0));
    assert_eq!(report.archive_ns.len(), 2);
    assert!(report.archive_ns.values().all(|duration| *duration > 0));
    assert_eq!(report.slowest_asset().unwrap().0, "textures/shared.dds");

    let tied = LoadReport {
        asset_ns: BTreeMap::from([("z.nif".into(), 7), ("a.nif".into(), 7)]),
        ..LoadReport::default()
    };
    assert_eq!(tied.slowest_asset(), Some(("a.nif", 7)));
}

#[test]
fn prf_20_adversarial_allocator_keeps_owner_after_scope_and_thread_transfer() {
    let before = MemorySnapshot::now();
    let frame = FrameSnapshot::now();
    let payload = {
        let _scope = SystemScope::new(System::Save);
        let mut bytes = Vec::with_capacity(8192);
        bytes.resize(8192, 0x5a);
        black_box(&bytes);
        bytes.into_boxed_slice()
    };
    let owned = MemorySnapshot::now();
    let index = System::Save as usize;
    assert!(owned.live_bytes[index] >= before.live_bytes[index] + 8192);
    assert!(owned.allocation_calls[index] > before.allocation_calls[index]);
    std::thread::spawn(move || drop(payload)).join().unwrap();
    let freed = MemorySnapshot::now();
    assert!(freed.live_bytes[index] <= owned.live_bytes[index] - 8192);
    let sample = frame.finish();
    assert!(sample.allocation_calls[index] > 0);
}

#[test]
fn prf_20_adversarial_installed_cells_report_every_stage_and_exact_texture_inventory() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source =
        soe_godot_host::data_source::DataSource::open_with_threads(std::path::Path::new(&data), 1)
            .unwrap();
    let game = soe_game::Game::from_shared_order(source.shared_order());
    let plugin_bytes = source.record_memory_by_plugin();
    assert_eq!(plugin_bytes.len(), 5);
    assert!(plugin_bytes.values().all(|bytes| *bytes > 0));

    for cell_name in ["WhiterunBanneredMare", "WhiterunOrigin"] {
        source.clear_prepared_textures();
        let cell = source.resolve_cell(cell_name).unwrap();
        game.world_mut().load_cell(cell);
        let loaded = if source.cell_index().is_interior(cell) {
            source.stream_cell_id(cell).unwrap()
        } else {
            let mut cells = source.stream_exterior(cell_name, 0).unwrap().cells;
            assert_eq!(cells.len(), 1);
            cells.pop().unwrap().1
        };
        loaded.stats.profile.add_duration(
            LoadStage::Navmesh,
            game.world_mut().take_navigation_load_time(cell),
        );
        {
            let _capture = loaded.stats.profile.enter();
            source.placed_cell_colliders(cell).unwrap();
        }
        let report = loaded.stats.profile.report();
        let diagnostic = report.diagnostic();
        for stage in LoadStage::ALL {
            assert!(
                diagnostic.contains(stage.name()),
                "{cell_name}: {}",
                stage.name()
            );
        }
        assert!(report.archive_ns.values().all(|duration| *duration > 0));
        assert!(report
            .archive_ns
            .keys()
            .any(|name| name.to_ascii_lowercase().ends_with(".bsa")));
        let expected_slowest = report
            .asset_ns
            .iter()
            .max_by(|(a, left), (b, right)| left.cmp(right).then_with(|| b.cmp(a)));
        let actual_slowest = report.slowest_asset().unwrap();
        assert_eq!(
            (actual_slowest.0, actual_slowest.1),
            expected_slowest
                .map(|(name, duration)| (name.as_str(), *duration))
                .unwrap()
        );
        assert!(diagnostic.contains(actual_slowest.0));
        assert_eq!(report.texture_bytes, loaded.texture_inventory());
        assert_eq!(
            report.texture_total(),
            report.texture_bytes.values().copied().sum::<usize>()
        );
        assert!(report.texture_total() > 0);
    }
}

#[test]
fn prf_20_adversarial_live_godot_upload_and_hitch_diagnostic_path() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return;
    }
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN must be set");
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let output = std::process::Command::new(godot)
        .args(["--headless", "--path"])
        .arg(project)
        .args(["--script", "prf_20_attribution_test.gd"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("RESULT: PASS"), "{stdout}");
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}
