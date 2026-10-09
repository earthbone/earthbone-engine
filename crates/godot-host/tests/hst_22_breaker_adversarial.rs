// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed dynamic clutter carry through the real Godot host.

use std::path::{Path, PathBuf};
use std::process::Command;

use soe_esm::records::refr::Refr;
use soe_godot_host::data_source::DataSource;

#[test]
fn hst_22_installed_tankard_carries_collides_drops_and_activates_on_tap() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA unset, skipping HST-22 installed breaker scenario");
        return;
    };
    assert!(
        data.is_dir(),
        "installed Skyrim data is required: {}",
        data.display()
    );
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN is required");
    let source = DataSource::open(&data).expect("installed plugin and archive data");
    let order = source.shared_order();
    let cell = source
        .resolve_cell("WhiterunBanneredMare")
        .expect("Bannered Mare cell");
    let (placed, _) = source
        .placed_cell_colliders(cell)
        .expect("placed collision");
    let tankard = placed
        .iter()
        .find(|item| {
            let Some(owner) = order.winner_ref(item.reference) else {
                return false;
            };
            let Ok(reference) = Refr::from_record(order.record(owner)) else {
                return false;
            };
            order
                .winner(order.resolve(owner.file, reference.base))
                .and_then(|base| base.editor_id())
                .is_some_and(|name| name.to_ascii_lowercase().contains("tankard"))
                && !item.collider.is_static()
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
        .expect("launch installed carry scene");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("HST-22 PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "{stdout}\n{stderr}"
    );
}
