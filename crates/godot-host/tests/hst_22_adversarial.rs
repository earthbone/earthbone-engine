// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;

use godot::classes::physics_server_3d::BodyMode;
use soe_esm::records::refr::Refr;
use soe_godot_host::data_source::DataSource;

#[test]
fn hst_22_ck_motion_overrides_keep_dynamic_codes_simulated() {
    for code in [1, 2, 3, 6] {
        assert_eq!(
            soe_godot_host::godot_physics::motion_type_to_body_mode(code),
            Some(BodyMode::RIGID),
            "CK dynamic motion code {code} must simulate"
        );
    }
    assert_eq!(
        soe_godot_host::godot_physics::motion_type_to_body_mode(4),
        Some(BodyMode::KINEMATIC)
    );
    assert_eq!(
        soe_godot_host::godot_physics::motion_type_to_body_mode(5),
        Some(BodyMode::STATIC)
    );
    assert_eq!(
        soe_godot_host::godot_physics::motion_type_to_body_mode(7),
        Some(BodyMode::KINEMATIC)
    );
    for invalid in [0, 8, 9, u32::MAX] {
        assert_eq!(
            soe_godot_host::godot_physics::motion_type_to_body_mode(invalid),
            None,
            "unknown CK motion code {invalid} must not make a rigid body"
        );
    }
}

#[test]
fn hst_22_installed_tankard_hold_collision_drop_and_tap_scenario() {
    let data = std::env::var_os("SKYRIM_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                "/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
            )
        });
    assert!(data.is_dir(), "installed Skyrim data is required: {}", data.display());
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN is required");
    let source = DataSource::open(&data).expect("installed plugin and archive data");
    let order = source.shared_order();
    let cell = source
        .resolve_cell("WhiterunBanneredMare")
        .expect("Bannered Mare cell");
    let (placed, _) = source
        .placed_cell_colliders(cell)
        .expect("Bannered Mare placed collision");
    let tankard = placed
        .iter()
        .find(|placed| {
            let Some(winner) = order.winner_ref(placed.reference) else {
                return false;
            };
            let Ok(reference) = Refr::from_record(order.record(winner)) else {
                return false;
            };
            let base = order.resolve(winner.file, reference.base);
            order
                .winner(base)
                .and_then(|record| record.editor_id())
                .is_some_and(|name| name.to_ascii_lowercase().contains("tankard"))
                && !placed.collider.is_static()
        })
        .expect("dynamic Bannered Mare tankard with bhk collision");

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(godot)
        .arg("--path")
        .arg(root.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "hst_22_carry_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .env("SOE_HST_22_FILE", tankard.reference.file.to_string())
        .env("SOE_HST_22_OBJECT", tankard.reference.object.to_string())
        .output()
        .expect("launch HST-22 Godot scene");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report = stdout
        .lines()
        .chain(stderr.lines())
        .filter(|line| line.contains("HST-22") || line.contains("SCRIPT ERROR"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        output.status.success()
            && stdout.contains("HST-22 PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "{report}\nFull output: target/hst-22-scene.log"
    );
}
