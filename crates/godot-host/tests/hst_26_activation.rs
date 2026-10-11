// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-26 installed Bannered Mare acceptance through the walkthrough startup.

use soe_esm::records::refr::Refr;
use soe_godot_host::data_source::DataSource;
use std::path::Path;
use std::process::Command;

#[test]
fn hst_26_bannered_mare_walkthrough_pickup_book_and_activator() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let order = source.shared_order();
    let cell = source.resolve_cell("WhiterunBanneredMare").unwrap();
    let (colliders, _) = source.placed_cell_colliders(cell).unwrap();
    let tankard = colliders
        .iter()
        .find(|placed| {
            if placed.collider.is_static() {
                return false;
            }
            let winner = order.winner_ref(placed.reference).unwrap();
            let record = order.record(winner);
            if record.tag != soe_esm::tag::REFR {
                return false;
            }
            let reference = Refr::from_record(record).unwrap();
            let base = order.resolve(winner.file, reference.base);
            order
                .winner(base)
                .and_then(|record| record.editor_id())
                .is_some_and(|name| name.to_ascii_lowercase().contains("tankard"))
        })
        .expect("Bannered Mare dynamic tankard");
    let find = |kind: &[u8; 4]| {
        source
            .cell_index()
            .references(cell)
            .iter()
            .copied()
            .find(|id| {
                if !soe_game::World::authored_reference_enabled(&order, *id) {
                    return false;
                }
                let Some(winner) = order.winner_ref(*id) else {
                    return false;
                };
                let record = order.record(winner);
                if record.tag != soe_esm::tag::REFR {
                    return false;
                }
                let Ok(reference) = Refr::from_record(record) else {
                    return false;
                };
                order
                    .winner(order.resolve(winner.file, reference.base))
                    .is_some_and(|record| {
                        record.tag.as_bytes() == kind
                            && (kind != b"BOOK"
                                || record
                                    .find(soe_esm::Tag(*b"DATA"))
                                    .and_then(|data| data.first())
                                    .is_none_or(|flags| flags & 2 == 0))
                    })
            })
            .expect("Bannered Mare requested reference type")
    };
    let references = [tankard.reference, find(b"BOOK"), find(b"ACTI")];
    let mut command = Command::new(
        std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN for installed host check"),
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    command
        .arg("--headless")
        .arg("--path")
        .arg(root.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "hst_26_activation_test.gd",
        ]);
    for (name, reference) in ["ITEM", "BOOK", "ACTI"].into_iter().zip(references) {
        command.env(
            format!("SOE_HST_26_{name}_FILE"),
            reference.file.to_string(),
        );
        command.env(
            format!("SOE_HST_26_{name}_OBJECT"),
            reference.object.to_string(),
        );
    }
    let output = command.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    std::fs::create_dir_all(root.join("target")).unwrap();
    std::fs::write(
        root.join("target/hst-26-scene.log"),
        format!("{stdout}\n{stderr}"),
    )
    .unwrap();
    assert!(
        output.status.success()
            && stdout.contains("HST-26 PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "{stdout}\n{stderr}"
    );
}
