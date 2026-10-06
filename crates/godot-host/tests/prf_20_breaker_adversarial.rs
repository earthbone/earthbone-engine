// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::GlobalFormId;
use soe_godot_host::data_source::{
    BlockFormat, CellStats, LoadedCell, LoadedSurface, SurfaceTextures, TextureData,
};
use soe_godot_host::mesh::{build_surface, MaterialDesc, MeshData, MeshOptions};
use soe_trace::{LoadStage, System};
use std::sync::Arc;

#[test]
fn prf_20_breaker_installed_cells_reach_the_godot_upload_and_asset_diagnostic_path() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN for installed host check");
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let output = std::process::Command::new(godot)
        .args(["--headless", "--path"])
        .arg(project)
        .args(["--script", "prf_20_attribution_test.gd"])
        .env("SKYRIM_DATA", data)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("RESULT: PASS"), "{stdout}\n{stderr}");
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}

#[test]
fn prf_20_breaker_texture_inventory_counts_distinct_assets_and_all_cubemap_faces() {
    let texture = Arc::new(TextureData::Compressed {
        width: 4,
        height: 4,
        format: BlockFormat::Bc1,
        mips: 1,
        mip0_len: 8,
        bytes: vec![0; 8],
    });
    let surfaces = ["textures/a.dds", "textures/b.dds"].map(|path| LoadedSurface {
        reference: GlobalFormId { file: 0, object: 1 },
        arrays: build_surface(
            &MeshData {
                positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                indices: vec![0, 1, 2],
                material: MaterialDesc {
                    diffuse: Some(path.into()),
                    ..Default::default()
                },
                ..Default::default()
            },
            MeshOptions::default(),
        )
        .unwrap(),
        textures: SurfaceTextures {
            diffuse: Some(Arc::clone(&texture)),
            ..Default::default()
        },
        placement: soe_physics::Iso::IDENTITY,
        instance_transform: None,
        mesh_identity: None,
    });
    let cell = LoadedCell {
        stats: CellStats::default(),
        surfaces: surfaces.into(),
    };
    assert_eq!(cell.texture_inventory().len(), 2);
    assert_eq!(cell.texture_inventory().values().sum::<usize>(), 16);
    assert_eq!(LoadStage::ALL.len(), 6);
    assert_eq!(System::ALL.len(), 14);
}
