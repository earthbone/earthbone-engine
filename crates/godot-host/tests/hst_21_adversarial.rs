// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial HST-21 scene check against installed Whiterun doors.

use soe_esm::{records::refr::Refr, Tag};
use soe_godot_host::data_source::DataSource;
use std::path::{Path, PathBuf};

#[test]
fn hst_21_installed_doors_keep_identity_and_clear_collision_when_open() {
    let data = std::env::var("SKYRIM_DATA").expect("installed Skyrim data is required");
    let source = DataSource::open(Path::new(&data)).unwrap();
    let order = source.shared_order();
    let mut manifest = Vec::new();

    for name in ["WhiterunBanneredMare", "WhiterunBreezehome"] {
        let cell = source
            .resolve_cell(name)
            .expect("installed Whiterun interior");
        let mut doors = Vec::new();
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
            if base_record.tag != Tag(*b"DOOR") || reference.teleport.is_none() {
                continue;
            }
            if !soe_game::World::authored_reference_enabled(&order, id)
                || base_record
                    .editor_id()
                    .is_some_and(|editor_id| editor_id.ends_with("MinUse"))
            {
                continue;
            }
            if name == "WhiterunBanneredMare" {
                assert_eq!(base_record.editor_id().as_deref(), Some("WRDragonDoor01"));
            }
            let position = reference.transform.expect("placed load door").position;
            assert!(!source.stream_reference(id).unwrap().surfaces.is_empty());
            assert!(source
                .placed_reference_colliders(id)
                .unwrap()
                .iter()
                .any(|placed| placed.collider.is_static()));
            doors.push((id, position));
        }
        doors.sort_by_key(|(id, _)| *id);
        assert_eq!(doors.len(), 1, "{name} has one front load door");
        let (id, position) = doors[0];
        manifest.push(serde_json::json!({
            "cell": name,
            "file": id.file,
            "object": id.object,
            "position": position
        }));
    }

    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(
        target.join("hst_21_doors.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();

    let godot = std::env::var_os("SOE_GODOT_BIN").expect("Godot 4.4 binary is required");
    let output = std::process::Command::new(godot)
        .arg("--path")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "hst_21_resident_doors_test.gd",
        ])
        .output()
        .expect("run the installed Whiterun door scene");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS hst_21_resident_doors"),
        "installed door scene failed\n{stdout}\n{stderr}"
    );
}
