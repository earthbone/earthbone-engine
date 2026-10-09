// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for landscape LOD lighting and the LAND/BTR seam.

use std::{path::PathBuf, process::Command};

use soe_godot_host::data_source::DataSource;
use soe_godot_host::mesh::{MaterialDesc, MaterialKey};
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel};
use soe_nif::blocks::{flags1, flags2};

#[test]
fn rnd_26_lod_shading_requires_both_normal_space_and_landscape_flags() {
    let cases = [
        (0, 0, false),
        (flags1::MODEL_SPACE_NORMALS, 0, false),
        (0, flags2::LOD_LANDSCAPE, false),
        (flags1::MODEL_SPACE_NORMALS, flags2::LOD_LANDSCAPE, true),
        (
            flags1::MODEL_SPACE_NORMALS | flags1::ZBUFFER_TEST,
            flags2::LOD_LANDSCAPE | flags2::ZBUFFER_WRITE,
            true,
        ),
    ];
    for (first, second, expected) in cases {
        let material = MaterialDesc {
            shader_flags_1: first,
            shader_flags_2: second,
            ..Default::default()
        };
        assert_eq!(
            material.landscape_lod_normals(),
            expected,
            "{first:#x} {second:#x}"
        );
    }
}

#[test]
fn rnd_26_noise_cache_key_only_changes_for_active_landscape_lod() {
    for flags in [
        (0, 0),
        (flags1::MODEL_SPACE_NORMALS, 0),
        (0, flags2::LOD_LANDSCAPE),
    ] {
        let plain = MaterialDesc {
            shader_flags_1: flags.0,
            shader_flags_2: flags.1,
            ..Default::default()
        };
        let mut with_noise = plain.clone();
        with_noise.landscape_lod_noise = true;
        assert_eq!(MaterialKey::from(&plain), MaterialKey::from(&with_noise));
    }

    let lod = MaterialDesc {
        shader_flags_1: flags1::MODEL_SPACE_NORMALS,
        shader_flags_2: flags2::LOD_LANDSCAPE,
        ..Default::default()
    };
    let mut noise_lod = lod.clone();
    noise_lod.landscape_lod_noise = true;
    assert_ne!(MaterialKey::from(&lod), MaterialKey::from(&noise_lod));
}

#[test]
fn rnd_26_installed_lod_textures_match_their_shader_flags() {
    let data = std::env::var_os("SKYRIM_DATA")
        .map(PathBuf::from)
        .expect("SKYRIM_DATA");
    let source = DataSource::open(&data).expect("installed data opens");
    let (world, _) = source.resolve_world("Tamriel").expect("Tamriel resolves");
    for (x, y) in [(0, -8), (4, -8), (8, -8), (0, -4), (4, -4)] {
        let block = LodBlockCoord {
            level: LodLevel::L4,
            x,
            y,
        };
        let loaded = source
            .load_lod_block(world, "Tamriel", block)
            .expect("BTR loads")
            .expect("BTR exists");
        let mut count = 0;
        for surface in &loaded.surfaces {
            let material = &surface.arrays.material;
            if !material.landscape_lod_normals() {
                assert!(!material.landscape_lod_noise);
                continue;
            }
            count += 1;
            assert!(surface.textures.diffuse.is_some(), "{block:?} diffuse");
            assert!(surface.textures.normal.is_some(), "{block:?} normal");
            assert!(surface.textures.terrain_noise.is_some(), "{block:?} noise");
            assert!(material.landscape_lod_noise, "{block:?} feature 18 noise");
            assert_eq!(surface.arrays.normals.len(), surface.arrays.positions.len());
            assert!(surface
                .arrays
                .normals
                .iter()
                .flatten()
                .all(|n| n.is_finite()));
        }
        assert!(count > 0, "{block:?} contains no landscape LOD surface");
    }
}

#[test]
fn rnd_26_installed_capture_checks_lighting_and_land_lod_seam() {
    let data = std::env::var_os("SKYRIM_DATA").expect("SKYRIM_DATA");
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN");
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    assert!(
        project
            .join("../target/release/libsoe_godot_host.so")
            .is_file(),
        "release extension must be built before capture"
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
        .expect("Godot capture starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("rnd_26_lod_lighting: PASS"),
        "Godot capture failed: {stdout}\n{stderr}"
    );
}
