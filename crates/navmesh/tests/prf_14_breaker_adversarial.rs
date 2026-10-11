// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Equal-height overlapping surfaces retain deterministic location and counts.

use soe_navmesh::{NavGraph, NodeId};
use soe_trace::{Operation, WorkFrame};

#[test]
fn prf_14_overlapping_equal_height_meshes_choose_lowest_node_and_count_candidates() {
    let mut graph = NavGraph::new();
    for _ in 0..2 {
        graph.add_mesh_with_adjacency(
            vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
            &[[0, 1, 2]],
        );
    }

    let frame = WorkFrame::begin();
    assert_eq!(graph.locate([2.0, 2.0, 0.0]), Some(NodeId::new(0, 0)));
    assert_eq!(graph.locate([2.0, 2.0, 0.0]), Some(NodeId::new(0, 0)));
    assert_eq!(graph.locate([20.0, 20.0, 0.0]), None);
    assert_eq!(frame.counts().get(Operation::NavmeshLocate), 3);
    assert_eq!(frame.counts().get(Operation::NavmeshTriangles), 4);
}
