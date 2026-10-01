// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_godot_host::streaming::{CellCoord, CellStreamer, StreamBudget, StreamConfig, StreamCost};
use soe_godot_host::units::SkyrimPos;

#[test]
fn hst_08_each_budget_axis_is_independently_unlimited_when_zero() {
    let mut streamer = CellStreamer::new(StreamConfig {
        load_radius: 1,
        unload_radius: 1,
        max_loads_per_tick: 0,
        max_unloads_per_tick: 0,
    }).unwrap();
    let plan = streamer.update_with_budget(
        SkyrimPos::new(0.0, 0.0, 0.0),
        StreamBudget { triangles: 0, bytes: 11 },
        |_| StreamCost { triangles: usize::MAX, bytes: 11 },
    );

    assert_eq!(plan.load, [CellCoord::new(0, 0)]);
}

#[test]
fn hst_08_saturating_costs_never_wrap_past_a_finite_budget() {
    let mut streamer = CellStreamer::new(StreamConfig {
        load_radius: 1,
        unload_radius: 1,
        max_loads_per_tick: 0,
        max_unloads_per_tick: 0,
    }).unwrap();
    let plan = streamer.update_with_budget(
        SkyrimPos::new(0.0, 0.0, 0.0),
        StreamBudget { triangles: usize::MAX, bytes: usize::MAX },
        |_| StreamCost { triangles: usize::MAX, bytes: usize::MAX },
    );

    assert_eq!(plan.load.len(), 1);
    assert_eq!(plan.load[0], CellCoord::new(0, 0));
}
