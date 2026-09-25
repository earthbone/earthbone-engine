// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Where a real BTR block's geometry lands once built. A LOD block replaces
//! full resolution terrain only if it is drawn over the cells it is named
//! for, so each built block's bounds, converted back to Skyrim cells, must
//! sit on the block's own square. Also reads Tamriel's `lodsettings` file,
//! the anchor the block grid is aligned from.
//!
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-godot-host --release --test terrain_lod_placement -- --nocapture
//! ```

use std::path::PathBuf;

use soe_esm::GlobalFormId;
use soe_godot_host::data_source::DataSource;
use soe_godot_host::streaming::CellCoord;
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel};
use soe_godot_host::units::{GodotPos, CELL_UNITS};

fn open_data() -> Option<DataSource> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    Some(DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}")))
}

/// Tamriel's anchor must put every level on the grid its shipped files
/// use: level 4 block (-68, -36) and level 32 block (0, 0) are real files.
#[test]
fn tamriel_lodsettings_anchor_matches_the_shipped_file_names() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping the terrain LOD placement test");
        return;
    };
    let settings = source
        .lod_settings("Tamriel")
        .unwrap_or_else(|e| panic!("lod_settings: {e}"))
        .unwrap_or_else(|| panic!("a vanilla install ships lodsettings/tamriel.lod"));
    eprintln!("tamriel lodsettings: {settings:?}");
    assert_eq!(settings.finest_level(), Some(LodLevel::L4));
    for (level, x, y) in [(LodLevel::L4, -68, -36), (LodLevel::L32, 0, 0)] {
        let block = LodBlockCoord::containing_from(CellCoord::new(x, y), level, settings.sw_cell);
        assert_eq!(
            (block.x, block.y),
            (x, y),
            "the anchor {:?} must keep the shipped {level:?} block on its own grid",
            settings.sw_cell
        );
    }
}

/// Each built block's bounds, back in Skyrim cells, must lie on the block's
/// own square, within half a cell for seam skirts and float rounding.
#[test]
fn a_built_block_sits_on_its_own_cells() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping the terrain LOD placement test");
        return;
    };
    let world = GlobalFormId {
        file: 0,
        object: 0x3c,
    };
    for (level, x, y) in [
        (LodLevel::L4, -68, -36),
        (LodLevel::L8, 8, 24),
        (LodLevel::L16, 0, 48),
        (LodLevel::L32, 0, 0),
    ] {
        let block = LodBlockCoord { level, x, y };
        let loaded = source
            .load_lod_block(world, "Tamriel", block)
            .unwrap_or_else(|e| panic!("load_lod_block {block:?}: {e}"))
            .unwrap_or_else(|| panic!("block {block:?} ships on a vanilla install"));
        let mut lo = [f32::MAX; 2];
        let mut hi = [f32::MIN; 2];
        for surface in &loaded.surfaces {
            for corner in [surface.arrays.bounds.min, surface.arrays.bounds.max] {
                let s = GodotPos::new(corner.x, corner.y, corner.z).to_skyrim();
                lo = [lo[0].min(s.x), lo[1].min(s.y)];
                hi = [hi[0].max(s.x), hi[1].max(s.y)];
            }
        }
        let cells = |v: f32| v / CELL_UNITS;
        let span = level.cells() as f32;
        eprintln!(
            "{level:?} ({x}, {y}): {} surfaces, {} bytes, cells x {:.2}..{:.2} y {:.2}..{:.2}",
            loaded.surfaces.len(),
            loaded.approx_bytes,
            cells(lo[0]),
            cells(hi[0]),
            cells(lo[1]),
            cells(hi[1]),
        );
        let (x, y) = (x as f32, y as f32);
        assert!(
            cells(lo[0]) >= x - 0.5 && cells(hi[0]) <= x + span + 0.5,
            "{block:?} spans cells x {:.2}..{:.2}, not its own {x}..{}",
            cells(lo[0]),
            cells(hi[0]),
            x + span,
        );
        assert!(
            cells(lo[1]) >= y - 0.5 && cells(hi[1]) <= y + span + 0.5,
            "{block:?} spans cells y {:.2}..{:.2}, not its own {y}..{}",
            cells(lo[1]),
            cells(hi[1]),
            y + span,
        );
    }
}
