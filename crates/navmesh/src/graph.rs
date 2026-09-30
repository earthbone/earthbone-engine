// SPDX-License-Identifier: GPL-3.0-or-later

//! A walkable graph over triangles, and A* on it.
//!
//! Nodes are triangles. A triangle has up to three neighbours, one across
//! each edge, and a neighbour may live in another mesh: that is how the
//! vanilla navmeshes of adjacent cells join, and the graph does not care
//! which mesh a node belongs to once the links are in.
//!
//! Paths retain the A* triangle corridor. Detour-style string pulling selects
//! XY corners, then intersects every crossed portal to retain terrain height
//! and waypoint ownership. Non-geometric graph links remain explicit segments.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::OnceLock;

mod funnel;
#[cfg(test)]
mod funnel_tests;
#[cfg(test)]
mod query_tests;
mod spatial;
use spatial::SpatialIndex;

const WATER_FLAG: u16 = 0x0200;
const PREFERRED_FLAG: u16 = 0x0040;

/// Traversal costs for the NAVM triangle flags this graph understands.
///
/// The default values are an engine policy, not measured Skyrim setting values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathCosts {
    /// Multiplier for a triangle carrying the water flag.
    pub water: f32,
    /// Multiplier for a triangle carrying the preferred flag.
    pub preferred: f32,
}

impl Default for PathCosts {
    fn default() -> Self {
        Self {
            water: 10.0,
            preferred: 0.5,
        }
    }
}

/// A triangle in the graph: which mesh, and which triangle in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId {
    pub mesh: u32,
    pub triangle: u32,
}

impl NodeId {
    pub const fn new(mesh: u32, triangle: u32) -> Self {
        NodeId { mesh, triangle }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Triangle {
    pub vertices: [u32; 3],
    /// Across edge 0-1, 1-2, 2-0.
    pub neighbors: [Option<NodeId>; 3],
    /// A triangle the game would not path over. It stays in the graph so
    /// links through it still resolve, A* skips it.
    pub blocked: bool,
    /// Vanilla NAVM triangle flags retained for path cost selection.
    pub flags: u16,
}

#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<Triangle>,
    min: [f32; 3],
    max: [f32; 3],
}

impl Mesh {
    fn recompute_bounds(&mut self) {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for v in &self.vertices {
            for i in 0..3 {
                min[i] = min[i].min(v[i]);
                max[i] = max[i].max(v[i]);
            }
        }
        self.min = min;
        self.max = max;
    }

    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        (self.min, self.max)
    }

    pub fn centroid(&self, triangle: u32) -> [f32; 3] {
        let t = &self.triangles[triangle as usize];
        let mut c = [0f32; 3];
        for v in t.vertices {
            let p = self.vertices[v as usize];
            for i in 0..3 {
                c[i] += p[i] / 3.0;
            }
        }
        c
    }

    /// Midpoint of edge `edge` (0-1, 1-2, 2-0) of a triangle.
    pub fn edge_midpoint(&self, triangle: u32, edge: usize) -> [f32; 3] {
        let t = &self.triangles[triangle as usize];
        let a = self.vertices[t.vertices[edge] as usize];
        let b = self.vertices[t.vertices[(edge + 1) % 3] as usize];
        [
            a[0].midpoint(b[0]),
            a[1].midpoint(b[1]),
            a[2].midpoint(b[2]),
        ]
    }

    /// Height of the triangle's plane at an XY position, or `None` when the
    /// XY position is outside the triangle.
    fn height_at(&self, triangle: u32, x: f32, y: f32) -> Option<f32> {
        let t = &self.triangles[triangle as usize];
        let [a, b, c] = t.vertices.map(|v| self.vertices[v as usize]);
        let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
        if d.abs() < 1e-9 {
            return None;
        }
        let l0 = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / d;
        let l1 = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / d;
        let l2 = 1.0 - l0 - l1;
        let eps = -1e-4;
        if l0 < eps || l1 < eps || l2 < eps {
            return None;
        }
        Some(l0 * a[2] + l1 * b[2] + l2 * c[2])
    }
}

