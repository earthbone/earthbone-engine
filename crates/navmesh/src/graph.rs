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
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

mod funnel;
#[cfg(test)]
mod funnel_tests;
#[cfg(test)]
mod prf_11_tests;
#[cfg(test)]
mod query_tests;
mod search;
mod spatial;
pub use search::PathSearch;
use spatial::SpatialIndex;

const WATER_FLAG: u16 = 0x0200;
const PREFERRED_FLAG: u16 = 0x0040;
const DOOR_BINDING_Z_TOLERANCE: f32 = 64.0;

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
    spatial: SpatialIndex,
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
        soe_trace::count(soe_trace::Operation::NavmeshTriangles, 1);
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

/// Stable handle for a runtime obstacle registered with a navigation graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObstacleId(u64);

/// Explicit obstacle geometry used to keep routes away from blocked triangles.
///
/// A triangle is blocked when its projected footprint intersects this shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ObstacleShape {
    /// A vertical cylinder with a base center in graph `(x, y, z)` coordinates.
    Cylinder {
        base: [f32; 3],
        radius: f32,
        height: f32,
    },
    /// An axis-aligned box in the graph's `(x, y, z)` coordinates.
    Box { min: [f32; 3], max: [f32; 3] },
}

impl ObstacleShape {
    fn is_valid(self) -> bool {
        match self {
            Self::Cylinder {
                base,
                radius,
                height,
            } => {
                base.iter().all(|value| value.is_finite())
                    && radius.is_finite()
                    && radius > 0.0
                    && height.is_finite()
                    && height > 0.0
            }
            Self::Box { min, max } => {
                min.iter().chain(max.iter()).all(|value| value.is_finite())
                    && (0..3).all(|axis| min[axis] < max[axis])
            }
        }
    }

    fn intersects_triangle(self, points: [[f32; 3]; 3]) -> bool {
        match self {
            Self::Cylinder {
                base,
                radius,
                height,
            } => {
                let bottom = base[2];
                triangle_overlaps_z(points, bottom, bottom + height)
                    && (point_in_triangle_xy([base[0], base[1]], points)
                        || (0..3).any(|edge| {
                            distance_to_segment_squared(
                                [base[0], base[1]],
                                [points[edge][0], points[edge][1]],
                                [points[(edge + 1) % 3][0], points[(edge + 1) % 3][1]],
                            ) <= radius * radius
                        }))
            }
            Self::Box { min, max } => {
                triangle_overlaps_z(points, min[2], max[2])
                    && triangle_overlaps_box_xy(points, min, max)
            }
        }
    }
}

fn triangle_overlaps_z(points: [[f32; 3]; 3], min: f32, max: f32) -> bool {
    let low = points
        .iter()
        .map(|point| point[2])
        .fold(f32::INFINITY, f32::min);
    let high = points
        .iter()
        .map(|point| point[2])
        .fold(f32::NEG_INFINITY, f32::max);
    low <= max && high >= min
}

fn point_in_triangle_xy(point: [f32; 2], triangle: [[f32; 3]; 3]) -> bool {
    let cross = |a: [f32; 3], b: [f32; 3]| {
        (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0])
    };
    let signs = [
        cross(triangle[0], triangle[1]),
        cross(triangle[1], triangle[2]),
        cross(triangle[2], triangle[0]),
    ];
    signs.iter().all(|sign| *sign >= -1e-5) || signs.iter().all(|sign| *sign <= 1e-5)
}

fn distance_to_segment_squared(point: [f32; 2], start: [f32; 2], end: [f32; 2]) -> f32 {
    let segment = [end[0] - start[0], end[1] - start[1]];
    let length_squared = segment[0] * segment[0] + segment[1] * segment[1];
    let along = if length_squared == 0.0 {
        0.0
    } else {
        (((point[0] - start[0]) * segment[0] + (point[1] - start[1]) * segment[1]) / length_squared)
            .clamp(0.0, 1.0)
    };
    let dx = point[0] - (start[0] + along * segment[0]);
    let dy = point[1] - (start[1] + along * segment[1]);
    dx * dx + dy * dy
}

