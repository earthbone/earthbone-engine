// SPDX-License-Identifier: GPL-3.0-or-later

//! Navmesh generation with Recast, through the `rerecast` port.
//!
//! The pipeline is the one Recast's solo-mesh sample runs: rasterize the
//! scene into a heightfield, filter spans the agent cannot use, compact,
//! erode by the agent radius, build a distance field and watershed regions,
//! trace contours, and turn them into convex polygons. The detail mesh
//! stage is skipped: polygon vertices already sit on the walkable surface
//! to within one cell height, which is finer than the vanilla navmeshes.
//!
//! Recast is Y up and this crate's boundary is Z up. Skyrim `(x, y, z)`
//! becomes Recast `(x, z, -y)`, the same map the Godot host uses, and the
//! polygons come back through the inverse.

use std::collections::HashMap;

use glam::{UVec3, Vec3, Vec3A};
use rerecast::{Aabb3d, AreaType, BuildContoursFlags, HeightfieldBuilder, PolygonNavmesh, TriMesh};

use soe_esm::records::navm::{EdgeLink, Geometry, NavGrid, Parent, Triangle as NavmTriangle};

use crate::graph::NavGraph;
use crate::scene::Scene;
use crate::{Error, Result};

/// Who the navmesh is for, in game units. The defaults describe a human
/// sized actor: 128 units is roughly 1.83 m at Skyrim's scale.
#[derive(Debug, Clone, PartialEq)]
pub struct Agent {
    pub height: f32,
    pub radius: f32,
    /// The tallest step the actor walks up without a jump.
    pub climb: f32,
    pub max_slope_degrees: f32,
    /// Voxel size in the ground plane. Smaller is more exact and slower.
    pub cell_size: f32,
    /// Voxel size in height.
    pub cell_height: f32,
    /// Regions smaller than this many cells squared are dropped.
    pub min_region_size: u16,
    /// Regions smaller than this many cells squared are merged.
    pub merge_region_size: u16,
    /// Longest polygon edge, in multiples of the agent radius in cells.
    pub edge_max_len_factor: u16,
    /// Contour simplification tolerance in cells.
    pub max_simplification_error: f32,
    pub max_vertices_per_polygon: u16,
    /// Refuse heightfields with more columns than this, since a world-sized
    /// scene at a fine cell size is a mistake rather than a job.
    pub max_columns: usize,
}

impl Default for Agent {
    fn default() -> Self {
        Agent {
            height: 128.0,
            radius: 24.0,
            climb: 40.0,
            max_slope_degrees: 50.0,
            cell_size: 12.0,
            cell_height: 8.0,
            min_region_size: 8,
            merge_region_size: 20,
            edge_max_len_factor: 8,
            max_simplification_error: 1.3,
            max_vertices_per_polygon: 6,
            max_columns: 64 * 1024 * 1024,
        }
    }
}

impl Agent {
    fn validate(&self) -> Result<()> {
        let positive = [
            ("height", self.height),
            ("radius", self.radius),
            ("cell_size", self.cell_size),
            ("cell_height", self.cell_height),
        ];
        for (name, v) in positive {
            if v <= 0.0 || !v.is_finite() {
                return Err(Error::Agent(format!("{name} must be positive, got {v}")));
            }
        }
        if self.climb < 0.0 || !self.climb.is_finite() {
            return Err(Error::Agent(format!(
                "climb must not be negative, got {}",
                self.climb
            )));
        }
        if !(0.0..90.0).contains(&self.max_slope_degrees) {
            return Err(Error::Agent(format!(
                "max_slope_degrees must be in 0 to 90, got {}",
                self.max_slope_degrees
            )));
        }
        if self.max_vertices_per_polygon < 3 {
            return Err(Error::Agent(
                "max_vertices_per_polygon must be at least 3".into(),
            ));
        }
        Ok(())
    }

    fn walkable_height(&self) -> u16 {
        (self.height / self.cell_height).ceil().max(1.0) as u16
    }

    fn walkable_climb(&self) -> u16 {
        (self.climb / self.cell_height).floor() as u16
    }

    fn walkable_radius(&self) -> u16 {
        (self.radius / self.cell_size).ceil() as u16
    }
}

/// A generated navmesh: convex polygons in Skyrim's frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Navmesh {
    pub vertices: Vec<[f32; 3]>,
    /// Each polygon is three or more vertex indices, counter-clockwise seen
    /// from above.
    pub polygons: Vec<Vec<u32>>,
    /// For polygon `p` and its edge `i` (from vertex `i` to `i + 1`), the
    /// neighbouring polygon, when any.
    pub neighbors: Vec<Vec<Option<u32>>>,
}