/// The route between two triangles.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub nodes: Vec<NodeId>,
    /// Start, portal crossings along the pulled route, and end.
    pub points: Vec<[f32; 3]>,
    /// Triangle entered at each point, parallel to `points`.
    pub point_nodes: Vec<NodeId>,
    /// Sum of centroid distances weighted by each entered triangle's flags.
    pub cost: f32,
}

#[derive(Debug, Clone, Default)]
pub struct NavGraph {
    meshes: Vec<Mesh>,
    /// NAVM door references mapped to the triangles authored for them.
    door_nodes: HashMap<u64, Vec<NodeId>>,
    spatial: OnceLock<SpatialIndex>,
    components: OnceLock<Vec<usize>>,
}

#[derive(Clone, Copy, PartialEq)]
struct Open {
    f: f32,
    g: f32,
    node: NodeId,
}

impl Eq for Open {}

impl Ord for Open {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed so the heap pops the smallest f. NaN never enters.
        other
            .f
            .partial_cmp(&self.f)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.node.cmp(&self.node))
            .then_with(|| other.g.total_cmp(&self.g))
    }
}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

impl PathCosts {
    fn traversal_cost(self, flags: u16) -> f32 {
        let mut multiplier = 1.0;
        if flags & WATER_FLAG != 0 {
            multiplier *= self.water;
        }
        if flags & PREFERRED_FLAG != 0 {
            multiplier *= self.preferred;
        }
        multiplier
    }

    fn minimum(self) -> f32 {
        1.0_f32
            .min(self.water)
            .min(self.preferred)
            .min(self.water * self.preferred)
    }

    fn is_valid(self) -> bool {
        let combined = self.water * self.preferred;
        self.water.is_finite()
            && self.water > 0.0
            && self.preferred.is_finite()
            && self.preferred > 0.0
            && combined.is_finite()
            && combined > 0.0
    }
}

