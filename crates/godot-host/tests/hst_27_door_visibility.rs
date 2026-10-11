// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed door scene acceptance uses the production streamer and player collision.

use soe_esm::{records::refr::Refr, Tag};
use soe_godot_host::data_source::{DataSource, LoadedCell};
use soe_godot_host::units::SkyrimPos;
use std::path::{Path, PathBuf};

fn hinge_radius(loaded: &LoadedCell, position: [f32; 3]) -> f32 {
    let hinge = SkyrimPos::new(position[0], position[1], position[2]).to_godot();
    loaded
        .surfaces
        .iter()
        .flat_map(|surface| {
            assert!(surface.instance_transform.is_none());
            &surface.arrays.positions
        })
        .map(|point| {
            ((point[0] - hinge.x).powi(2)
                + (point[1] - hinge.y).powi(2)
                + (point[2] - hinge.z).powi(2))
            .sqrt()
        })
        .fold(0.0, f32::max)
}

#[test]
fn hst_27_installed_front_doors_stay_at_the_hinge_in_every_state_and_after_return() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let order = source.shared_order();
    let mut manifest = Vec::new();
    for name in ["WhiterunBanneredMare", "WhiterunBreezehome"] {
        let cell = source
            .resolve_cell(name)
            .expect("installed Whiterun interior");
        let mut doors = Vec::new();
        let mut swing_radii = serde_json::Map::new();
        for &id in source.cell_index().references(cell) {
            let Some(owner) = order.winner_ref(id) else {
                continue;
            };
            let record = order.record(owner);
            if record.tag != Tag(*b"REFR") {
                continue;
            }
            let Ok(reference) = Refr::from_record(record) else {
                continue;
            };
            let base = order.resolve(owner.file, reference.base);
            if order
                .winner(base)
                .is_none_or(|record| record.tag != Tag(*b"DOOR"))
            {
                continue;
            }
            let base_record = order.winner(base).unwrap();
            if !soe_game::World::authored_reference_enabled(&order, id)
                || (reference.teleport.is_some()
                    && base_record
                        .editor_id()
                        .is_some_and(|name| name.ends_with("MinUse")))
            {
                continue;
            }
            if reference.teleport.is_none() {
                let loaded = source.stream_reference(id).unwrap();
                let radius = hinge_radius(&loaded, reference.transform.unwrap().position);
                swing_radii.insert(
                    format!("{}:{}", id.file, id.object),
                    serde_json::json!(radius),
                );
                continue;
            }
            if name == "WhiterunBanneredMare" {
                assert_eq!(base_record.editor_id().as_deref(), Some("WRDragonDoor01"));
            }
            eprintln!(
                "hst_27 front door cell={name} id={id} base={:?}",
                base_record.editor_id()
            );
            let surfaces = source.stream_reference(id).unwrap();
            assert!(
                !surfaces.surfaces.is_empty(),
                "{name} front door has geometry"
            );
            assert!(
                source
                    .placed_reference_colliders(id)
                    .unwrap()
                    .iter()
                    .any(|placed| placed.collider.is_static()),
                "{name} front door has authored static collision"
            );
            doors.push((
                id,
                reference.transform.expect("placed front door").position,
                order.resolve(owner.file, reference.teleport.unwrap().door),
            ));
        }
        doors.sort_by_key(|(id, _, _)| *id);
        assert_eq!(doors.len(), 1, "{name} has one front load door");
        let (id, position, far_door) = doors[0];
        let radius = hinge_radius(&source.stream_reference(id).unwrap(), position);
        let far_owner = order.winner_ref(far_door).unwrap();
        let far_reference = Refr::from_record(order.record(far_owner)).unwrap();
        let far_radius = hinge_radius(
            &source.stream_reference(far_door).unwrap(),
            far_reference.transform.unwrap().position,
        );
        manifest.push(
            serde_json::json!({"cell": name, "file": id.file, "object": id.object,
            "position": position, "radius": radius, "far_file": far_door.file,
            "far_object": far_door.object, "far_radius": far_radius, "swing_radii": swing_radii}),
        );
    }
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(
        target.join("hst_27_doors.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let godot =
        std::env::var_os("SOE_GODOT_BIN").expect("installed scene check needs SOE_GODOT_BIN");
    let output = std::process::Command::new(godot)
        .arg("--path")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "hst_27_resident_doors_test.gd",
        ])
        .output()
        .expect("run windowed installed door scene after building and importing");
    let captures = target.join("hst_27");
    std::fs::create_dir_all(&captures).unwrap();
    std::fs::write(captures.join("stdout.txt"), &output.stdout).unwrap();
    std::fs::write(captures.join("stderr.txt"), &output.stderr).unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS hst_27_resident_doors"),
        "installed door scene failed\n{stdout}\n{stderr}"
    );
    println!("{stdout}");
}
