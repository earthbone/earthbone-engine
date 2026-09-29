// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_godot_host::streaming::{CellCoord, CellStreamer, StreamBudget, StreamConfig, StreamCost};
use soe_godot_host::units::SkyrimPos;

fn cell_cost(cell: CellCoord) -> StreamCost {
    StreamCost {
        triangles: 20 + cell.x.rem_euclid(4) as usize * 10,
        bytes: 64 + cell.x.rem_euclid(5) as usize * 16,
    }
}

#[test]
fn hst_08_fast_traversal_stays_within_triangle_and_byte_budgets_per_tick() {
    let budget = StreamBudget {
        triangles: 120,
        bytes: 256,
    };
    let mut streamer = CellStreamer::new(StreamConfig {
        load_radius: 1,
        unload_radius: 2,
        max_loads_per_tick: 0,
        max_unloads_per_tick: 0,
    })
    .unwrap();

    for step in 0..40 {
        let plan = streamer.update_with_budget(
            SkyrimPos::new(step as f32 * 4096.0, 0.0, 0.0),
            budget,
            cell_cost,
        );
        let spent =
            plan.load
                .iter()
                .copied()
                .map(cell_cost)
                .fold(StreamCost::default(), |sum, cost| StreamCost {
                    triangles: sum.triangles + cost.triangles,
                    bytes: sum.bytes + cost.bytes,
                });
        assert!(
            spent.triangles <= budget.triangles,
            "tick {step}: {spent:?}"
        );
        assert!(spent.bytes <= budget.bytes, "tick {step}: {spent:?}");
    }
}