impl NavGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a mesh whose triangles carry only their in-mesh neighbours (or
    /// none, to be linked later). Returns the mesh index for [`NodeId`]s.
    pub fn add_mesh(&mut self, vertices: Vec<[f32; 3]>, triangles: Vec<Triangle>) -> u32 {
        let mut mesh = Mesh {
            vertices,
            triangles,
            min: [0.0; 3],
            max: [0.0; 3],
        };
        mesh.recompute_bounds();
        self.spatial.take();
        self.components.take();
        self.meshes.push(mesh);
        (self.meshes.len() - 1) as u32
    }

    /// Add a mesh and derive its in-mesh adjacency from shared vertex pairs.
    pub fn add_mesh_with_adjacency(&mut self, vertices: Vec<[f32; 3]>, tris: &[[u32; 3]]) -> u32 {
        let mut by_edge: HashMap<(u32, u32), Vec<(u32, usize)>> = HashMap::new();
        for (ti, t) in tris.iter().enumerate() {
            for e in 0..3 {
                let a = t[e];
                let b = t[(e + 1) % 3];
                by_edge
                    .entry((a.min(b), a.max(b)))
                    .or_default()
                    .push((ti as u32, e));
            }
        }
        let mesh_index = self.meshes.len() as u32;
        let mut triangles: Vec<Triangle> = tris
            .iter()
            .map(|t| Triangle {
                vertices: *t,
                neighbors: [None; 3],
                blocked: false,
                flags: 0,
            })
            .collect();
        for users in by_edge.values() {
            if users.len() == 2 {
                let (t0, e0) = users[0];
                let (t1, e1) = users[1];
                triangles[t0 as usize].neighbors[e0] = Some(NodeId::new(mesh_index, t1));
                triangles[t1 as usize].neighbors[e1] = Some(NodeId::new(mesh_index, t0));
            }
        }
        self.add_mesh(vertices, triangles)
    }

    /// Make `b` the neighbour of `a` across `edge`. One direction only, the
    /// other mesh carries its own link back.
    pub fn link(&mut self, a: NodeId, edge: usize, b: NodeId) {
        self.components.take();
        self.meshes[a.mesh as usize].triangles[a.triangle as usize].neighbors[edge] = Some(b);
    }

    pub fn set_blocked(&mut self, node: NodeId, blocked: bool) {
        self.meshes[node.mesh as usize].triangles[node.triangle as usize].blocked = blocked;
    }

    /// Associate a resolved door reference with its authored NAVM triangle.
    /// This association does not make the triangle an edge or change its
    /// walkability.
    pub(crate) fn add_door_node(&mut self, door: u64, node: NodeId) {
        let nodes = self.door_nodes.entry(door).or_default();
        if !nodes.contains(&node) {
            nodes.push(node);
            nodes.sort_unstable();
        }
    }

    /// Triangles that NAVM associates with a resolved door key.
    ///
    /// The key is the value returned by `vanilla::Builder`'s resolver for the
    /// raw door FormID. The association does not control pathfinding.
    pub fn door_nodes(&self, door: u64) -> &[NodeId] {
        self.door_nodes.get(&door).map_or(&[], Vec::as_slice)
    }

    /// Set the vanilla NAVM triangle flags used by path cost selection.
    /// Returns `false` when `node` is outside the graph.
    pub fn set_flags(&mut self, node: NodeId, flags: u16) -> bool {
        let Some(triangle) = self
            .meshes
            .get_mut(node.mesh as usize)
            .and_then(|mesh| mesh.triangles.get_mut(node.triangle as usize))
        else {
            return false;
        };
        triangle.flags = flags;
        true
    }

    pub fn meshes(&self) -> &[Mesh] {
        &self.meshes
    }

    pub fn mesh(&self, index: u32) -> &Mesh {
        &self.meshes[index as usize]
    }

    pub fn triangle(&self, node: NodeId) -> &Triangle {
        &self.meshes[node.mesh as usize].triangles[node.triangle as usize]
    }

    pub fn triangle_count(&self) -> usize {
        self.meshes.iter().map(|m| m.triangles.len()).sum()
    }

    pub fn centroid(&self, node: NodeId) -> [f32; 3] {
        self.meshes[node.mesh as usize].centroid(node.triangle)
    }

    /// The triangle under a point: its XY inside the triangle, and of those
    /// the one whose surface is nearest in Z. Meshes whose bounds exclude
    /// the point are skipped, so this is cheap on a world-sized graph.
    pub fn locate(&self, point: [f32; 3]) -> Option<NodeId> {
        if !point.iter().all(|p| p.is_finite()) {
            return None;
        }
        let mut best: Option<(f32, NodeId)> = None;
        self.spatial
            .get_or_init(|| SpatialIndex::build(&self.meshes))
            .visit_xy(point, |mi| {
                let mesh = &self.meshes[mi];
                if point[0] < mesh.min[0]
                    || point[0] > mesh.max[0]
                    || point[1] < mesh.min[1]
                    || point[1] > mesh.max[1]
                {
                    return;
                }
                for ti in 0..mesh.triangles.len() {
                    if let Some(z) = mesh.height_at(ti as u32, point[0], point[1]) {
                        let candidate = ((z - point[2]).abs(), NodeId::new(mi as u32, ti as u32));
                        if best.is_none_or(|old| candidate < old) {
                            best = Some(candidate);
                        }
                    }
                }
            });
        best.map(|(_, n)| n)
    }

    /// The triangle whose centroid is nearest, for points just off the mesh.
    pub fn nearest(&self, point: [f32; 3]) -> Option<NodeId> {
        if !point.iter().all(|p| p.is_finite()) {
            return None;
        }
        let mut best: Option<(f32, NodeId)> = None;
        self.spatial
            .get_or_init(|| SpatialIndex::build(&self.meshes))
            .visit_nearest(point, |mi, radius| {
                let mesh = &self.meshes[mi];
                for ti in 0..mesh.triangles.len() {
                    let candidate = (
                        distance(mesh.centroid(ti as u32), point),
                        NodeId::new(mi as u32, ti as u32),
                    );
                    if best.is_none_or(|old| candidate < old) {
                        *radius = candidate.0;
                        best = Some(candidate);
                    }
                }
            });
        best.map(|(_, n)| n)
    }

    /// Weak mesh components only reject impossible routes. A component can
    /// still contain directed links or disconnected triangles, so A* remains
    /// the authority for reachability within it.
    fn components(&self) -> &[usize] {
        self.components.get_or_init(|| {
            fn root(parents: &mut [usize], mut node: usize) -> usize {
                while parents[node] != node {
                    parents[node] = parents[parents[node]];
                    node = parents[node];
                }
                node
            }
            let mut parents: Vec<_> = (0..self.meshes.len()).collect();
            let mut ranks = vec![0u8; parents.len()];
            for (mi, mesh) in self.meshes.iter().enumerate() {
                for tri in &mesh.triangles {
                    for next in tri.neighbors.iter().flatten() {
                        let other = next.mesh as usize;
                        if other == mi
                            || self
                                .meshes
                                .get(other)
                                .and_then(|m| m.triangles.get(next.triangle as usize))
                                .is_none()
                        {
                            continue;
                        }
                        let mut a = root(&mut parents, mi);
                        let mut b = root(&mut parents, other);
                        if a == b {
                            continue;
                        }
                        if ranks[a] < ranks[b] {
                            std::mem::swap(&mut a, &mut b);
                        }
                        parents[b] = a;
                        if ranks[a] == ranks[b] {
                            ranks[a] += 1;
                        }
                    }
                }
            }
            for mi in 0..parents.len() {
                parents[mi] = root(&mut parents, mi);
            }
            parents
        })
    }

    /// A* over triangle adjacency with weighted centroid edge costs and a
    /// minimum-multiplier-scaled straight-line heuristic.
    pub fn find_path(&self, from: NodeId, to: NodeId) -> Option<Path> {
        self.find_path_with_costs(from, to, PathCosts::default())
    }

    /// Find a path with caller-selected triangle flag costs.
    /// Returns `None` when either cost or their product is non-finite or not
    /// strictly positive.
    pub fn find_path_with_costs(&self, from: NodeId, to: NodeId, costs: PathCosts) -> Option<Path> {
        self.find_path_impl(from, to, None, costs)
    }

    fn find_path_impl(
        &self,
        from: NodeId,
        to: NodeId,
        endpoints: Option<([f32; 3], [f32; 3])>,
        costs: PathCosts,
    ) -> Option<Path> {
        let _span = soe_trace::Span::new("navmesh.find_path", "navmesh");
        if !costs.is_valid() {
            return None;
        }
        let start = self
            .meshes
            .get(from.mesh as usize)?
            .triangles
            .get(from.triangle as usize)?;
        let end = self
            .meshes
            .get(to.mesh as usize)?
            .triangles
            .get(to.triangle as usize)?;
        if start.blocked || end.blocked {
            return None;
        }
        if from.mesh != to.mesh
            && self.components()[from.mesh as usize] != self.components()[to.mesh as usize]
        {
            return None;
        }
        let goal = self.centroid(to);
        let mut open = BinaryHeap::new();
        let mut g: HashMap<NodeId, f32> = HashMap::new();
        let mut came: HashMap<NodeId, (NodeId, usize)> = HashMap::new();
        g.insert(from, 0.0);
        open.push(Open {
            f: distance(self.centroid(from), goal) * costs.minimum(),
            node: from,
            g: 0.0,
        });
        while let Some(Open {
            node, g: queued, ..
        }) = open.pop()
        {
            if queued > g[&node] {
                continue;
            }
            if node == to {
                return Some(self.assemble(from, to, &came, g[&to], endpoints));
            }
            let here = g[&node];
            let c = self.centroid(node);
            let tri = self.triangle(node);
            for (edge, next) in tri.neighbors.iter().enumerate() {
                let Some(next) = *next else { continue };
                let Some(m) = self.meshes.get(next.mesh as usize) else {
                    continue;
                };
                let Some(nt) = m.triangles.get(next.triangle as usize) else {
                    continue;
                };
                if nt.blocked {
                    continue;
                }
                let tentative =
                    here + distance(c, m.centroid(next.triangle)) * costs.traversal_cost(nt.flags);
                if g.get(&next).is_none_or(|&old| tentative < old) {
                    g.insert(next, tentative);
                    came.insert(next, (node, edge));
                    open.push(Open {
                        f: tentative + distance(m.centroid(next.triangle), goal) * costs.minimum(),
                        node: next,
                        g: tentative,
                    });
                }
            }
        }
        None
    }

    /// Path between two world points, each located on the graph first.
    /// Exact endpoints are retained. Off-mesh fallback approach/departure
    /// segments are not guaranteed to lie on the walkable surface.
    pub fn find_path_between(&self, from: [f32; 3], to: [f32; 3]) -> Option<Path> {
        let a = self.locate(from).or_else(|| self.nearest(from))?;
        let b = self.locate(to).or_else(|| self.nearest(to))?;
        self.find_path_impl(a, b, Some((from, to)), PathCosts::default())
    }

    fn assemble(
        &self,
        from: NodeId,
        to: NodeId,
        came: &HashMap<NodeId, (NodeId, usize)>,
        cost: f32,
        endpoints: Option<([f32; 3], [f32; 3])>,
    ) -> Path {
        let mut nodes = vec![to];
        let mut cur = to;
        while cur != from {
            let (prev, _) = came[&cur];
            nodes.push(prev);
            cur = prev;
        }
        nodes.reverse();
        let (start, end) = endpoints.unwrap_or_else(|| (self.centroid(from), self.centroid(to)));
        let (points, point_nodes) = self.pull_path(&nodes, start, end);
        Path {
            nodes,
            points,
            point_nodes,
            cost,
        }
    }

    /// Every triangle reachable from `start`, for connectivity checks.
    pub fn reachable(&self, start: NodeId) -> Vec<NodeId> {
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![start];
        let mut out = Vec::new();
        while let Some(n) = stack.pop() {
            if !seen.insert(n) {
                continue;
            }
            out.push(n);
            for next in self.triangle(n).neighbors.iter().flatten() {
                if self
                    .meshes
                    .get(next.mesh as usize)
                    .is_some_and(|m| (next.triangle as usize) < m.triangles.len())
                {
                    stack.push(*next);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1 by 4 strip of squares, two triangles each, 100 units per square.
    fn strip() -> NavGraph {
        let mut vertices = Vec::new();
        for i in 0..=4 {
            vertices.push([i as f32 * 100.0, 0.0, 0.0]);
            vertices.push([i as f32 * 100.0, 100.0, 0.0]);
        }
        let mut tris = Vec::new();
        for i in 0..4u32 {
            let bl = i * 2;
            let br = bl + 2;
            tris.push([bl, br, br + 1]);
            tris.push([bl, br + 1, bl + 1]);
        }
        let mut g = NavGraph::new();
        g.add_mesh_with_adjacency(vertices, &tris);
        g
    }

    fn isolated_triangle(graph: &mut NavGraph, x: f32, y: f32) -> NodeId {
        let mesh = graph.add_mesh_with_adjacency(
            vec![[x, y, 0.0], [x + 1.0, y, 0.0], [x, y + 1.0, 0.0]],
            &[[0, 1, 2]],
        );
        NodeId::new(mesh, 0)
    }

    #[test]
    fn adjacency_comes_from_shared_edges() {
        let g = strip();
        let t0 = g.triangle(NodeId::new(0, 0));
        assert_eq!(
            t0.neighbors[1],
            Some(NodeId::new(0, 3)),
            "across the right edge"
        );
        assert_eq!(
            t0.neighbors[2],
            Some(NodeId::new(0, 1)),
            "across the diagonal"
        );
        assert_eq!(t0.neighbors[0], None, "the bottom edge is a border");
    }

    #[test]
    fn a_star_walks_the_strip_and_stays_on_it() {
        let g = strip();
        let p = g.find_path(NodeId::new(0, 0), NodeId::new(0, 7)).unwrap();
        // Two triangles per square, the diagonal alternates, six hops.
        assert_eq!(p.nodes.len(), 6, "{:?}", p.nodes);
        assert_eq!(p.points.len(), 7);
        for pt in &p.points {
            assert!(pt[0] >= 0.0 && pt[0] <= 400.0 && pt[1] >= 0.0 && pt[1] <= 100.0);
        }
        assert!(p.cost > 300.0 && p.cost < 500.0, "{}", p.cost);
    }

    #[test]
    fn a_blocked_triangle_cuts_the_strip() {
        let mut g = strip();
        g.set_blocked(NodeId::new(0, 4), true);
        g.set_blocked(NodeId::new(0, 5), true);
        assert!(g.find_path(NodeId::new(0, 0), NodeId::new(0, 7)).is_none());
        assert!(g.find_path(NodeId::new(0, 0), NodeId::new(0, 3)).is_some());
    }

    #[test]
    fn paths_avoid_water_when_a_dry_detour_costs_less() {
        let mut graph = NavGraph::new();
        let start = isolated_triangle(&mut graph, 0.0, 0.0);
        let water = isolated_triangle(&mut graph, 1.0, 0.0);
        let dry = isolated_triangle(&mut graph, 0.0, 2.0);
        let goal = isolated_triangle(&mut graph, 2.0, 0.0);
        graph.link(start, 0, water);
        graph.link(water, 0, goal);
        graph.link(start, 1, dry);
        graph.link(dry, 0, goal);
        assert!(graph.set_flags(water, WATER_FLAG));

        let path = graph.find_path(start, goal).unwrap();
        assert_eq!(path.nodes, vec![start, dry, goal]);
        assert!(path.cost < 5.0, "dry path cost is {}", path.cost);
    }

    #[test]
    fn paths_choose_preferred_triangles_over_a_shorter_default_route() {
        let mut graph = NavGraph::new();
        let start = isolated_triangle(&mut graph, 0.0, 0.0);
        let direct = isolated_triangle(&mut graph, 1.0, 0.0);
        let preferred = isolated_triangle(&mut graph, 3.5, 0.0);
        let goal = isolated_triangle(&mut graph, 3.0, 0.0);
        graph.link(start, 0, direct);
        graph.link(direct, 0, goal);
        graph.link(start, 1, preferred);
        graph.link(preferred, 0, goal);
        assert!(graph.set_flags(preferred, PREFERRED_FLAG));

        let path = graph.find_path(start, goal).unwrap();
        assert_eq!(path.nodes, vec![start, preferred, goal]);
        assert!(path.cost < 2.5, "preferred path cost is {}", path.cost);
    }

    #[test]
    fn locate_picks_the_triangle_under_a_point() {
        let g = strip();
        let n = g.locate([250.0, 20.0, 5.0]).unwrap();
        assert_eq!(n.mesh, 0);
        assert!(n.triangle == 4 || n.triangle == 5);
        assert!(g.locate([900.0, 20.0, 0.0]).is_none());
        assert_eq!(g.nearest([900.0, 20.0, 0.0]).unwrap().triangle, 6);
    }

    #[test]
    fn links_join_two_meshes() {
        let mut g = NavGraph::new();
        let a = g.add_mesh_with_adjacency(
            vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 0.0]],
            &[[0, 1, 2]],
        );
        let b = g.add_mesh_with_adjacency(
            vec![[100.0, 0.0, 0.0], [100.0, 100.0, 0.0], [0.0, 100.0, 0.0]],
            &[[0, 1, 2]],
        );
        assert!(g.find_path(NodeId::new(a, 0), NodeId::new(b, 0)).is_none());
        g.link(NodeId::new(a, 0), 1, NodeId::new(b, 0));
        g.link(NodeId::new(b, 0), 2, NodeId::new(a, 0));
        let p = g.find_path(NodeId::new(a, 0), NodeId::new(b, 0)).unwrap();
        assert_eq!(p.nodes, vec![NodeId::new(a, 0), NodeId::new(b, 0)]);
        assert_eq!(
            p.points[1],
            [50.0, 50.0, 0.0],
            "the midpoint of the shared diagonal"
        );
        assert_eq!(g.reachable(NodeId::new(a, 0)).len(), 2);
    }

    #[test]
    fn a_dangling_link_is_ignored_not_followed() {
        let mut g = NavGraph::new();
        let a = g.add_mesh_with_adjacency(
            vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 0.0]],
            &[[0, 1, 2]],
        );
        g.link(NodeId::new(a, 0), 1, NodeId::new(99, 7));
        assert!(g.find_path(NodeId::new(a, 0), NodeId::new(a, 0)).is_some());
        assert_eq!(g.reachable(NodeId::new(a, 0)).len(), 1);
    }
}