fn to_recast(p: [f32; 3]) -> Vec3A {
    Vec3A::new(p[0], p[2], -p[1])
}

fn from_recast(v: Vec3) -> [f32; 3] {
    [v.x, -v.z, v.y]
}

/// Generate a navmesh for `agent` over `scene`.
pub fn generate(scene: &Scene, agent: &Agent) -> Result<Navmesh> {
    agent.validate()?;
    let (min, max) = scene.bounds().ok_or(Error::EmptyScene)?;
    if scene.triangles.is_empty() {
        return Err(Error::EmptyScene);
    }
    // Pad by one radius so erosion at the scene edge does not eat into the
    // walkable area, and by one cell so the last column is not dropped.
    let pad = agent.radius + agent.cell_size;
    let lo = to_recast([min[0], max[1], min[2]]);
    let hi = to_recast([max[0], min[1], max[2]]);
    let aabb = Aabb3d {
        min: Vec3::new(lo.x - pad, lo.y - agent.cell_height, lo.z - pad),
        max: Vec3::new(hi.x + pad, hi.y + agent.height, hi.z + pad),
    };
    let width = ((aabb.max.x - aabb.min.x) / agent.cell_size + 0.5) as usize;
    let depth = ((aabb.max.z - aabb.min.z) / agent.cell_size + 0.5) as usize;
    if width == 0 || depth == 0 {
        return Err(Error::Agent("the scene is thinner than one cell".into()));
    }
    if width.saturating_mul(depth) > agent.max_columns
        || width > u16::MAX as usize
        || depth > u16::MAX as usize
    {
        return Err(Error::Agent(format!(
            "{width} by {depth} cells exceeds the limit, raise cell_size or split the scene"
        )));
    }

    let mut trimesh = TriMesh {
        vertices: scene.positions.iter().map(|p| to_recast(*p)).collect(),
        indices: scene
            .triangles
            .iter()
            .map(|t| UVec3::new(t[0], t[1], t[2]))
            .collect(),
        area_types: vec![AreaType::NOT_WALKABLE; scene.triangles.len()],
    };
    if let Some(bad) = scene
        .triangles
        .iter()
        .find(|t| t.iter().any(|&i| i as usize >= scene.positions.len()))
    {
        return Err(Error::Generate(format!(
            "triangle {bad:?} indexes past the positions"
        )));
    }
    trimesh.mark_walkable_triangles(agent.max_slope_degrees.to_radians());

    let mut heightfield = HeightfieldBuilder {
        aabb,
        cell_size: agent.cell_size,
        cell_height: agent.cell_height,
    }
    .build()
    .map_err(|e| Error::Generate(e.to_string()))?;
    heightfield
        .populate_from_trimesh(trimesh, agent.walkable_height(), agent.walkable_climb())
        .map_err(|e| Error::Generate(e.to_string()))?;

    let mut compact = heightfield
        .into_compact(agent.walkable_height(), agent.walkable_climb())
        .map_err(|e| Error::Generate(e.to_string()))?;
    compact.erode_walkable_area(agent.walkable_radius());
    compact.build_distance_field();
    let min_area = agent.min_region_size.saturating_mul(agent.min_region_size);
    let merge_area = agent
        .merge_region_size
        .saturating_mul(agent.merge_region_size);
    compact
        .build_regions(0, min_area, merge_area)
        .map_err(|e| Error::Generate(e.to_string()))?;

    let max_edge_len = agent
        .walkable_radius()
        .saturating_mul(agent.edge_max_len_factor);
    let contours = compact.build_contours(
        agent.max_simplification_error,
        max_edge_len,
        BuildContoursFlags::default(),
    );
    let mesh = contours
        .into_polygon_mesh(agent.max_vertices_per_polygon)
        .map_err(|e| Error::Generate(e.to_string()))?;
    Ok(convert(&mesh))
}

