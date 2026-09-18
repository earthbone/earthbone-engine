// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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

use soe_esm::records::{
    cell::Cell, land::Land, navm::NavMesh, refr::Refr, stat::Static, wrld::World,
};
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
    let world = World::from_record(
        order.winner(tamriel).unwrap(),
        order.file(0).plugin.header.is_localized(),
    )
    .unwrap();
    eprintln!(
        "Tamriel: {tamriel} bounds {:?} to {:?}",
        world.bounds_min, world.bounds_max
    );

    let mut cells = 0;
    let mut refs = 0;
    let mut unresolved = 0;
    let mut lands = 0;
    let mut navmeshes = 0;
    for (file_index, file) in order.files().iter().enumerate() {
        let localized = file.plugin.header.is_localized();
        for rec in &file.plugin.records {
            if rec.placement.world != Some(tamriel_raw)
                && !(rec.tag == tag::WRLD && rec.form_id == tamriel_raw)
            {
                continue;
            }
            match rec.tag {
                tag::CELL => {
                    Cell::from_record(rec, localized)
                        .unwrap_or_else(|e| panic!("{} {:08X}: {e}", file.name, rec.form_id));
                    assert!(order.contains(order.resolve(file_index, rec.form_id)));
                    cells += 1;
                }
                tag::REFR => {
                    let r = Refr::from_record(rec)
                        .unwrap_or_else(|e| panic!("{} {:08X}: {e}", file.name, rec.form_id));
                    if !order.contains(order.resolve(file_index, r.base)) {
                        unresolved += 1;
                    }
                    refs += 1;
                }
                tag::LAND => {
                    let l = Land::from_record(rec)
                        .unwrap_or_else(|e| panic!("{} {:08X}: {e}", file.name, rec.form_id));
                    if let Some(h) = &l.heights {
                        let decoded = h.decode();
                        assert!(decoded.iter().all(|v| v.is_finite()));
                    }
                    lands += 1;
                }
                tag::NAVM => {
                    NavMesh::from_record(rec)
                        .unwrap_or_else(|e| panic!("{} {:08X}: {e}", file.name, rec.form_id));
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
    assert_eq!(
        unresolved, 0,
        "every reference base must be in the form table"
    );
}

/// Builds the cell index over the installed masters, times it, and counts
/// what it holds for the Bannered Mare and for the 3 by 3 exterior block
/// around WhiterunOrigin. Run with `--nocapture` to see the numbers.
#[test]
fn the_cell_index_covers_whiterun() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the cell index test");
        return;
    };
    let mut order = LoadOrder::new();
    for name in MASTERS {
        let path = data.join(name);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let plugin = Plugin::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        order.add(name, plugin).unwrap();
    }
    let started = std::time::Instant::now();
    order.build_cell_index();
    let build = started.elapsed();
    let index = order.cell_index().unwrap();
    eprintln!(
        "cell index: {} cells, {} placed objects, {} grid cells, {} worldspaces, built in {:.3}s",
        index.cell_count(),
        index.placed_count(),
        index.grid_count(),
        index.world_count(),
        build.as_secs_f64()
    );
    assert!(index.cell_count() > 10_000);
    assert!(index.placed_count() > 500_000);

    let mare = index
        .cell_by_editor_id("WhiterunBanneredMare")
        .expect("WhiterunBanneredMare exists");
    assert!(index.is_interior(mare));
    let placed = index.references(mare);
    let plain_references = placed
        .iter()
        .filter(|id| order.winner(**id).is_some_and(|r| r.tag == tag::REFR))
        .count();
    eprintln!(
        "WhiterunBanneredMare {mare}: {} placed objects, {plain_references} REFR, {} persistent, {} navmeshes",
        placed.len(),
        index.persistent_references(mare).len(),
        index.navmeshes(mare).len()
    );
    assert!(plain_references > 100);
    for id in placed {
        let winner = order.winner_ref(*id).expect("indexed ids have winners");
        let rec = order.record(winner);
        assert_eq!(
            order.resolve(winner.file, rec.placement.cell.unwrap()),
            mare,
            "{id} is filed under the wrong cell"
        );
    }

    let origin = index
        .cell_by_editor_id("WhiterunOrigin")
        .expect("WhiterunOrigin exists");
    let grid = index.grid_of(origin).expect("WhiterunOrigin is exterior");
    let world = order
        .winner(grid.world)
        .expect("the worldspace has a record");
    eprintln!(
        "WhiterunOrigin {origin}: worldspace {} ({:?}) at ({}, {})",
        grid.world,
        world.editor_id(),
        grid.x,
        grid.y
    );
    assert_eq!(world.tag, tag::WRLD);
    let persistent_cell = index
        .world_persistent_cell(grid.world)
        .expect("the worldspace has a persistent cell");
    eprintln!(
        "worldspace persistent cell {persistent_cell}: {} placed objects",
        index.references(persistent_cell).len()
    );
    let mut block_refs = 0;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let cell = index
                .exterior_cell(grid.world, grid.x + dx, grid.y + dy)
                .unwrap_or_else(|| panic!("no cell at ({}, {})", grid.x + dx, grid.y + dy));
            let refs = index.references(cell).len();
            block_refs += refs;
            eprintln!(
                "  ({}, {}) {cell}: {refs} placed objects, land {}, {} navmeshes",
                grid.x + dx,
                grid.y + dy,
                index.land(cell).is_some(),
                index.navmeshes(cell).len()
            );
        }
    }
    eprintln!("3 by 3 block around WhiterunOrigin: {block_refs} placed objects");
    assert!(block_refs > 500);

    let lookups = 100_000;
    let started = std::time::Instant::now();
    let mut total = 0usize;
    for _ in 0..lookups {
        total += index.references(mare).len();
    }
    let per_lookup = started.elapsed() / lookups;
    eprintln!("lookup: {per_lookup:?} per call ({total} ids summed)");
}
