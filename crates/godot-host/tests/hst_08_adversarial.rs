// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_godot_host::streaming::{CellCoord, CellStreamer, StreamBudget, StreamConfig, StreamCost};
use soe_godot_host::units::SkyrimPos;

fn config() -> StreamConfig {
    StreamConfig {
        load_radius: 1,
        unload_radius: 1,
        max_loads_per_tick: 0,
        max_unloads_per_tick: 0,
    }
}

#[test]
fn hst_08_exact_limit_fits_and_zero_axis_is_unlimited() {
    let mut streamer = CellStreamer::new(config()).unwrap();
    let plan = streamer.update_with_budget(
        SkyrimPos::new(0.0, 0.0, 0.0),
        StreamBudget {
            triangles: 10,
            bytes: 0,
        },
        |_| StreamCost {
            triangles: 10,
            bytes: usize::MAX,
        },
    );
    assert_eq!(plan.load, [CellCoord::new(0, 0)]);
}

#[test]
fn hst_08_oversized_nearest_cell_is_deferred_while_later_cell_fits() {
    let mut streamer = CellStreamer::new(config()).unwrap();
    let plan = streamer.update_with_budget(
        SkyrimPos::new(0.0, 0.0, 0.0),
        StreamBudget {
            triangles: 5,
            bytes: 5,
        },
        |cell| {
            if cell == CellCoord::new(0, 0) {
                StreamCost {
                    triangles: 6,
                    bytes: 1,
                }
            } else {
                StreamCost {
                    triangles: 5,
                    bytes: 5,
                }
            }
        },
    );
    assert_eq!(plan.load, [CellCoord::new(-1, 0)]);
    assert!(streamer.is_resident(CellCoord::new(-1, 0)));
    assert!(!streamer.is_resident(CellCoord::new(0, 0)));
}

#[test]
fn hst_08_deferred_cells_are_retried_on_later_ticks() {
    let mut streamer = CellStreamer::new(config()).unwrap();
    let focus = SkyrimPos::new(0.0, 0.0, 0.0);
    let none = streamer.update_with_budget(
        focus,
        StreamBudget {
            triangles: 1,
            bytes: 1,
        },
        |_| StreamCost {
            triangles: 2,
            bytes: 2,
        },
    );
    assert!(none.load.is_empty());
    let next = streamer.update_with_budget(
        focus,
        StreamBudget {
            triangles: 2,
            bytes: 2,
        },
        |_| StreamCost {
            triangles: 2,
            bytes: 2,
        },
    );
    assert_eq!(next.load, [CellCoord::new(0, 0)]);
}

#[test]
fn hst_08_unloads_are_not_suppressed_by_load_mesh_budget() {
    let mut streamer = CellStreamer::new(StreamConfig {
        load_radius: 0,
        unload_radius: 0,
        max_loads_per_tick: 0,
        max_unloads_per_tick: 0,
    })
    .unwrap();
    streamer.update(SkyrimPos::new(0.0, 0.0, 0.0));
    let plan = streamer.update_with_budget(
        SkyrimPos::new(4096.0 * 4.0, 0.0, 0.0),
        StreamBudget {
            triangles: 0,
            bytes: 1,
        },
        |_| StreamCost {
            triangles: 0,
            bytes: 0,
        },
    );
    assert_eq!(plan.unload, [CellCoord::new(0, 0)]);
}
