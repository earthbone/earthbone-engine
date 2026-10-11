// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "godot")]
#![allow(clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;

use soe_esm::records::refr::Refr;
use soe_godot_host::data_source::DataSource;

#[test]
fn hst_18_live_crosshair_and_grabbed_getters_follow_host_state() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA unset, skipping HST-18 host check");
        return;
    };
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN is required");
    let source = DataSource::open(&data).expect("installed plugin and archive data");
    let order = source.shared_order();
    let cell = source
        .resolve_cell("WhiterunBanneredMare")
        .expect("Bannered Mare cell");
    let (placed, _) = source
        .placed_cell_colliders(cell)
        .expect("Bannered Mare placed collision");
    let target = placed
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
        .expect("dynamic Bannered Mare tankard with collision");

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(godot)
        .arg("--path")
        .arg(root.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "hst_18_adversarial_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .env("SOE_HST_18_FILE", target.reference.file.to_string())
        .env("SOE_HST_18_OBJECT", target.reference.object.to_string())
        .output()
        .expect("launch HST-18 Godot host check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("HST-18 PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "HST-18 host check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
