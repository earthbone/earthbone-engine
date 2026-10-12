// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "godot")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-27 installed doorway state and hinge-position regression.

use soe_esm::{records::refr::Refr, Tag};
use soe_godot_host::data_source::DataSource;
use std::path::{Path, PathBuf};

fn radius(loaded: &soe_godot_host::data_source::LoadedCell, position: [f32; 3]) -> f32 {
    let hinge =
        soe_godot_host::units::SkyrimPos::new(position[0], position[1], position[2]).to_godot();
    loaded
        .surfaces
        .iter()
        .flat_map(|surface| &surface.arrays.positions)
        .map(|point| {
            ((point[0] - hinge.x).powi(2)
                + (point[1] - hinge.y).powi(2)
                + (point[2] - hinge.z).powi(2))
            .sqrt()
        })
        .fold(0.0, f32::max)
}

#[test]
fn hst_27_installed_door_states_keep_the_leaf_visible_at_its_hinge() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let order = source.shared_order();
    let mut manifest = Vec::new();
    for name in ["WhiterunBanneredMare", "WhiterunBreezehome"] {
        let cell = source.resolve_cell(name).unwrap();
        let mut front = Vec::new();
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
            let Some(base_record) = order.winner(base) else {
                continue;
            };
            if base_record.tag != Tag(*b"DOOR") {
                continue;
            }
            if !soe_game::World::authored_reference_enabled(&order, id) {
                continue;
            }
            let position = reference.transform.unwrap().position;
            let loaded = source.stream_reference(id).unwrap();
            if reference.teleport.is_none() {
                if !base_record
                    .editor_id()
                    .is_some_and(|editor| editor.ends_with("MinUse"))
                {
                    swing_radii.insert(
                        format!("{}:{}", id.file, id.object),
                        serde_json::json!(radius(&loaded, position)),
                    );
                }
                continue;
            }
            front.push((
                id,
                position,
                radius(&loaded, position),
                order.resolve(owner.file, reference.teleport.unwrap().door),
            ));
        }
        front.sort_by_key(|entry| entry.0);
        assert_eq!(front.len(), 1, "{name} has one enabled front door");
        let (door, position, extent, far) = front[0];
        let far_record = order.record(order.winner_ref(far).unwrap());
        let far_ref = Refr::from_record(far_record).unwrap();
        let far_position = far_ref.transform.unwrap().position;
        let far_extent = radius(&source.stream_reference(far).unwrap(), far_position);
        manifest.push(serde_json::json!({"cell":name,"file":door.file,"object":door.object,"position":position,"radius":extent,"far_file":far.file,"far_object":far.object,"far_radius":far_extent,"swing_radii":swing_radii}));
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
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success() && log.contains("RESULT: PASS hst_27_resident_doors"),
        "{log}"
    );
}
