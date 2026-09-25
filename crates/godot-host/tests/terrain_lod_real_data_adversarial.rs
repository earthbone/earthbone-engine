// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial tests for `DataSource::load_lod_block` against real Skyrim
//! Special Edition BTR files. RND-03's own writer notes it "never ran a
//! live SKYRIM_DATA pass" against this code, so nothing has yet exercised
//! `load_lod_block` on an actual vanilla file. These attack the contracts
//! `crates/godot-host/src/data_source/lod.rs`'s doc comment states outright:
//! a missing block is `Ok(None)` and not an error, and repeated reads of the
//! same block are stable, plus the RND-03 done check's residency accounting.
//!
//! `WhiterunOrigin`, the marker every other adversarial and real-data test
//! in this crate resolves against, names the origin cell of `WhiterunWorld`,
//! Whiterun's own small walled-city worldspace, not Tamriel. A city
//! worldspace carries no distant-terrain LOD, so a Tamriel path is needed
//! here. `resolve_exterior_block` only resolves a worldspace by looking up
//! a named `CELL`, and Tamriel's open-world grid cells carry no such
//! editor ids, so these tests build `LodBlockCoord` values directly against
//! `"tamriel"` and block coordinates confirmed present in this machine's
//! `Skyrim - Meshes0.bsa` (`meshes/terrain/tamriel/tamriel.<level>.<x>.<y>.btr`).
//! `load_lod_block` never checks its `world` argument against `world_editor_id`,
//! it only uses `world_editor_id` to build the read path, so a `GlobalFormId`
//! that is not actually Tamriel's own form id changes nothing being tested
//! here and is only carried through to tag the returned surfaces.
//!
//! Run with SKYRIM_DATA set:
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-godot-host --release --test terrain_lod_real_data_adversarial -- --nocapture
//! ```

use std::path::PathBuf;

use soe_esm::GlobalFormId;
use soe_godot_host::data_source::DataSource;
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel};

fn open_data() -> Option<DataSource> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    Some(DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}")))
}

/// Not Tamriel's real form id, `load_lod_block` never checks it against
/// `world_editor_id`, it only stamps returned surfaces with it. See the
/// module doc above.
fn placeholder_world() -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: 0x3c,
    }
}

/// Block coordinates this machine's vanilla `Skyrim - Meshes0.bsa` is
/// confirmed to carry a `.btr` file for, one per LOD level, found by
/// listing `meshes/terrain/tamriel/` through `soe_bsa::Vfs` directly.
fn known_present_block(level: LodLevel) -> LodBlockCoord {
    let (x, y) = match level {
        LodLevel::L4 => (-68, -36),
        LodLevel::L8 => (8, 24),
        LodLevel::L16 => (0, 48),
        LodLevel::L32 => (0, 0),
    };
    LodBlockCoord { level, x, y }
}

/// The module doc for `load_lod_block` states plainly: a worldspace's LOD
/// coverage "rarely fills every block at every level", so a missing BTR
/// file "is content that is not there, not a read error", the same
/// convention `Vfs::read` already uses for a missing texture. A block whose
/// grid coordinates sit far outside Tamriel's actual bounds is guaranteed
/// to have no BTR file at any level, so this must come back `Ok(None)`,
/// never an `Err`.
#[test]
fn a_block_with_no_btr_file_is_ok_none_not_an_error() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping the real terrain LOD adversarial test");
        return;
    };
    let world = placeholder_world();
    for level in [LodLevel::L4, LodLevel::L8, LodLevel::L16, LodLevel::L32] {
        let block = LodBlockCoord {
            level,
            x: 10_000,
            y: 10_000,
        };
        let result = source.load_lod_block(world, "tamriel", block);
        let Ok(content) = result else {
            panic!(
                "a block far outside Tamriel's LOD coverage must not \
                 error, load_lod_block's own doc comment says a missing \
                 BTR file is content that is not there, at level {level:?}"
            );
        };
        assert!(
            content.is_none(),
            "a block far outside Tamriel's LOD coverage must resolve to \
             no content, at level {level:?}"
        );
    }
}

