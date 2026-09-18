// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Streams the exterior block around `WhiterunOrigin` through `DataSource`
//! and batches it, against an installed Skyrim SE copy. Runs only when
//! `SKYRIM_DATA` points at the game's `Data` folder:
//!
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-godot-host --release --test real_exterior -- --nocapture
//! ```
//!
//! `SOE_BENCH_RADIUS` overrides the radius (default 1, the 3 by 3 block).

use std::path::PathBuf;

use soe_godot_host::batch::batch_surfaces;
use soe_godot_host::data_source::DataSource;

#[test]
fn the_whiterun_exterior_block_streams_and_batches() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the real-exterior test");
        return;
    };
    let radius: i32 = std::env::var("SOE_BENCH_RADIUS")
        .ok()
        .and_then(|r| r.parse().ok())
        .unwrap_or(1);

    let source = DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}"));
    let started = std::time::Instant::now();
    let loaded = source
        .stream_exterior("WhiterunOrigin", radius)
        .unwrap_or_else(|e| panic!("stream_exterior: {e}"));
    let streamed = started.elapsed();

    println!(
        "worldspace: {} ({})",
        loaded.block.world_editor_id.as_deref().unwrap_or("?"),
        loaded.block.world
    );
    println!(
        "origin: {:?} radius {}",
        loaded.block.origin, loaded.block.radius
    );
    println!(
        "land parent: {} ({} cells at the block's positions)",
        loaded
            .block
            .land_parent
            .map_or("none".to_string(), |id| id.to_string()),
        loaded.block.parent_cells.len()
    );
    println!("cells: {}", loaded.cells.len());
    println!("references: {}", loaded.references());
    println!("meshes parsed: {}", loaded.meshes_parsed());
    println!("meshes failed: {}", loaded.meshes_failed());
    println!("triangles: {}", loaded.triangles());
    println!("surfaces: {}", loaded.surfaces());
    println!("land cells: {}", loaded.land_cells);
    println!("stream wall time: {:.2}s", streamed.as_secs_f64());

    let batch_started = std::time::Instant::now();
    let mut nodes = 0;
    let mut largest = 0;
    for (cell, loaded_cell) in &loaded.cells {
        let (batches, stats) = batch_surfaces(&loaded_cell.surfaces);
        let has_land = loaded_cell.surfaces.last().is_some_and(|s| {
            s.arrays.material.diffuse.is_none() && s.arrays.triangle_count() == 2048
        });
        println!(
            "  cell ({}, {}) {}: {} refs, {} surfaces, {} batches, land {}",
            cell.x,
            cell.y,
            cell.id,
            loaded_cell.stats.references,
            stats.surfaces_in,
            stats.batches,
            has_land
        );
        assert_eq!(stats.triangles, loaded_cell.stats.triangles);
        nodes += batches.len();
        largest = largest.max(stats.largest_batch_surfaces);
    }
    println!("batched nodes: {nodes} (largest batch {largest} surfaces)");
    println!(
        "batch wall time: {:.3}s",
        batch_started.elapsed().as_secs_f64()
    );

    assert!(!loaded.cells.is_empty(), "no exterior cells resolved");
    assert!(loaded.references() > 0, "the block has no references");
    assert!(loaded.meshes_parsed() > 0, "not one static mesh parsed");
    assert!(
        nodes < loaded.surfaces(),
        "batching did not reduce the node count"
    );
}
