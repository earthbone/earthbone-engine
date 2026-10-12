// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "godot")]
#![allow(clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;

use soe_esm::records::refr::Refr;
use soe_godot_host::data_source::DataSource;

#[test]
fn hst_18_installed_host_crosshair_grab_and_papyrus_getters_track_each_frame() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA unset, skipping HST-18 installed host check");
        return;
    };
    assert!(
        data.is_dir(),
        "installed Skyrim data is required: {}",
        data.display()
    );
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("SOE_GODOT_BIN unset, skipping HST-18 installed host check");
        return;
    };
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
        .expect("dynamic Bannered Mare tankard with bhk collision");

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(godot)
        .arg("--path")
        .arg(root.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "hst_18_crosshair_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .env("SOE_HST_18_FILE", target.reference.file.to_string())
        .env("SOE_HST_18_OBJECT", target.reference.object.to_string())
        .output()
        .expect("launch HST-18 Godot host check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    std::fs::write(
        root.join("target/hst-18-scene.log"),
        format!("{stdout}\n{stderr}"),
    )
    .expect("write HST-18 host output");
    let report = stdout
        .lines()
        .chain(stderr.lines())
        .filter(|line| line.contains("HST-18") || line.contains("SCRIPT ERROR"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        output.status.success()
            && stdout.contains("HST-18 PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "{report}\nFull output: target/hst-18-scene.log"
    );
}