/// `load_lod_block` takes `&self` and touches no mutable state, so two
/// calls for the same block, an open and close cycle repeated, must agree:
/// same surface count, same triangle totals, same recorded `approx_bytes`.
/// A caller that streams LOD blocks in and out as the camera moves relies
/// on this to keep the residency budget's bookkeeping consistent across
/// repeated loads of a block that re-enters range.
#[test]
fn repeated_loads_of_the_same_block_agree() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping the real terrain LOD adversarial test");
        return;
    };
    let world = placeholder_world();

    for level in [LodLevel::L4, LodLevel::L8, LodLevel::L16, LodLevel::L32] {
        let block = known_present_block(level);
        let first = source
            .load_lod_block(world, "tamriel", block)
            .unwrap_or_else(|e| panic!("load_lod_block first call at {level:?}: {e}"))
            .unwrap_or_else(|| {
                panic!(
                    "block {block:?} is confirmed present in this machine's \
                     Skyrim - Meshes0.bsa (meshes/terrain/tamriel/), a first \
                     load must find it"
                )
            });
        let second = source
            .load_lod_block(world, "tamriel", block)
            .unwrap_or_else(|e| panic!("load_lod_block second call at {level:?}: {e}"))
            .unwrap_or_else(|| {
                panic!("block {block:?} vanished between two identical reads at level {level:?}")
            });

        assert_eq!(
            first.surfaces.len(),
            second.surfaces.len(),
            "surface count changed between identical loads at level {level:?}"
        );
        assert_eq!(
            first.approx_bytes, second.approx_bytes,
            "approx_bytes changed between identical loads at level {level:?}"
        );
        let first_tris: usize = first
            .surfaces
            .iter()
            .map(|s| s.arrays.indices.len() / 3)
            .sum();
        let second_tris: usize = second
            .surfaces
            .iter()
            .map(|s| s.arrays.indices.len() / 3)
            .sum();
        assert_eq!(
            first_tris, second_tris,
            "triangle total changed between identical loads at level {level:?}"
        );
    }
}

/// A block whose read succeeds with at least one built surface must be
/// recorded with a nonzero residency cost. `TerrainLodStreamer::record_bytes`
/// is meant to receive `LoadedLodBlock::approx_bytes` for exactly this
/// purpose (see `crate::terrain_lod`'s module doc and `data_source/lod.rs`'s
/// doc comment on `approx_bytes`); a resident block charged 0 bytes would
/// let a caller load an unbounded number of them without ever tripping
/// `TerrainLodStreamer::over_budget`, defeating the "recorded residency
/// budget" the RND-03 done check requires.
#[test]
fn a_loaded_block_with_surfaces_is_never_recorded_as_free() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping the real terrain LOD adversarial test");
        return;
    };
    let world = placeholder_world();

    for level in [LodLevel::L4, LodLevel::L8, LodLevel::L16, LodLevel::L32] {
        let block = known_present_block(level);
        let loaded = source
            .load_lod_block(world, "tamriel", block)
            .unwrap_or_else(|e| panic!("load_lod_block at {level:?}: {e}"))
            .unwrap_or_else(|| {
                panic!(
                    "block {block:?} is confirmed present in this machine's \
                     Skyrim - Meshes0.bsa (meshes/terrain/tamriel/), a load \
                     must find it"
                )
            });
        assert!(
            !loaded.surfaces.is_empty(),
            "block {block:?}, confirmed present on disk, built zero \
             surfaces at level {level:?}"
        );
        assert!(
            loaded.approx_bytes > 0,
            "level {level:?} block {block:?} built {} surfaces but was \
             recorded at 0 bytes, a caller feeding this into \
             TerrainLodStreamer::record_bytes would treat a real resident \
             block as free",
            loaded.surfaces.len(),
        );
    }
}
