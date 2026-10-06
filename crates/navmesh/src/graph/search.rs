// SPDX-License-Identifier: GPL-3.0-or-later

//! Retained A* state, following DetourNavMeshQuery.cpp's sliced query contract.
//! Heap order, centroid costs and path assembly use the existing NAVM graph.

use std::collections::{BinaryHeap, HashMap};
use std::task::Poll;

use super::{distance, NavGraph, NodeId, Open, Path, PathCosts};

/// A request whose heap and predecessor map survive between frame slices.
/// Resume only against the graph that created it. Graph mutations cancel it.
#[derive(Debug)]
pub struct PathSearch {
    from: NodeId,
    to: NodeId,
    goal: [f32; 3],
    endpoints: Option<([f32; 3], [f32; 3])>,
    costs: PathCosts,
    revision: u64,
    open: BinaryHeap<Open>,
    g: HashMap<NodeId, f32>,
    came: HashMap<NodeId, (NodeId, usize)>,
    finished: bool,
    last_work: usize,
    last_expanded: usize,
}

impl PathSearch {
    pub(super) fn new(
        graph: &NavGraph,
        from: NodeId,
        to: NodeId,
        endpoints: Option<([f32; 3], [f32; 3])>,
        costs: PathCosts,
    ) -> Self {
        let goal = graph.centroid(to);
        let mut open = BinaryHeap::new();
        open.push(Open {
            f: distance(graph.centroid(from), goal) * costs.minimum(),
            node: from,
            g: 0.0,
        });
        Self {
            from,
            to,
            goal,
            endpoints,
            costs,
            revision: graph.revision,
            open,
            g: HashMap::from([(from, 0.0)]),
            came: HashMap::new(),
            finished: false,
            last_work: 0,
            last_expanded: 0,
        }
    }

    /// Heap entries processed by the last slice, including stale entries.
    pub fn last_work(&self) -> usize {
        self.last_work
    }

    /// Non-stale nodes expanded by the last slice, including the goal.
    pub fn last_expanded(&self) -> usize {
        self.last_expanded
    }

    /// Process at most `node_budget` heap entries and retain unfinished work.
    /// Completion includes the same corridor smoothing as synchronous queries.
    ///
    /// # Errors
    /// `Ready(None)` means no route, graph mutation, or an already consumed result.
    /// Zero budget returns `Pending` for an unfinished search.
    ///
    /// # Panics
    /// This function does not panic when resumed against its original graph.
    pub fn advance(&mut self, graph: &NavGraph, node_budget: usize) -> Poll<Option<Path>> {
        let _span = soe_trace::Span::new("navmesh.find_path", "navmesh");
        self.last_work = 0;
        self.last_expanded = 0;
        if self.finished || self.revision != graph.revision {
            self.finished = true;
            return Poll::Ready(None);
        }
        while self.last_work < node_budget {
            let Some(Open {
                node, g: queued, ..
            }) = self.open.pop()
            else {
                self.finished = true;
                return Poll::Ready(None);
            };
            self.last_work += 1;
            if queued > self.g[&node] {
                continue;
            }
            self.last_expanded += 1;
            soe_trace::count(soe_trace::Operation::PathNodes, 1);
            if node == self.to {
                self.finished = true;
                return Poll::Ready(Some(graph.assemble(
                    self.from,
                    self.to,
                    &self.came,
                    self.g[&self.to],
                    self.endpoints,
                )));
            }
            let here = self.g[&node];
            let c = graph.centroid(node);
            for (edge, next) in graph.triangle(node).neighbors.iter().enumerate() {
                let Some(next) = *next else { continue };
                let Some(mesh) = graph.meshes.get(next.mesh as usize) else {
                    continue;
                };
                let Some(triangle) = mesh.triangles.get(next.triangle as usize) else {
                    continue;
                };
                if graph.node_blocked(next) {
                    continue;
                }
                let centroid = mesh.centroid(next.triangle);
                let tentative =
                    here + distance(c, centroid) * self.costs.traversal_cost(triangle.flags);
                if self.g.get(&next).is_none_or(|&old| tentative < old) {
                    self.g.insert(next, tentative);
                    self.came.insert(next, (node, edge));
                    self.open.push(Open {
                        f: tentative + distance(centroid, self.goal) * self.costs.minimum(),
                        node: next,
                        g: tentative,
                    });
                }
            }
        }
        Poll::Pending
    }
}
