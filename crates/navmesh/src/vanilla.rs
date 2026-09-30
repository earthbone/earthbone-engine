// SPDX-License-Identifier: GPL-3.0-or-later

//! The shipped navmeshes as a graph.
//!
//! A `NAVM` record's `NVNM` block holds triangles with three edge fields.
//! Each field is the index of the adjacent triangle in the same navmesh, or
//! `-1` for a border, unless the matching bit in the triangle's flags is set
//! (bit 0 for edge 0-1, bit 1 for edge 1-2, bit 2 for edge 2-0), in which
//! case the field indexes the record's edge link array and the link names
//! another navmesh by FormID and a triangle in it. xEdit's `wbEdgeToStr` is
//! the reference for that rule. Door links are not edges: they say which
//! triangle a door reference stands on, for the AI to open it. The builder
//! retains them as door-to-triangle associations, not walkable links.
//!
//! A `NAVM` whose winning record is deleted, which the shipped masters carry
//! and xEdit's cleaning removes, is not added. Every edge link a neighbour
//! aims at it becomes a border and is counted as repaired, apart from the
//! unresolved count that names links to navmeshes nobody loaded.

use std::collections::{HashMap, HashSet};

use soe_esm::records::navm::{Geometry, NavMesh};
use soe_esm::{tag, LoadOrder, Record};

use crate::graph::{NavGraph, NodeId, Triangle};

/// Triangle flag bits, from xEdit's `wbNavmeshTriangleFlags`.
pub mod flags {
    pub const EDGE_0_1_LINK: u16 = 0x0001;
    pub const EDGE_1_2_LINK: u16 = 0x0002;
    pub const EDGE_2_0_LINK: u16 = 0x0004;
    pub const DELETED: u16 = 0x0008;
    pub const NO_LARGE_CREATURES: u16 = 0x0010;
    pub const OVERLAPPING: u16 = 0x0020;
    pub const PREFERRED: u16 = 0x0040;
    pub const WATER: u16 = 0x0200;
    pub const DOOR: u16 = 0x0400;
    pub const FOUND: u16 = 0x0800;
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stats {
    pub meshes: usize,
    pub triangles: usize,
    /// Triangles carrying the deleted flag, kept but blocked.
    pub deleted_triangles: usize,
    /// Edge fields that point into the edge link array.
    pub edge_links: usize,
    /// Of those, links whose target navmesh is in the graph and whose target
    /// triangle exists.
    pub resolved_links: usize,
    pub unresolved_links: usize,
    /// Links whose target navmesh is deleted, cut to a border.
    pub repaired_links: usize,
    pub door_links: usize,
    /// Navmeshes whose winning record is deleted, left out of the graph.
    pub deleted_meshes: usize,
}

/// Collects navmeshes under any key, resolves their links, and hands back
/// the graph. Keys are whatever identifies a navmesh to the caller: a raw
/// FormID for a single file, a packed [`soe_esm::GlobalFormId`] for a load
/// order.
#[derive(Default)]
pub struct Builder {
    graph: NavGraph,
    index_of: HashMap<u64, u32>,
    /// (source node, edge, target key, target triangle)
    pending: Vec<(NodeId, usize, u64, i16)>,
    deleted: HashSet<u64>,
    stats: Stats,
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that the navmesh under `key` is deleted. Links to it are cut
    /// and counted as repaired when the graph is built.
    pub fn mark_deleted(&mut self, key: u64) {
        if self.deleted.insert(key) {
            self.stats.deleted_meshes += 1;
        }
    }