fn triangle_overlaps_box_xy(triangle: [[f32; 3]; 3], min: [f32; 3], max: [f32; 3]) -> bool {
    let center = [min[0].midpoint(max[0]), min[1].midpoint(max[1])];
    let half = [(max[0] - min[0]) * 0.5, (max[1] - min[1]) * 0.5];
    let points = triangle.map(|point| [point[0] - center[0], point[1] - center[1]]);
    let axes = [
        [1.0, 0.0],
        [0.0, 1.0],
        [points[1][1] - points[0][1], points[0][0] - points[1][0]],
        [points[2][1] - points[1][1], points[1][0] - points[2][0]],
        [points[0][1] - points[2][1], points[2][0] - points[0][0]],
    ];
    axes.iter().all(|axis| {
        let projection = points.map(|point| point[0] * axis[0] + point[1] * axis[1]);
        let triangle_min = projection.into_iter().fold(f32::INFINITY, f32::min);
        let triangle_max = projection.into_iter().fold(f32::NEG_INFINITY, f32::max);
        let box_radius = half[0] * axis[0].abs() + half[1] * axis[1].abs();
        triangle_min <= box_radius && triangle_max >= -box_radius
    })
}

#[derive(Debug, Clone, Default)]
pub struct NavGraph {
    meshes: Vec<Mesh>,
    /// NAVM door references mapped to the triangles authored for them.
    door_nodes: HashMap<u64, Vec<NodeId>>,
    /// Door nodes whose placed reference footprint overlaps their triangle.
    door_bindings: HashMap<u64, Vec<NodeId>>,
    /// A closed door blocks its bound NAVM triangle during path searches.
    closed_doors: HashSet<u64>,
    door_blocks: HashMap<NodeId, usize>,
    spatial: OnceLock<SpatialIndex>,
    components: OnceLock<Vec<usize>>,
    obstacles: BTreeMap<ObstacleId, ObstacleShape>,
    next_obstacle_id: u64,
    revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
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
            spatial: SpatialIndex::default(),
        };
        mesh.recompute_bounds();
        mesh.spatial = SpatialIndex::triangles(&mesh);
        self.revision = self.revision.wrapping_add(1);
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
        self.revision = self.revision.wrapping_add(1);
        self.components.take();
        self.meshes[a.mesh as usize].triangles[a.triangle as usize].neighbors[edge] = Some(b);
    }

    pub fn set_blocked(&mut self, node: NodeId, blocked: bool) {
        self.revision = self.revision.wrapping_add(1);
        self.meshes[node.mesh as usize].triangles[node.triangle as usize].blocked = blocked;
    }

    /// Add an explicit cylinder or axis-aligned box obstacle.
    ///
    /// Routes are recalculated on the next query. Only triangles whose
    /// footprints intersecting the shape become dynamically blocked. The
    /// triangle adjacency and unrelated graph nodes remain unchanged.
    ///
    /// # Errors
    /// Returns `None` for invalid geometry or when obstacle ids are exhausted.
    ///
    /// # Panics
    /// This function does not panic.
    pub fn add_obstacle(&mut self, shape: ObstacleShape) -> Option<ObstacleId> {
        if !shape.is_valid() {
            return None;
        }
        let id = ObstacleId(self.next_obstacle_id);
        self.next_obstacle_id = self.next_obstacle_id.checked_add(1)?;
        self.revision = self.revision.wrapping_add(1);
        self.obstacles.insert(id, shape);
        Some(id)
    }

    /// Remove an obstacle and restore routes through its triangles.
    ///
    /// # Errors
    /// Returns `false` when the id is not registered.
    ///
    /// # Panics
    /// This function does not panic.
    pub fn remove_obstacle(&mut self, id: ObstacleId) -> bool {
        let changed = self.obstacles.remove(&id).is_some();
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }

    fn is_blocked(&self, node: NodeId) -> bool {
        let triangle = self.triangle(node);
        if triangle.blocked {
            return true;
        }
        let mesh = &self.meshes[node.mesh as usize];
        let points = triangle.vertices.map(|index| mesh.vertices[index as usize]);
        self.obstacles
            .values()
            .any(|shape| shape.intersects_triangle(points))
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

    /// Resolved door reference keys retained by NAVM door links.
    pub fn door_keys(&self) -> impl Iterator<Item = u64> + '_ {
        self.door_nodes.keys().copied()
    }

    /// Resolve NAVM door associations against the placed door's XY position.
    /// The authored link supplies candidate triangles, and the reference
    /// position must fall within the triangle footprint to bind it.
    pub fn bind_door_footprint(&mut self, door: u64, position: [f32; 3]) -> bool {
        let candidates = self.door_nodes(door).to_vec();
        let nodes: Vec<_> = candidates
            .into_iter()
            .filter(|node| self.contains_xy(*node, position))
            .collect();
        let binding = if nodes.is_empty() { None } else { Some(nodes) };
        let changed = self.door_bindings.get(&door) != binding.as_ref();
        if changed {
            self.revision = self.revision.wrapping_add(1);
            if self.closed_doors.contains(&door) {
                let old = self.door_bindings.get(&door).cloned().unwrap_or_default();
                self.adjust_door_blocks(&old, false);
                if let Some(nodes) = &binding {
                    self.adjust_door_blocks(nodes, true);
                }
            }
        }
        if let Some(nodes) = binding {
            self.door_bindings.insert(door, nodes);
        } else {
            self.door_bindings.remove(&door);
        }
        changed
    }

    /// Set whether a bound door is open. Closed links block only their
    /// authored door triangle. Returns whether the effective state changed.
    pub fn set_door_open(&mut self, door: u64, open: bool) -> bool {
        if !self.door_bindings.contains_key(&door) {
            return false;
        }
        let changed = if open {
            self.closed_doors.remove(&door)
        } else {
            self.closed_doors.insert(door)
        };
        if changed {
            self.revision = self.revision.wrapping_add(1);
            let nodes = self.door_bindings.get(&door).cloned().unwrap_or_default();
            self.adjust_door_blocks(&nodes, !open);
        }
        changed
    }

    fn adjust_door_blocks(&mut self, nodes: &[NodeId], closed: bool) {
        for node in nodes {
            if closed {
                *self.door_blocks.entry(*node).or_default() += 1;
            } else if let Some(count) = self.door_blocks.get_mut(node) {
                *count -= 1;
                if *count == 0 {
                    self.door_blocks.remove(node);
                }
            }
        }
    }

    fn node_blocked(&self, node: NodeId) -> bool {
        self.is_blocked(node) || self.door_blocks.contains_key(&node)
    }

    /// Whether a valid node can be traversed with current doors and obstacles.
    ///
    /// # Errors
    /// Invalid node identities return `false`.
    ///
    /// # Panics
    /// This function does not panic for valid graph geometry.
    pub fn is_walkable(&self, node: NodeId) -> bool {
        self.meshes
            .get(node.mesh as usize)
            .and_then(|mesh| mesh.triangles.get(node.triangle as usize))
            .is_some()
            && !self.node_blocked(node)
    }

    fn contains_xy(&self, node: NodeId, point: [f32; 3]) -> bool {
        let Some(mesh) = self.meshes.get(node.mesh as usize) else {
            return false;
        };
        let Some(triangle) = mesh.triangles.get(node.triangle as usize) else {
            return false;
        };
        let Some(height) = mesh.height_at(node.triangle, point[0], point[1]) else {
            return false;
        };
        if (height - point[2]).abs() > DOOR_BINDING_Z_TOLERANCE {
            return false;
        }
        let [a, b, c] = triangle.vertices.map(|index| mesh.vertices[index as usize]);
        let cross = |p: [f32; 3], q: [f32; 3], r: [f32; 3]| {
            (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
        };
        let area = cross(a, b, c);
        if !area.is_finite() || area.abs() < 1e-6 || !point[0].is_finite() || !point[1].is_finite()
        {
            return false;
        }
        let eps = area.abs() * 1e-5;
        let ab = cross(a, b, point);
        let bc = cross(b, c, point);
        let ca = cross(c, a, point);
        (ab >= -eps && bc >= -eps && ca >= -eps) || (ab <= eps && bc <= eps && ca <= eps)
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
        self.revision = self.revision.wrapping_add(1);
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
        soe_trace::count(soe_trace::Operation::NavmeshLocate, 1);
        self.locate_impl(point, |_| {})
    }

    fn locate_impl(&self, point: [f32; 3], mut tested: impl FnMut(NodeId)) -> Option<NodeId> {
        if !point.iter().all(|p| p.is_finite()) {
            return None;
        }
        let mut best: Option<(f32, NodeId)> = None;
        self.spatial
            .get_or_init(|| SpatialIndex::build(&self.meshes))
            .visit_xy_nearest_height(point, &mut f32::INFINITY, |mi, _radius| {
                let mesh = &self.meshes[mi];
                if point[0] < mesh.min[0]
                    || point[0] > mesh.max[0]
                    || point[1] < mesh.min[1]
                    || point[1] > mesh.max[1]
                {
                    return;
                }
                for ti in 0..mesh.triangles.len() {
                    tested(NodeId::new(mi as u32, ti as u32));
                    if let Some(z) = mesh.height_at(ti as u32, point[0], point[1]) {
                        let candidate = ((z - point[2]).abs(), NodeId::new(mi as u32, ti as u32));
                        if !self.is_blocked(candidate.1) && best.is_none_or(|old| candidate < old) {
                            best = Some(candidate);
                        }
                    }
                }
            });
        best.map(|(_, n)| n)
    }

    /// Whether a point lies on any triangle, including dynamically blocked ones.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    pub fn contains_point(&self, point: [f32; 3]) -> bool {
        soe_trace::count(soe_trace::Operation::NavmeshLocate, 1);
        self.contains_point_impl(point, |_| {})
    }

    fn contains_point_impl(&self, point: [f32; 3], mut tested: impl FnMut(NodeId)) -> bool {
        if !point.iter().all(|value| value.is_finite()) {
            return false;
        }
        let mut found = false;
        self.spatial
            .get_or_init(|| SpatialIndex::build(&self.meshes))
            .visit_xy_nearest_height(point, &mut f32::INFINITY, |mi, _radius| {
                let mesh = &self.meshes[mi];
                if point[0] < mesh.min[0]
                    || point[0] > mesh.max[0]
                    || point[1] < mesh.min[1]
                    || point[1] > mesh.max[1]
                {
                    return;
                }
                found |= mesh.triangles.iter().enumerate().any(|(ti, _)| {
                    tested(NodeId::new(mi as u32, ti as u32));
                    mesh.height_at(ti as u32, point[0], point[1]).is_some()
                });
            });
        found
    }

    /// The triangle whose centroid is nearest, for points just off the mesh.
    pub fn nearest(&self, point: [f32; 3]) -> Option<NodeId> {
        self.nearest_impl(point, |_| {})
    }

    fn nearest_impl(&self, point: [f32; 3], mut tested: impl FnMut(NodeId)) -> Option<NodeId> {
        if !point.iter().all(|p| p.is_finite()) {
            return None;
        }
        let mut best: Option<(f32, NodeId)> = None;
        self.spatial
            .get_or_init(|| SpatialIndex::build(&self.meshes))
            .visit_nearest(point, |mi, radius| {
                let mesh = &self.meshes[mi];
                for ti in 0..mesh.triangles.len() {
                    let node = NodeId::new(mi as u32, ti as u32);
                    tested(node);
                    if self.node_blocked(node) {
                        continue;
                    }
                    let candidate = (distance(mesh.centroid(ti as u32), point), node);
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
        let mut search = self.start_search(from, to, endpoints, costs)?;
        match search.advance(self, usize::MAX) {
            std::task::Poll::Ready(path) => path,
            std::task::Poll::Pending => None,
        }
    }

    /// Prepare query caches before timed actor work.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic for valid graph geometry.
    pub fn prepare_queries(&self) {
        self.spatial
            .get_or_init(|| SpatialIndex::build(&self.meshes));
        self.components();
    }

    /// Revision of geometry, links and walkability for transient query caches.
    ///
    /// Compare revisions only on the same graph. Mutating query inputs advances
    /// this value, including door bindings, flags and obstacles.
    ///
    /// # Errors
    /// This function does not return errors.
    ///
    /// # Panics
    /// This function does not panic.
    pub fn query_revision(&self) -> u64 {
        self.revision
    }

    /// Resolve endpoints once and start a resumable point-to-point search.
    ///
    /// # Errors
    /// Returns `None` for invalid endpoints or an impossible mesh component.
    ///
    /// # Panics
    /// This function does not panic for valid graph geometry.
    pub fn start_path_between(&self, from: [f32; 3], to: [f32; 3]) -> Option<PathSearch> {
        let a = self.locate(from).or_else(|| self.nearest(from))?;
        let b = self.locate(to).or_else(|| self.nearest(to))?;
        self.start_search(a, b, Some((from, to)), PathCosts::default())
    }

    fn start_search(
        &self,
        from: NodeId,
        to: NodeId,
        endpoints: Option<([f32; 3], [f32; 3])>,
        costs: PathCosts,
    ) -> Option<PathSearch> {
        let _span = soe_trace::Span::new("navmesh.find_path", "navmesh");
        soe_trace::trace!(target: "navmesh", system = "navmesh", from = ?from, to = ?to, "finding path");
        if !costs.is_valid() {
            return None;
        }
        self.meshes
            .get(from.mesh as usize)?
            .triangles
            .get(from.triangle as usize)?;
        self.meshes
            .get(to.mesh as usize)?
            .triangles
            .get(to.triangle as usize)?;
        if self.node_blocked(from) || self.node_blocked(to) {
            return None;
        }
        if from.mesh != to.mesh
            && self.components()[from.mesh as usize] != self.components()[to.mesh as usize]
        {
            return None;
        }
        Some(PathSearch::new(self, from, to, endpoints, costs))
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
            if self.node_blocked(n) {
                continue;
            }
            soe_trace::count(soe_trace::Operation::PathNodes, 1);
            out.push(n);
            for next in self.triangle(n).neighbors.iter().flatten() {
                if self
                    .meshes
                    .get(next.mesh as usize)
                    .is_some_and(|m| (next.triangle as usize) < m.triangles.len())
                    && !self.node_blocked(*next)
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

    #[test]
    fn ai_09_physical_door_binding_blocks_its_navm_triangle_until_opened() {
        let mut graph = NavGraph::new();
        let mesh = graph.add_mesh_with_adjacency(
            vec![
                [0.0, 0.0, 0.0],
                [10.0, 0.0, 0.0],
                [10.0, 10.0, 0.0],
                [0.0, 10.0, 0.0],
            ],
            &[[0, 1, 2], [0, 2, 3]],
        );
        let door = 0x0001_0000_0000_0001;
        let linked = NodeId::new(mesh, 0);
        let other = NodeId::new(mesh, 1);
        graph.add_door_node(door, linked);

        assert!(graph.bind_door_footprint(door, [7.0, 2.0, 0.0]));
        assert!(graph.bind_door_footprint(door, [20.0, 20.0, 0.0]));
        assert!(graph.find_path(linked, other).is_some());
        assert!(graph.bind_door_footprint(door, [7.0, 2.0, 0.0]));
        assert!(graph.set_door_open(door, false));
        assert!(graph.find_path(linked, other).is_none());
        assert!(graph.find_path(other, other).is_some());
        assert!(!graph.set_door_open(door, false));
        assert!(graph.set_door_open(door, true));
        assert!(graph.find_path(linked, other).is_some());
    }

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
