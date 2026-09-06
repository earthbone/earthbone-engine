// SPDX-License-Identifier: GPL-3.0-or-later

//! The triangle soup a navmesh is generated from, in Skyrim's frame.
//!
//! What goes in is whatever the character can stand on or bump into: the
//! render geometry of placed statics (the NIF collision blocks are still
//! opaque in `soe-nif`, so render meshes stand in for them, which is what
//! the Creation Kit's own navmesh generator uses as well), the terrain from
//! `LAND` heights, and test boxes.

use soe_nif::{Mesh, Vec3};

/// Side of an exterior cell in game units.
pub const CELL_SIZE: f32 = 4096.0;
/// Vertices per side of a `LAND` height grid.
pub const LAND_VERTS_PER_SIDE: usize = 33;
/// Spacing between `LAND` vertices.
pub const LAND_VERTEX_SPACING: f32 = CELL_SIZE / (LAND_VERTS_PER_SIDE - 1) as f32;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    pub positions: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }

    /// Append triangles given as positions plus a triangle list.
    pub fn add_triangles(&mut self, positions: &[[f32; 3]], triangles: &[[u32; 3]]) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(positions);
        self.triangles.extend(
            triangles
                .iter()
                .map(|t| [t[0] + base, t[1] + base, t[2] + base]),
        );
    }

    /// A NIF shape, placed by its own accumulated transform and then by
    /// `placement` (the `REFR` transform, identity for a test).
    pub fn add_nif_mesh(&mut self, mesh: &Mesh, placement: &soe_nif::Transform) {
        let positions: Vec<[f32; 3]> = mesh
            .positions
            .iter()
            .map(|p| {
                placement
                    .apply(mesh.transform.apply(Vec3::new(p[0], p[1], p[2])))
                    .to_array()
            })
            .collect();
        let triangles: Vec<[u32; 3]> = mesh.indices.as_chunks::<3>().0.to_vec();
        self.add_triangles(&positions, &triangles);
    }

    /// One exterior cell of terrain from its decoded heights (1089 values,
    /// row-major, row 0 at the south edge, as `Land::heights.decode` gives).
    pub fn add_land(&mut self, cell_x: i32, cell_y: i32, heights: &[f32]) {
        let n = LAND_VERTS_PER_SIDE;
        assert_eq!(heights.len(), n * n, "a LAND grid has {} heights", n * n);
        let ox = cell_x as f32 * CELL_SIZE;
        let oy = cell_y as f32 * CELL_SIZE;
        let mut positions = Vec::with_capacity(n * n);
        for row in 0..n {
            for col in 0..n {
                positions.push([
                    ox + col as f32 * LAND_VERTEX_SPACING,
                    oy + row as f32 * LAND_VERTEX_SPACING,
                    heights[row * n + col],
                ]);
            }
        }
        let mut triangles = Vec::with_capacity((n - 1) * (n - 1) * 2);
        for row in 0..n - 1 {
            for col in 0..n - 1 {
                let bl = (row * n + col) as u32;
                let br = bl + 1;
                let tl = bl + n as u32;
                let tr = tl + 1;
                // Counter-clockwise seen from above, so the normal points up.
                triangles.push([bl, br, tr]);
                triangles.push([bl, tr, tl]);
            }
        }
        self.add_triangles(&positions, &triangles);
    }

    /// An axis-aligned box, outward-facing.
    pub fn add_box(&mut self, min: [f32; 3], max: [f32; 3]) {
        let p = |x: usize, y: usize, z: usize| {
            [
                if x == 0 { min[0] } else { max[0] },
                if y == 0 { min[1] } else { max[1] },
                if z == 0 { min[2] } else { max[2] },
            ]
        };
        let positions = [
            p(0, 0, 0),
            p(1, 0, 0),
            p(1, 1, 0),
            p(0, 1, 0),
            p(0, 0, 1),
            p(1, 0, 1),
            p(1, 1, 1),
            p(0, 1, 1),
        ];
        let triangles = [
            [4, 5, 6],
            [4, 6, 7], // top, up
            [0, 2, 1],
            [0, 3, 2], // bottom, down
            [0, 1, 5],
            [0, 5, 4], // south (y = min)
            [1, 2, 6],
            [1, 6, 5], // east (x = max)
            [2, 3, 7],
            [2, 7, 6], // north
            [3, 0, 4],
            [3, 4, 7], // west
        ];
        self.add_triangles(&positions, &triangles);
    }

    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        if self.positions.is_empty() {
            return None;
        }
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for p in &self.positions {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        Some((min, max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flat_land_cell_covers_its_square_with_upward_triangles() {
        let mut s = Scene::new();
        s.add_land(2, -1, &vec![10.0; 33 * 33]);
        assert_eq!(s.positions.len(), 1089);
        assert_eq!(s.triangles.len(), 32 * 32 * 2);
        let (min, max) = s.bounds().unwrap();
        assert_eq!(min, [8192.0, -4096.0, 10.0]);
        assert_eq!(max, [12288.0, 0.0, 10.0]);
        for t in &s.triangles {
            let [a, b, c] = t.map(|i| s.positions[i as usize]);
            let ux = b[0] - a[0];
            let uy = b[1] - a[1];
            let vx = c[0] - a[0];
            let vy = c[1] - a[1];
            assert!(ux * vy - uy * vx > 0.0, "clockwise triangle {t:?}");
        }
    }

    #[test]
    fn a_box_has_twelve_triangles_and_its_top_faces_up() {
        let mut s = Scene::new();
        s.add_box([0.0, 0.0, 0.0], [10.0, 20.0, 30.0]);
        assert_eq!(s.triangles.len(), 12);
        let [a, b, c] = s.triangles[0].map(|i| s.positions[i as usize]);
        assert_eq!(a[2], 30.0);
        let n = [
            (b[1] - a[1]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[1] - a[1]),
            (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]),
            (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]),
        ];
        assert!(n[2] > 0.0);
    }

    #[test]
    fn appended_triangles_are_offset() {
        let mut s = Scene::new();
        s.add_triangles(&[[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], &[[0, 1, 2]]);
        s.add_triangles(&[[5.0; 3], [6.0, 5.0, 5.0], [5.0, 6.0, 5.0]], &[[0, 1, 2]]);
        assert_eq!(s.triangles[1], [3, 4, 5]);
    }
}