fn convert(mesh: &PolygonNavmesh) -> Navmesh {
    let vertices = mesh
        .vertices
        .iter()
        .map(|v| {
            from_recast(Vec3::new(
                mesh.aabb.min.x + f32::from(v.x) * mesh.cell_size,
                mesh.aabb.min.y + f32::from(v.y) * mesh.cell_height,
                mesh.aabb.min.z + f32::from(v.z) * mesh.cell_size,
            ))
        })
        .collect();
    let nvp = mesh.max_vertices_per_polygon as usize;
    let mut polygons = Vec::with_capacity(mesh.polygon_count());
    let mut neighbors = Vec::with_capacity(mesh.polygon_count());
    for (poly, adj) in mesh
        .polygons
        .chunks_exact(nvp)
        .zip(mesh.polygon_neighbors.chunks_exact(nvp))
    {
        let count = poly
            .iter()
            .take_while(|&&i| i != PolygonNavmesh::NO_INDEX)
            .count();
        // Recast winds polygons clockwise seen from +Y. After the axis map
        // (which mirrors one axis) that is counter-clockwise seen from +Z,
        // so the order is kept as is.
        polygons.push(
            poly[..count]
                .iter()
                .map(|&i| u32::from(i))
                .collect::<Vec<_>>(),
        );
        neighbors.push(
            adj[..count]
                .iter()
                .map(|&n| {
                    (n != PolygonNavmesh::NO_CONNECTION && n & 0x8000 == 0).then_some(u32::from(n))
                })
                .collect::<Vec<_>>(),
        );
    }
    Navmesh {
        vertices,
        polygons,
        neighbors,
    }
}

impl Navmesh {
    pub fn polygon_count(&self) -> usize {
        self.polygons.len()
    }

    /// Fan-triangulate every polygon. Returns the triangles and, for each,
    /// the polygon it came from.
    pub fn triangles(&self) -> (Vec<[u32; 3]>, Vec<u32>) {
        let mut tris = Vec::new();
        let mut owner = Vec::new();
        for (pi, poly) in self.polygons.iter().enumerate() {
            for i in 1..poly.len().saturating_sub(1) {
                tris.push([poly[0], poly[i], poly[i + 1]]);
                owner.push(pi as u32);
            }
        }
        (tris, owner)
    }

    /// The mesh as a walkable graph, one mesh, adjacency from shared edges.
    pub fn to_graph(&self) -> NavGraph {
        let (tris, _) = self.triangles();
        let mut g = NavGraph::new();
        g.add_mesh_with_adjacency(self.vertices.clone(), &tris);
        g
    }