    /// Add one navmesh. `resolve` maps the raw FormID an edge link names in
    /// this record to the key the target was (or will be) added under.
    pub fn add(
        &mut self,
        key: u64,
        geometry: &Geometry,
        resolve: impl Fn(u32) -> Option<u64>,
    ) -> u32 {
        let mesh_index = self.graph.meshes().len() as u32;
        let count = geometry.triangles.len();
        let mut triangles = Vec::with_capacity(count);
        for (ti, t) in geometry.triangles.iter().enumerate() {
            let mut neighbors = [None; 3];
            for (edge, &value) in t.edges.iter().enumerate() {
                if t.flags & (1 << edge) != 0 {
                    self.stats.edge_links += 1;
                    let target = usize::try_from(value)
                        .ok()
                        .and_then(|i| geometry.edge_links.get(i));
                    match target.and_then(|l| resolve(l.navmesh).map(|k| (k, l.triangle))) {
                        Some((k, tri)) => {
                            self.pending
                                .push((NodeId::new(mesh_index, ti as u32), edge, k, tri));
                        }
                        None => self.stats.unresolved_links += 1,
                    }
                } else if let Ok(i) = u32::try_from(value) {
                    if (i as usize) < count {
                        neighbors[edge] = Some(NodeId::new(mesh_index, i));
                    }
                }
            }
            let deleted = t.flags & flags::DELETED != 0;
            if deleted {
                self.stats.deleted_triangles += 1;
            }
            triangles.push(Triangle {
                vertices: t.vertices.map(u32::from),
                neighbors,
                blocked: deleted,
                flags: t.flags,
            });
        }
        self.stats.meshes += 1;
        self.stats.triangles += count;
        self.stats.door_links += geometry.door_links.len();
        let index = self.graph.add_mesh(geometry.vertices.clone(), triangles);
        for door_link in &geometry.door_links {
            let Some(door) = resolve(door_link.door) else {
                continue;
            };
            let Ok(triangle) = u32::try_from(door_link.triangle) else {
                continue;
            };
            if (triangle as usize) < count {
                self.graph.add_door_node(door, NodeId::new(index, triangle));
            }
        }
        self.index_of.insert(key, index);
        index
    }

    pub fn index_of(&self, key: u64) -> Option<u32> {
        self.index_of.get(&key).copied()
    }

