// SPDX-License-Identifier: GPL-3.0-or-later

//! Faces of a convex point set. `bhkConvexVerticesShape` stores its
//! vertices and its half spaces but not which vertices bound which face,
//! and the collision routines want triangles. Bethesda hulls are small
//! (a few dozen points), so supporting planes are found by vertex triples.
//! Coplanar faces retain an ordered convex boundary, as in Jolt's local
//! Geometry/ConvexHullBuilder and ConvexHullBuilder2D references.

use crate::math::Vec3;
use std::collections::BTreeSet;

/// Triangles covering the surface of the convex hull of `points`, wound so
/// their normals point outward. Coplanar faces use an ordered boundary fan.
/// Fewer than four non-coplanar points give an empty result.
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
    let mut planes = BTreeSet::new();
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
                let on_plane: Vec<usize> = (0..n)
                    .filter(|&m| (normal.dot(points[m]) - d).abs() <= eps)
                    .collect();
                if !planes.insert(on_plane.clone()) {
                    continue;
                }
                let outward = if above { -normal } else { normal };
                let boundary = face_boundary(points, &on_plane, normal, eps);
                if boundary.len() < 3 {
                    continue;
                }
                for edge in boundary[1..].windows(2) {
                    let [a, b, c] = [boundary[0], edge[0], edge[1]];
                    let face_normal = (points[b] - points[a]).cross(points[c] - points[a]);
                    let triangle = if face_normal.dot(outward) < 0.0 {
                        [a as u32, c as u32, b as u32]
                    } else {
                        [a as u32, b as u32, c as u32]
                    };
                    out.push(triangle);
                }
            }
        }
    }
    out
}

/// Project a supporting face onto its largest normal axis and retain its convex boundary.
fn face_boundary(points: &[Vec3], face: &[usize], normal: Vec3, eps: f32) -> Vec<usize> {
    let project = |index: usize| {
        let p = points[index];
        if normal.x.abs() >= normal.y.abs() && normal.x.abs() >= normal.z.abs() {
            [p.y, p.z]
        } else if normal.y.abs() >= normal.z.abs() {
            [p.x, p.z]
        } else {
            [p.x, p.y]
        }
    };
    let mut ordered = face.to_vec();
    ordered.sort_by(|&a, &b| {
        let a = project(a);
        let b = project(b);
        a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1]))
    });
    ordered.dedup_by(|a, b| project(*a) == project(*b));
    let turn = |a, b, c| {
        let [ax, ay] = project(a);
        let [bx, by] = project(b);
        let [cx, cy] = project(c);
        (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
    };
    let chain = |indices: Vec<usize>| {
        let mut hull = Vec::new();
        for index in indices {
            while hull.len() >= 2
                && turn(hull[hull.len() - 2], hull[hull.len() - 1], index) <= eps * eps
            {
                hull.pop();
            }
            hull.push(index);
        }
        hull.pop();
        hull
    };
    let mut lower = chain(ordered.clone());
    lower.extend(chain(ordered.into_iter().rev().collect()));
    lower
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
    fn box_and_shuffled_hull_faces_support_every_grid_ray() {
        use crate::{Collider, Iso, Layer, MotionType, PhysicsWorld, Shape, SimpleWorld};
        for shape in [
            Shape::Box {
                half_extents: Vec3::new(64.0, 64.0, 2.0),
            },
            Shape::ConvexHull {
                points: unit_cube()
                    .into_iter()
                    .rev()
                    .map(|p| Vec3::new(p.x * 64.0, p.y * 64.0, p.z * 2.0))
                    .collect(),
            },
        ] {
            let mut world = SimpleWorld::new();
            world.add_static(&Collider {
                node: soe_nif::BlockRef(0),
                node_name: "grid floor".into(),
                layer: Layer::GROUND,
                motion: MotionType::Fixed,
                mass: 0.0,
                transform: Iso::IDENTITY,
                shape,
            });
            for x in -7..=7 {
                for y in -7..=7 {
                    let p = Vec3::new(x as f32 * 8.0, y as f32 * 8.0, 20.0);
                    let hit = world
                        .raycast(p, p - Vec3::Z * 40.0)
                        .expect("whole top face supports the ray");
                    assert!((hit.point.z - 2.0).abs() < 0.001, "{p:?}: {hit:?}");
                }
            }
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
