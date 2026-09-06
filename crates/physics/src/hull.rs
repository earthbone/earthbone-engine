// SPDX-License-Identifier: GPL-3.0-or-later

//! Faces of a convex point set. `bhkConvexVerticesShape` stores its
//! vertices and its half spaces but not which vertices bound which face,
//! and the collision routines want triangles. Bethesda hulls are small
//! (a few dozen points), so the brute-force test of every vertex triple
//! against every other vertex is fast enough and has no degenerate-case
//! machinery to get wrong.

use crate::math::Vec3;

/// Triangles covering the surface of the convex hull of `points`, wound so
/// their normals point outward. Coplanar faces come back as fans of
/// triangles that may overlap, which is harmless for closest-point
/// queries. Fewer than four non-coplanar points give an empty result.
pub fn triangles(points: &[Vec3]) -> Vec<[u32; 3]> {
    let n = points.len();
    if n < 4 {
        return Vec::new();
    }
    let extent = {
        let mut lo = points[0];
        let mut hi = points[0];
        for &p in points {
            lo = lo.min(p);
            hi = hi.max(p);
        }
        (hi - lo).max_element().max(1e-6)
    };
    let eps = extent * 1e-4;
    let mut out = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let (a, b, c) = (points[i], points[j], points[k]);
                let normal = (b - a).cross(c - a);
                if normal.length() < eps * eps {
                    continue;
                }
                let normal = normal.normalized();
                let d = normal.dot(a);
                let mut above = false;
                let mut below = false;
                for (m, &p) in points.iter().enumerate() {
                    if m == i || m == j || m == k {
                        continue;
                    }
                    let s = normal.dot(p) - d;
                    if s > eps {
                        above = true;
                    } else if s < -eps {
                        below = true;
                    }
                    if above && below {
                        break;
                    }
                }
                if above && below {
                    continue;
                }
                // Keep the face only if this triple is the lexicographically
                // first triple on its plane among the points on the plane,
                // so a coplanar face is emitted once as a fan from its first
                // two points rather than for every triple.
                let on_plane: Vec<usize> = (0..n)
                    .filter(|&m| (normal.dot(points[m]) - d).abs() <= eps)
                    .collect();
                if on_plane.len() > 3 && !(on_plane[0] == i && on_plane[1] == j) {
                    continue;
                }
                if above {
                    out.push([i as u32, k as u32, j as u32]);
                } else {
                    out.push([i as u32, j as u32, k as u32]);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_cube() -> Vec<Vec3> {
        let mut v = Vec::new();
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    v.push(Vec3::new(x, y, z));
                }
            }
        }
        v
    }

    #[test]
    fn cube_faces_point_outward() {
        let pts = unit_cube();
        let tris = triangles(&pts);
        assert_eq!(tris.len(), 12, "six faces as two triangles each");
        for t in &tris {
            let (a, b, c) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
            let normal = (b - a).cross(c - a).normalized();
            let centre = (a + b + c) * (1.0 / 3.0);
            assert!(normal.dot(centre) > 0.0, "face {t:?} faces inward");
        }
    }

    #[test]
    fn tetrahedron_has_four_faces_and_a_line_has_none() {
        let tet = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        assert_eq!(triangles(&tet).len(), 4);
        let line = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
        ];
        assert!(triangles(&line).is_empty());
    }
}
