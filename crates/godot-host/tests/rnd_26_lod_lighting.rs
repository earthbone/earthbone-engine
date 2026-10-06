// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-26 landscape LOD shading, using installed BTR files through DataSource.

use std::{path::PathBuf, process::Command};

use soe_godot_host::data_source::DataSource;
use soe_godot_host::mesh::{MaterialDesc, MaterialKey};
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel};
use soe_nif::blocks::{flags1, flags2};

#[test]
fn rnd_26_lod_normal_space_keeps_material_cache_identity() {
    let plain = MaterialDesc {
        diffuse: Some("terrain.dds".into()),
        normal: Some("terrain_n.dds".into()),
        ..Default::default()
    };
    let mut lod = plain.clone();
    lod.shader_flags_1 = flags1::MODEL_SPACE_NORMALS;
    lod.shader_flags_2 = flags2::LOD_LANDSCAPE;
    assert!(lod.landscape_lod_normals());
    assert_ne!(MaterialKey::from(&plain), MaterialKey::from(&lod));
    let without_noise = MaterialKey::from(&lod);
    lod.landscape_lod_noise = true;
    assert_ne!(without_noise, MaterialKey::from(&lod));
    for (first, second) in [(flags1::MODEL_SPACE_NORMALS, 0), (0, flags2::LOD_LANDSCAPE)] {
        lod.shader_flags_1 = first;
        lod.shader_flags_2 = second;
        assert!(!lod.landscape_lod_normals());
    }
}

#[test]
fn rnd_26_installed_lod_has_diffuse_normal_and_valid_geometry() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        return;
    };
    let source = DataSource::open(&data).unwrap();
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_archive_path(&data.join("Skyrim - Meshes0.bsa"))
        .unwrap();
    vfs.add_archive_path(&data.join("Skyrim - Meshes1.bsa"))
        .unwrap();
    let bytes = vfs
        .read("meshes/terrain/tamriel/tamriel.4.4.-8.btr")
        .unwrap();
    assert!(soe_nif::parse(&bytes)
        .unwrap()
        .meshes()
        .unwrap()
        .iter()
        .any(|mesh| {
            mesh.material.kind
                == soe_nif::mesh::MaterialKind::Lighting(
                    soe_nif::blocks::ShaderType::LodLandscapeNoise,
                )
        }));
    let (world, _) = source.resolve_world("Tamriel").unwrap();
    for (x, y) in [(0, -8), (4, -8), (8, -8), (0, -4), (4, -4)] {
        let block = LodBlockCoord {
            level: LodLevel::L4,
            x,
            y,
        };
        let loaded = source
            .load_lod_block(world, "Tamriel", block)
            .unwrap()
            .unwrap();
        let mut landscape_count = 0;
        for surface in &loaded.surfaces {
            if !surface.arrays.material.landscape_lod_normals() {
                continue;
            }
            landscape_count += 1;
            assert!(surface.textures.diffuse.is_some());
            assert!(surface.textures.normal.is_some());
            assert!(surface.arrays.material.landscape_lod_noise);
            assert!(surface.textures.terrain_noise.is_some());
            assert_eq!(surface.arrays.normals.len(), surface.arrays.positions.len());
            assert!(surface
                .arrays
                .normals
                .iter()
                .all(|n| n.iter().all(|v| v.is_finite())
                    && (n.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() < 0.001));
        }
        assert!(landscape_count > 0, "{block:?} has no landscape surface");
    }
}

#[test]
fn rnd_26_installed_whiterun_wall_capture_bounds_lod_seam_brightness() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    assert!(
        project
            .join("../target/release/libsoe_godot_host.so")
            .is_file(),
        "build release extension and import project first"
    );
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "rnd_26_lod_lighting.gd",
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("RND-26 Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = project.join(".godot/rnd_26-lod-lighting.log");
    std::fs::write(&log, format!("{stdout}\n{stderr}")).unwrap();
    let diagnostics = stdout
        .lines()
        .chain(stderr.lines())
        .filter(|line| {
            line.starts_with("rnd_26")
                || line.starts_with("FAIL:")
                || line.starts_with("SCRIPT ERROR:")
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        output.status.success() && stdout.contains("rnd_26_lod_lighting: PASS"),
        "RND-26 capture failed, {}\n{diagnostics}",
        log.display()
    );
    assert!(project.join(".godot/rnd_26-whiterun-seam.png").is_file());
    println!("{diagnostics}");
}
