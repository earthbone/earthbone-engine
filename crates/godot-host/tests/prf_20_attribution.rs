// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Cell attribution against shared synthetic textures and the installed masters.

use soe_esm::GlobalFormId;
use soe_godot_host::data_source::{
    BlockFormat, CellStats, DataSource, LoadedCell, LoadedSurface, SurfaceTextures, TextureData,
};
use soe_godot_host::mesh::{build_surface, MaterialDesc, MeshData, MeshOptions};
use soe_trace::{LoadStage, MemorySnapshot, System};
use std::sync::Arc;

#[test]
fn prf_20_cell_texture_memory_deduplicates_paths_and_counts_cube_faces() {
    let block = Arc::new(TextureData::Compressed {
        width: 4,
        height: 4,
        format: BlockFormat::Bc1,
        mips: 1,
        mip0_len: 8,
        bytes: vec![0; 8],
    });
    let cube = Arc::new(TextureData::Cubemap(Box::new(std::array::from_fn(|_| {
        (*block).clone()
    }))));
    let mut surfaces = Vec::new();
    for path in ["Textures/Shared.dds", "textures\\shared.DDS"] {
        let arrays = build_surface(
            &MeshData {
                positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                indices: vec![0, 1, 2],
                material: MaterialDesc {
                    diffuse: Some(path.into()),
                    ..MaterialDesc::default()
                },
                ..MeshData::default()
            },
            MeshOptions::default(),
        )
        .unwrap();
        surfaces.push(LoadedSurface {
            animation: None,
            reference: GlobalFormId { file: 0, object: 1 },
            arrays,
            textures: SurfaceTextures {
                diffuse: Some(Arc::clone(&block)),
                ..SurfaceTextures::default()
            },
            placement: soe_physics::Iso::IDENTITY,
            instance_transform: None,
            mesh_identity: None,
        });
    }
    let cell = LoadedCell {
        stats: CellStats::default(),
        surfaces,
    };
    assert_eq!(cell.texture_inventory().values().copied().sum::<usize>(), 8);
    assert_eq!(cell.texture_inventory().len(), 1);
    assert_eq!(cube.payload_bytes(), 48);
}

#[test]
fn prf_20_installed_bannered_mare_and_whiterun_report_every_category_and_texture_sum() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open_with_threads(std::path::Path::new(&data), 1).unwrap();
    let game = soe_game::Game::from_shared_order(source.shared_order());
    let plugins = source.record_memory_by_plugin();
    assert_eq!(plugins.len(), 5);
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        assert!(plugins[name] > 0, "{name}");
    }
    for name in ["WhiterunBanneredMare", "WhiterunOrigin"] {
        source.clear_prepared_textures();
        let id = source.resolve_cell(name).unwrap();
        game.world_mut().load_cell(id);
        let loaded = if source.cell_index().is_interior(id) {
            source.stream_cell_id(id).unwrap()
        } else {
            let mut cells = source.stream_exterior(name, 0).unwrap().cells;
            assert_eq!(cells.len(), 1);
            cells.pop().unwrap().1
        };
        loaded.stats.profile.add_duration(
            LoadStage::Navmesh,
            game.world_mut().take_navigation_load_time(id),
        );
        {
            let _capture = loaded.stats.profile.enter();
            source.placed_cell_colliders(id).unwrap();
        }
        let report = loaded.stats.profile.report();
        for stage in LoadStage::ALL {
            assert!(
                report.diagnostic().contains(stage.name()),
                "{name}: {}",
                stage.name()
            );
            if stage != LoadStage::TextureUpload {
                assert!(
                    report.stage_ns[stage as usize] > 0,
                    "{name}: {}",
                    stage.name()
                );
            }
        }
        assert!(!report.archive_ns.is_empty());
        assert!(report
            .archive_ns
            .keys()
            .any(|path| std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("bsa"))));
        assert!(report.slowest_asset().is_some());
        assert_eq!(report.texture_bytes, loaded.texture_inventory());
        assert!(report.texture_total() > 0);
        assert_eq!(
            report.texture_total(),
            report.texture_bytes.values().copied().sum::<usize>()
        );
        eprintln!("PRF-20 {name}\n{}", report.diagnostic());
    }
    assert!(MemorySnapshot::now().live_bytes[System::Streaming as usize] > 0);
}

#[test]
fn prf_20_installed_host_upload_reports_cells_and_texture_totals() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return;
    }
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN for installed host check");
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
    assert!(stdout.contains("RESULT: PASS"), "{stdout}\n{stderr}");
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}
