// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_godot_host::streaming::{CellCoord, CellStreamer, StreamBudget, StreamConfig, StreamCost};
use soe_godot_host::units::SkyrimPos;

fn streamer(radius: i32) -> CellStreamer {
    CellStreamer::new(StreamConfig {
        load_radius: radius,
        unload_radius: radius,
        max_loads_per_tick: 0,
        max_unloads_per_tick: 0,
    })
    .unwrap()
}

#[test]
fn hst_08_each_fast_traversal_tick_obeys_both_work_caps() {
    let budget = StreamBudget {
        triangles: 19,
        bytes: 43,
    };
    let cost = |cell: CellCoord| StreamCost {
        triangles: 4 + cell.x.rem_euclid(3) as usize * 3,
        bytes: 9 + cell.y.rem_euclid(4) as usize * 5,
    };
    let mut planner = streamer(1);

    for step in 0..12 {
        let plan = planner.update_with_budget(
            SkyrimPos::new(step as f32 * 4096.0, 0.0, 0.0),
            budget,
            cost,
        );
        let spent = plan.load.iter().copied().map(cost).fold(
            StreamCost::default(),
            |sum, next| StreamCost {
                triangles: sum.triangles + next.triangles,
                bytes: sum.bytes + next.bytes,
            },
        );
        assert!(spent.triangles <= budget.triangles, "tick {step}: {spent:?}");
        assert!(spent.bytes <= budget.bytes, "tick {step}: {spent:?}");
    }
}

#[test]
fn hst_08_ties_follow_openmw_manhattan_then_origin_distance_priority() {
    let mut planner = streamer(2);
    let center = CellCoord::new(10, 10);
    let plan = planner.update_with_budget(
        SkyrimPos::new(10.0 * 4096.0, 10.0 * 4096.0, 0.0),
        StreamBudget::default(),
        |_| StreamCost::default(),
    );

    let first_origin_priority_group = &plan.load[5..8];
    assert!(first_origin_priority_group.iter().all(|cell| {
        let manhattan_from_center = (cell.x - center.x).abs() + (cell.y - center.y).abs();
        let manhattan_from_origin = cell.x.abs() + cell.y.abs();
        manhattan_from_center == 2 && manhattan_from_origin == 18
    }));
}

#[test]
fn hst_08_both_exact_caps_can_be_spent_in_one_tick() {
    let budget = StreamBudget {
        triangles: 12,
        bytes: 24,
    };
    let mut planner = streamer(1);
    let plan = planner.update_with_budget(
        SkyrimPos::new(0.0, 0.0, 0.0),
        budget,
        |cell| {
            if cell == CellCoord::new(0, 0) {
                StreamCost {
                    triangles: 7,
                    bytes: 8,
                }
            } else {
                StreamCost {
                    triangles: 5,
                    bytes: 16,
                }
            }
        },
    );
    let spent = plan.load.iter().copied().map(|cell| {
        if cell == CellCoord::new(0, 0) {
            StreamCost {
                triangles: 7,
                bytes: 8,
            }
        } else {
            StreamCost {
                triangles: 5,
                bytes: 16,
            }
        }
    }).fold(StreamCost::default(), |sum, cost| StreamCost {
        triangles: sum.triangles + cost.triangles,
        bytes: sum.bytes + cost.bytes,
    });

    assert_eq!(
        spent,
        StreamCost {
            triangles: budget.triangles,
            bytes: budget.bytes,
        }
    );
}
