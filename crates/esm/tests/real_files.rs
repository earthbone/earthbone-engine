// SPDX-License-Identifier: GPL-3.0-or-later

//! Loads the five base masters from a real install and prints record counts
//! per signature in a form that diffs against xEdit's xDump output. Runs
//! only when `SKYRIM_DATA` points at the game's `Data` folder:
//!
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-esm --test real_files -- --nocapture
//! ```
//!
//! Success criterion from the plan: every CELL and REFR under Tamriel
//! resolves to a form, and every REFR's base object is in the table.

use std::path::PathBuf;

use soe_esm::records::{cell::Cell, land::Land, navm::NavMesh, refr::Refr, stat::Static, wrld::World};
use soe_esm::{tag, LoadOrder, Plugin};

const MASTERS: [&str; 5] = [
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
];

#[test]
fn the_base_masters_load_and_tamriel_resolves() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the real-file test");
        return;
    };
    let mut order = LoadOrder::new();
    for name in MASTERS {
        let path = data.join(name);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let started = std::time::Instant::now();
        let plugin = Plugin::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        eprintln!(
            "{name}: {} records, {} bytes, {:.2}s, version {}, light={}, localized={}, masters={:?}",
            plugin.records.len(),
            bytes.len(),
            started.elapsed().as_secs_f32(),
            plugin.header.version,
            plugin.header.is_light(),
            plugin.header.is_localized(),
            plugin.header.masters
        );
        assert!(!plugin.to_bytes().is_empty());
        for (t, n) in plugin.counts() {
            eprintln!("{name}\t{t}\t{n}");
        }
        order.add(name, plugin).unwrap();
    }
    eprintln!("form table: {} ids", order.len());

    let tamriel = order
        .winners_of(tag::WRLD)
        .find(|(_, r)| r.editor_id().as_deref() == Some("Tamriel"))
        .map(|(id, _)| id)
        .expect("Tamriel exists");
    let tamriel_raw = order.record(order.overrides(tamriel)[0]).form_id;
    let world = World::from_record(order.winner(tamriel).unwrap(), order.file(0).plugin.header.is_localized()).unwrap();
    eprintln!("Tamriel: {tamriel} bounds {:?} to {:?}", world.bounds_min, world.bounds_max);

    let mut cells = 0;
    let mut refs = 0;
    let mut unresolved = 0;
    let mut lands = 0;
    let mut navmeshes = 0;
    for (file_index, file) in order.files().iter().enumerate() {
        let localized = file.plugin.header.is_localized();
        for rec in &file.plugin.records {
            if rec.placement.world != Some(tamriel_raw) && !(rec.tag == tag::WRLD && rec.form_id == tamriel_raw) {
                continue;
            }
            match rec.tag {
                tag::CELL => {
                    Cell::from_record(rec, localized).unwrap_or_else(|e| panic!("{} {:08X}: {e}", file.name, rec.form_id));
                    assert!(order.contains(order.resolve(file_index, rec.form_id)));
                    cells += 1;
                }
                tag::REFR => {
                    let r = Refr::from_record(rec).unwrap_or_else(|e| panic!("{} {:08X}: {e}", file.name, rec.form_id));
                    if !order.contains(order.resolve(file_index, r.base)) {
                        unresolved += 1;
                    }
                    refs += 1;
                }
                tag::LAND => {
                    let l = Land::from_record(rec).unwrap_or_else(|e| panic!("{} {:08X}: {e}", file.name, rec.form_id));
                    if let Some(h) = &l.heights {
                        let decoded = h.decode();
                        assert!(decoded.iter().all(|v| v.is_finite()));
                    }
                    lands += 1;
                }
                tag::NAVM => {
                    NavMesh::from_record(rec).unwrap_or_else(|e| panic!("{} {:08X}: {e}", file.name, rec.form_id));
                    navmeshes += 1;
                }
                _ => {}
            }
        }
    }
    let statics = order
        .winners_of(tag::STAT)
        .map(|(_, r)| Static::from_record(r).map(|_| ()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .len();
    eprintln!("Tamriel: {cells} cell records, {refs} references, {unresolved} unresolved bases, {lands} land, {navmeshes} navmesh, {statics} statics in the table");
    assert!(cells > 1000);
    assert!(refs > 100_000);
    assert_eq!(unresolved, 0, "every reference base must be in the form table");
}