    /// Resolve every cross-mesh link that can be resolved and finish.
    pub fn build(mut self) -> (NavGraph, Stats) {
        let pending = std::mem::take(&mut self.pending);
        for (from, edge, key, tri) in pending {
            if self.deleted.contains(&key) {
                self.stats.repaired_links += 1;
                continue;
            }
            let target = self
                .index_of
                .get(&key)
                .and_then(|&m| u32::try_from(tri).ok().map(|t| NodeId::new(m, t)))
                .filter(|n| (n.triangle as usize) < self.graph.mesh(n.mesh).triangles.len());
            match target {
                Some(n) => {
                    self.graph.link(from, edge, n);
                    self.stats.resolved_links += 1;
                }
                None => self.stats.unresolved_links += 1,
            }
        }
        (self.graph, self.stats)
    }
}

/// Pack a global id into a builder key.
pub fn key_of(id: soe_esm::GlobalFormId) -> u64 {
    (u64::from(id.file) << 32) | u64::from(id.object)
}

/// Every winning `NAVM` in a load order that `keep` accepts (a worldspace
/// filter, typically), linked through the load order's FormID resolution.
pub fn from_load_order(order: &LoadOrder, keep: impl Fn(&Record) -> bool) -> (NavGraph, Stats) {
    let mut b = Builder::new();
    let mut winners: Vec<_> = order
        .winners_of(tag::NAVM)
        .filter(|(_, r)| keep(r))
        .filter_map(|(id, _)| order.winner_ref(id).map(|r| (id, r)))
        .collect();
    winners.sort_by_key(|(id, _)| *id);
    for (id, rref) in winners {
        let rec = order.record(rref);
        if rec.is_deleted() {
            b.mark_deleted(key_of(id));
            continue;
        }
        let Ok(navm) = NavMesh::from_record(rec) else {
            continue;
        };
        let Some(geometry) = navm.geometry else {
            continue;
        };
        let file = rref.file;
        b.add(key_of(id), &geometry, |raw| {
            Some(key_of(order.resolve(file, raw)))
        });
    }
    b.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::records::navm::{EdgeLink, NavGrid, Parent, Triangle as NavmTriangle};

    fn square(origin: f32, link_to: Option<(u32, i16)>) -> Geometry {
        let o = origin;
        let mut t0 = NavmTriangle {
            vertices: [0, 1, 2],
            edges: [-1, -1, 1],
            flags: 0,
            cover_flags: 0,
        };
        let t1 = NavmTriangle {
            vertices: [0, 2, 3],
            edges: [0, -1, -1],
            flags: 0,
            cover_flags: 0,
        };
        let mut edge_links = Vec::new();
        if let Some((mesh, tri)) = link_to {
            // Edge 1-2 of t0 (the right side, x = o + 100) links out.
            t0.edges[1] = 0;
            t0.flags |= flags::EDGE_1_2_LINK;
            edge_links.push(EdgeLink {
                kind: 2,
                navmesh: mesh,
                triangle: tri,
            });
        }
        Geometry {
            version: 12,
            location_crc: 0,
            parent: Parent::Grid {
                world: 0x3C,
                x: 0,
                y: 0,
            },
            vertices: vec![
                [o, 0.0, 0.0],
                [o + 100.0, 0.0, 0.0],
                [o + 100.0, 100.0, 0.0],
                [o, 100.0, 0.0],
            ],
            triangles: vec![t0, t1],
            edge_links,
            door_links: vec![],
            cover_triangles: vec![],
            grid: NavGrid {
                divisor: 1,
                size: [100.0, 100.0],
                min: [o, 0.0, 0.0],
                max: [o + 100.0, 100.0, 0.0],
                cells: vec![vec![0, 1]],
            },
        }
    }

    #[test]
    fn in_mesh_edges_and_cross_mesh_links_both_resolve() {
        let mut b = Builder::new();
        // Mesh 0x10 at x 0..100 links its right edge to mesh 0x20's triangle 1.
        b.add(0x10, &square(0.0, Some((0x20, 1))), |raw| {
            Some(u64::from(raw))
        });
        // Mesh 0x20 at x 100..200. Its triangle 1 has vertices 0,2,3 whose edge
        // 2-0 is its left side, linking back.
        let mut back = square(100.0, None);
        back.triangles[1].edges[2] = 0;
        back.triangles[1].flags |= flags::EDGE_2_0_LINK;
        back.edge_links.push(EdgeLink {
            kind: 2,
            navmesh: 0x10,
            triangle: 0,
        });
        b.add(0x20, &back, |raw| Some(u64::from(raw)));
        let (g, stats) = b.build();
        assert_eq!(stats.meshes, 2);
        assert_eq!(stats.triangles, 4);
        assert_eq!(stats.edge_links, 2);
        assert_eq!(stats.resolved_links, 2);
        assert_eq!(stats.unresolved_links, 0);
        let p = g.find_path(NodeId::new(0, 1), NodeId::new(1, 0)).unwrap();
        assert_eq!(p.nodes.len(), 4, "{:?}", p.nodes);
    }

    #[test]
    fn a_link_to_a_navmesh_that_is_not_loaded_is_counted_not_followed() {
        let mut b = Builder::new();
        b.add(0x10, &square(0.0, Some((0x999, 0))), |raw| {
            Some(u64::from(raw))
        });
        let (g, stats) = b.build();
        assert_eq!(stats.unresolved_links, 1);
        assert_eq!(stats.resolved_links, 0);
        assert_eq!(g.triangle(NodeId::new(0, 0)).neighbors[1], None);
    }

    #[test]
    fn a_link_to_a_deleted_navmesh_is_cut_and_counted_as_repaired() {
        let mut b = Builder::new();
        b.add(0x10, &square(0.0, Some((0x20, 1))), |raw| {
            Some(u64::from(raw))
        });
        b.mark_deleted(0x20);
        b.mark_deleted(0x20);
        let (g, stats) = b.build();
        assert_eq!(stats.deleted_meshes, 1);
        assert_eq!(stats.repaired_links, 1);
        assert_eq!(stats.unresolved_links, 0);
        assert_eq!(stats.resolved_links, 0);
        assert_eq!(g.meshes().len(), 1);
        assert_eq!(g.triangle(NodeId::new(0, 0)).neighbors[1], None);
    }

    #[test]
    fn a_deleted_triangle_is_blocked() {
        let mut g = square(0.0, None);
        g.triangles[1].flags |= flags::DELETED;
        let mut b = Builder::new();
        b.add(1, &g, |_| None);
        let (graph, stats) = b.build();
        assert_eq!(stats.deleted_triangles, 1);
        assert!(graph
            .find_path(NodeId::new(0, 0), NodeId::new(0, 1))
            .is_none());
    }

    #[test]
    fn builder_retains_triangle_flags_for_path_costs() {
        let mut navmesh = square(0.0, None);
        navmesh.triangles[0].flags |= flags::PREFERRED;
        navmesh.triangles[1].flags |= flags::WATER;
        let mut builder = Builder::new();
        builder.add(1, &navmesh, |_| None);
        let (graph, _) = builder.build();
        assert_eq!(graph.triangle(NodeId::new(0, 0)).flags, flags::PREFERRED);
        assert_eq!(graph.triangle(NodeId::new(0, 1)).flags, flags::WATER);
    }

    #[test]
    fn a_bad_edge_index_or_link_index_is_a_border() {
        let mut g = square(0.0, None);
        g.triangles[0].edges[0] = 57;
        g.triangles[0].edges[1] = 9;
        g.triangles[0].flags |= flags::EDGE_1_2_LINK;
        let mut b = Builder::new();
        b.add(1, &g, |_| Some(1));
        let (graph, stats) = b.build();
        let t = graph.triangle(NodeId::new(0, 0));
        assert_eq!(t.neighbors[0], None);
        assert_eq!(t.neighbors[1], None);
        assert_eq!(stats.unresolved_links, 1);
    }
}
