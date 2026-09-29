// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks against the local TES5Edit object LOD grid convention.

use soe_godot_host::streaming::CellCoord;
use soe_godot_host::terrain_lod::{LodBlockCoord, LodLevel};

#[test]
fn block_for_cell_uses_tes5edit_division_west_and_south_of_anchor() {
    // TES5Edit's TwbLodSettings.BlockForCell uses Pascal integer `div`,
    // which truncates toward zero for a cell just before the anchor.
    for level in [LodLevel::L4, LodLevel::L8, LodLevel::L16, LodLevel::L32] {
        let block = LodBlockCoord::containing_from(
            CellCoord::new(99, -1),
            level,
            CellCoord::new(100, 0),
        );
        assert_eq!(block.x, 100, "{level:?} x block must follow BlockForCell");
        assert_eq!(block.y, 0, "{level:?} y block must follow BlockForCell");
    }
}
