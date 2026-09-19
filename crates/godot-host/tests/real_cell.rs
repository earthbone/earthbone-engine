// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Streams one real cell through `DataSource` against an installed Skyrim SE
//! copy. Runs only when `SKYRIM_DATA` points at the game's `Data` folder:
//!
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-godot-host --release --test real_cell -- --nocapture
//! ```
//!
//! Prefers `WhiterunBanneredMare`. Falls back to the first interior cell
//! with more than 50 references, so the test still exercises real content on
//! an install where that editor id is missing (a different game version, a
//! mod that renamed the cell).

use std::path::PathBuf;

use soe_godot_host::data_source::DataSource;

#[test]
fn a_real_cell_streams_statics_from_the_installed_game() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the real-cell test");
        return;
    };

    let started = std::time::Instant::now();
    let mut source = DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}"));

    let wanted = "WhiterunBanneredMare";
    let cell_id = match source.resolve_cell(wanted) {
        Some(id) => id,
        None => {
            eprintln!("{wanted} not found, falling back to a populated interior cell");
            source
                .first_populated_interior_cell(50)
                .expect("no interior cell with more than 50 references was found")
        }
    };

    let loaded = source
        .stream_cell_id(cell_id)
        .unwrap_or_else(|e| panic!("stream_cell_id: {e}"));
    let elapsed = started.elapsed();

    println!("cell: {cell_id}");
    println!("references: {}", loaded.stats.references);
    println!("statics with a model: {}", loaded.stats.statics_with_model);
    println!(
        "furniture with a model: {}",
        loaded.stats.furniture_with_model
    );
    println!("meshes parsed: {}", loaded.stats.meshes_parsed);
    println!("meshes failed: {}", loaded.stats.meshes_failed);
    println!("triangles: {}", loaded.stats.triangles);
    println!("wall time: {:.2}s", elapsed.as_secs_f64());

    println!(
        "distinct texture sets: {}",
        loaded.stats.distinct_texture_sets
    );
    println!("textures decoded: {}", loaded.stats.textures_decoded);
    println!("textures missing: {}", loaded.stats.textures_missing.len());
    for path in loaded.stats.textures_missing.iter().take(10) {
        println!("  missing: {path}");
    }
    println!(
        "texture decode failures: {}",
        loaded.stats.texture_decode_failures
    );
    println!(
        "texture decode wall time: {:.2}s",
        loaded.stats.texture_decode_time.as_secs_f64()
    );

    assert!(loaded.stats.references > 0, "the cell has no references");
    assert!(
        loaded.stats.statics_with_model > 0,
        "the cell has no static reference with a model"
    );
    assert!(
        loaded.stats.meshes_parsed > 0,
        "not one static mesh parsed, DataSource is not reaching real content"
    );
    assert!(
        loaded.stats.triangles > 0,
        "meshes parsed but produced no triangles"
    );
    assert!(
        loaded.stats.distinct_texture_sets > 0,
        "no material named a texture, MaterialDesc is not reaching real content"
    );
    assert!(
        loaded.stats.textures_decoded > 0,
        "not one texture decoded, DataSource is not resolving textures through the Vfs"
    );

    for workers in [1, 2, 8] {
        source.set_threads(workers);
        let again = source.stream_cell_id(cell_id).expect("cell streams again");
        assert_eq!(again.stats.references, loaded.stats.references);
        assert_eq!(
            again.stats.furniture_with_model,
            loaded.stats.furniture_with_model
        );
        assert_eq!(again.stats.meshes_failed, loaded.stats.meshes_failed);
        assert_eq!(again.stats.triangles, loaded.stats.triangles);
        assert_eq!(again.surfaces.len(), loaded.surfaces.len());
        assert_eq!(again.stats.textures_decoded, 0, "warm cache needs no reads");
        for (a, b) in again.surfaces.iter().zip(&loaded.surfaces) {
            assert_eq!(
                a.reference, b.reference,
                "worker count {workers} changed ownership"
            );
            assert_eq!(
                a.arrays, b.arrays,
                "worker count {workers} changed mesh data"
            );
            assert_eq!(a.textures.diffuse, b.textures.diffuse);
            assert_eq!(a.textures.normal, b.textures.normal);
        }
    }
}
