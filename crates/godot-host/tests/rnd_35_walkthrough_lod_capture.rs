// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed RND-35 walkthrough startup, player LOD and border coverage captures.

use std::{path::PathBuf, process::Command};

#[test]
fn rnd_35_installed_walkthrough_masks_lod_at_startup_and_two_cell_borders() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping installed capture");
        return;
    };
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN required for capture");
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = workspace.join("godot");
    assert!(workspace
        .join("target/release/libsoe_godot_host.so")
        .is_file());
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args(["--resolution", "640x360", "--script"])
        .arg(project.join("rnd_35_walkthrough_lod_capture.gd"))
        .env("SKYRIM_DATA", data)
        .env("SOE_GODOT_TIMEOUT", "1800")
        .env("SOE_LOG", "off")
        .env_remove("SOE_CAPTURE_PATH")
        .env_remove("SOE_AUD_11_TEST")
        .output()
        .expect("start windowed Godot");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = project.join(".godot/rnd-35-walkthrough.log");
    std::fs::write(&log, format!("{stdout}\n{stderr}")).unwrap();
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS: RND-35 walkthrough LOD coverage"),
        "capture failed, log {}\n{stdout}\n{stderr}",
        log.display()
    );
    for stop in ["startup", "city-gate", "plains", "border-1", "border-2"] {
        for view in ["normal", "coverage", "lod-only"] {
            assert!(project
                .join(format!(".godot/rnd-35-{stop}-{view}.png"))
                .is_file());
        }
    }
    for line in stdout.lines().filter(|line| line.starts_with("RND-35")) {
        println!("{line}");
    }
}

#[test]
fn rnd_35_installed_radius_two_land_and_placed_lod_survey() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = soe_godot_host::data_source::DataSource::open(&PathBuf::from(data)).unwrap();
    let loaded = source.stream_exterior("WhiterunOrigin", 2).unwrap();
    let mut land = 0;
    for (cell, loaded) in loaded.cells {
        for surface in loaded.surfaces {
            if surface
                .arrays
                .material
                .diffuse
                .as_deref()
                .is_some_and(|p| p.starts_with("terrain:"))
            {
                land += 1;
            }
            if surface.arrays.material.shader_flags_2 & soe_nif::blocks::flags2::LOD_LANDSCAPE != 0
            {
                println!(
                    "RND-35 placed landscape LOD: cell={},{} reference={} bounds={:?} diffuse={:?}",
                    cell.x,
                    cell.y,
                    surface.reference,
                    surface.arrays.bounds,
                    surface.arrays.material.diffuse
                );
            }
        }
    }
    assert_eq!(land, 25, "radius-two startup has all LAND cells");
}