    /// The mesh as an `NVNM` block, ready to sit in a `NAVM` record. Edge
    /// links to other navmeshes are not produced, every border is a border.
    pub fn to_geometry(&self, parent: Parent, grid_divisor: u32) -> Result<Geometry> {
        if self.vertices.len() > usize::from(u16::MAX) {
            return Err(Error::Export(format!(
                "{} vertices, NVNM indexes them with 16 bits",
                self.vertices.len()
            )));
        }
        let (tris, _) = self.triangles();
        if tris.len() >= usize::from(i16::MAX as u16) {
            return Err(Error::Export(format!(
                "{} triangles, NVNM indexes them with a signed 16-bit field",
                tris.len()
            )));
        }
        let mut by_edge: HashMap<(u32, u32), Vec<(usize, usize)>> = HashMap::new();
        for (ti, t) in tris.iter().enumerate() {
            for e in 0..3 {
                let a = t[e];
                let b = t[(e + 1) % 3];
                by_edge
                    .entry((a.min(b), a.max(b)))
                    .or_default()
                    .push((ti, e));
            }
        }
        let mut triangles: Vec<NavmTriangle> = tris
            .iter()
            .map(|t| NavmTriangle {
                vertices: [t[0] as u16, t[1] as u16, t[2] as u16],
                edges: [-1; 3],
                flags: 0,
                cover_flags: 0,
            })
            .collect();
        for users in by_edge.values() {
            if users.len() == 2 {
                let (t0, e0) = users[0];
                let (t1, e1) = users[1];
                triangles[t0].edges[e0] = t1 as i16;
                triangles[t1].edges[e1] = t0 as i16;
            }
        }
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for v in &self.vertices {
            for i in 0..3 {
                min[i] = min[i].min(v[i]);
                max[i] = max[i].max(v[i]);
            }
        }
        if self.vertices.is_empty() {
            min = [0.0; 3];
            max = [0.0; 3];
        }
        let divisor = grid_divisor.max(1);
        let size = [
            ((max[0] - min[0]) / divisor as f32).max(1.0),
            ((max[1] - min[1]) / divisor as f32).max(1.0),
        ];
        let mut cells: Vec<Vec<i16>> = vec![Vec::new(); (divisor * divisor) as usize];
        for (ti, t) in tris.iter().enumerate() {
            let pts = t.map(|i| self.vertices[i as usize]);
            let cell_range = |axis: usize| {
                let lo = pts.iter().map(|p| p[axis]).fold(f32::INFINITY, f32::min);
                let hi = pts
                    .iter()
                    .map(|p| p[axis])
                    .fold(f32::NEG_INFINITY, f32::max);
                let a = (((lo - min[axis]) / size[axis]).floor() as i64)
                    .clamp(0, i64::from(divisor) - 1);
                let b = (((hi - min[axis]) / size[axis]).floor() as i64)
                    .clamp(0, i64::from(divisor) - 1);
                a..=b
            };
            for y in cell_range(1) {
                for x in cell_range(0) {
                    cells[(y * i64::from(divisor) + x) as usize].push(ti as i16);
                }
            }
        }
        Ok(Geometry {
            version: 12,
            location_crc: 0,
            parent,
            vertices: self.vertices.clone(),
            triangles,
            edge_links: Vec::<EdgeLink>::new(),
            door_links: Vec::new(),
            cover_triangles: Vec::new(),
            grid: NavGrid {
                divisor,
                size,
                min,
                max,
                cells,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_axis_map_round_trips_and_keeps_up_up() {
        let p = [1.0, 2.0, 3.0];
        let r = to_recast(p);
        assert_eq!(r, Vec3A::new(1.0, 3.0, -2.0));
        assert_eq!(from_recast(Vec3::from(r)), p);
    }

    #[test]
    fn bad_agents_are_refused_before_any_work() {
        let mut scene = Scene::new();
        scene.add_land(0, 0, &vec![0.0; 33 * 33]);
        let mut a = Agent {
            radius: -1.0,
            ..Agent::default()
        };
        assert!(matches!(generate(&scene, &a), Err(Error::Agent(_))));
        a.radius = 24.0;
        a.max_slope_degrees = 95.0;
        assert!(matches!(generate(&scene, &a), Err(Error::Agent(_))));
        a.max_slope_degrees = 50.0;
        a.max_columns = 10;
        let err = generate(&scene, &a).unwrap_err();
        assert!(err.to_string().contains("exceeds the limit"), "{err}");
        assert!(matches!(
            generate(&Scene::new(), &Agent::default()),
            Err(Error::EmptyScene)
        ));
    }

    #[test]
    fn a_flat_cell_becomes_a_few_polygons_covering_it() {
        let mut scene = Scene::new();
        scene.add_land(0, 0, &vec![100.0; 33 * 33]);
        let mesh = generate(&scene, &Agent::default()).unwrap();
        assert!(mesh.polygon_count() > 0);
        assert_eq!(mesh.polygons.len(), mesh.neighbors.len());
        for v in &mesh.vertices {
            assert!((v[2] - 100.0).abs() <= 8.0, "vertex off the ground: {v:?}");
            assert!(
                v[0] >= -1.0 && v[0] <= 4097.0 && v[1] >= -1.0 && v[1] <= 4097.0,
                "{v:?}"
            );
        }
        // Counter-clockwise seen from above, so the fan normals point up.
        for poly in &mesh.polygons {
            let a = mesh.vertices[poly[0] as usize];
            let b = mesh.vertices[poly[1] as usize];
            let c = mesh.vertices[poly[2] as usize];
            let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            assert!(cross > 0.0, "clockwise polygon {poly:?}");
        }
        let g = mesh.to_graph();
        let from = [300.0, 300.0, 100.0];
        let to = [3800.0, 3800.0, 100.0];
        let p = g.find_path_between(from, to).unwrap();
        assert_eq!((p.points[0], *p.points.last().unwrap()), (from, to));
        for pt in &p.points {
            assert!(
                pt[0] >= 0.0 && pt[0] <= 4096.0 && pt[1] >= 0.0 && pt[1] <= 4096.0,
                "{pt:?}"
            );
        }
        eprintln!(
            "{} polygons, path of {} triangles, cost {:.0}",
            mesh.polygon_count(),
            p.nodes.len(),
            p.cost
        );
    }

    #[test]
    fn the_export_round_trips_through_nvnm() {
        let mut scene = Scene::new();
        scene.add_land(1, 1, &vec![0.0; 33 * 33]);
        let mesh = generate(&scene, &Agent::default()).unwrap();
        let geometry = mesh
            .to_geometry(
                Parent::Grid {
                    world: 0x3C,
                    x: 1,
                    y: 1,
                },
                4,
            )
            .unwrap();
        let bytes = geometry.to_bytes();
        let back = Geometry::parse(soe_esm::tag::NAVM, &bytes).unwrap();
        assert_eq!(back, geometry);
        assert_eq!(back.grid.cells.len(), 16);
        assert!(back.grid.cells.iter().any(|c| !c.is_empty()));
        // Every in-mesh edge is mutual.
        for (ti, t) in back.triangles.iter().enumerate() {
            for (e, &n) in t.edges.iter().enumerate() {
                if n >= 0 {
                    let other = &back.triangles[n as usize];
                    assert!(
                        other.edges.contains(&(ti as i16)),
                        "edge {e} of {ti} is one way"
                    );
                }
            }
        }
    }
}
