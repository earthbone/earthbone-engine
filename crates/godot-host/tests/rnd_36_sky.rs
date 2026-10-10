// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Night-layer and zenith checks using installed CLMT/WTHR and sky NIFs.

use std::{path::PathBuf, process::Command, sync::Mutex};

use soe_godot_host::data_source::DataSource;

static CAPTURE: Mutex<()> = Mutex::new(());

#[test]
fn rnd_36_synthetic_night_layers_close_at_zenith_and_stay_behind_geometry() {
    capture("synthetic");
}

#[test]
fn rnd_36_installed_whiterun_noon_midnight_night_layers_and_zenith() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return;
    }
    capture("installed");
}

fn capture(case: &str) {
    let _guard = CAPTURE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("SOE_GODOT_BIN unset, skipping sky capture");
        return;
    };
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    assert!(
        project
            .join("../target/release/libsoe_godot_host.so")
            .is_file(),
        "build release extension and import project before captures"
    );
    let output = Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "rnd_36_sky_capture_test.gd",
        ])
        .env("SOE_RND36_CASE", case)
        .output()
        .expect("run RND-36 windowed capture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    std::fs::create_dir_all(project.join("../target/rnd_36")).unwrap();
    std::fs::write(
        project.join(format!("../target/rnd_36/{case}.log")),
        format!("{stdout}\n{stderr}"),
    )
    .unwrap();
    assert!(
        output.status.success()
            && stdout.contains("RND-36 RESULT: PASS")
            && !stderr.contains("SCRIPT ERROR:")
            && !stderr.contains("SHADER ERROR:"),
        "RND-36 {case} failed\n{stdout}\n{stderr}"
    );
    println!("{stdout}");
}

#[test]
fn rnd_36_installed_night_layers_and_climate_fade() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        return;
    };
    let source = DataSource::open(&data).unwrap();
    let mut world = soe_game::World::from_shared_order(source.shared_order());
    let cell = world.cell_by_editor_id("WhiterunOrigin").unwrap();
    let climate_id = world.sky_climate_for_cell(cell).unwrap().unwrap();
    let climate = world.sky_climate(climate_id).unwrap();
    assert_eq!(climate.night_model, "Sky/Stars.nif");
    let mut vfs = soe_bsa::Vfs::new();
    for name in ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"] {
        vfs.add_archive_path(&data.join(name)).unwrap();
    }
    let bytes = vfs
        .read(&format!("meshes/{}", climate.night_model))
        .unwrap();
    let file = soe_nif::parse(&bytes).unwrap();
    let meshes = file.meshes().unwrap();
    assert_eq!(meshes.len(), 4);
    for (name, texture) in [
        ("Constellations1-6", "SkyrimConstellations01.dds"),
        ("Constellations7-13", "SkyrimConstellations02.dds"),
        ("SkyrimGalaxy", "SkyrimGalaxy.dds"),
        ("SkyrimStars:0", "Stars.dds"),
    ] {
        let mesh = meshes.iter().find(|mesh| mesh.name == name).unwrap();
        assert_eq!(mesh.positions.len(), mesh.uvs.len());
        assert!(!mesh.indices.is_empty());
        let shader = match &file.blocks[mesh.block] {
            soe_nif::blocks::Block::BsTriShape(shape) => shape.shader,
            soe_nif::blocks::Block::TriShape(shape) => shape.shader,
            _ => panic!("night layer has no shape"),
        };
        assert!(matches!(&file.blocks[shader.get().unwrap()],
            soe_nif::blocks::Block::Unknown { type_name, data }
                if type_name == "BSSkyShaderProperty" && data.windows(texture.len()).any(|window| window == texture.as_bytes())));
        println!(
            "RND-36 layer={name} texture={texture} vertices={} triangles={}",
            mesh.positions.len(),
            mesh.indices.len() / 3
        );
        println!(
            "RND-36 {name} colors={:?}",
            &mesh.colors[..mesh.colors.len().min(3)]
        );
    }
    for weather in ["SkyrimClear", "SkyrimCloudy"] {
        let weather = world.form(weather).unwrap();
        let [rise_begin, rise_end, set_begin, set_end] = climate.timing;
        for (hour, expected) in [
            (0.0, 1.0),
            (rise_begin, 1.0),
            (rise_begin.midpoint(rise_end), 0.5),
            (rise_end, 0.0),
            (12.0, 0.0),
            (set_begin, 0.0),
            (set_begin.midpoint(set_end), 0.5),
            (set_end, 1.0),
            (24.0, 1.0),
        ] {
            let frame = world.sample_sky(weather, &climate, hour, 0.0).unwrap();
            assert!(
                (frame.night_alpha - expected).abs() < 0.00001,
                "CLMT night fade at {hour}"
            );
            assert_eq!(frame.night_model, climate.night_model);
        }
    }
}
