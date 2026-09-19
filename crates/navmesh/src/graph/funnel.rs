// SPDX-License-Identifier: GPL-3.0-or-later

//! String pulling adapted from recastnavigation's DetourNavMeshQuery.cpp,
//! findStraightPath and appendPortals. This is an altered Rust adaptation.
//! Copyright (c) 2009-2010 Mikko Mononen. The retained zlib notice is in
//! crates/navmesh/LICENSE-Detour.txt.
//! Skyrim uses XY for the ground plane. All portal crossings retain Z and
//! triangle ownership, including when several crossings share one corner.

use super::{NavGraph, NodeId};

type Point = [f32; 3];

#[derive(Clone, Copy)]
struct Portal {
    left: Point,
    right: Point,
    enters: NodeId,
}

// Detour's signed area is the negative of the usual XY cross product.
fn area(a: Point, b: Point, c: Point) -> f64 {
    (f64::from(c[0]) - f64::from(a[0])) * (f64::from(b[1]) - f64::from(a[1]))
        - (f64::from(b[0]) - f64::from(a[0])) * (f64::from(c[1]) - f64::from(a[1]))
}

fn same_xy(a: Point, b: Point) -> bool {
    (f64::from(a[0]) - f64::from(b[0])).hypot(f64::from(a[1]) - f64::from(b[1])) < 1e-6
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    std::array::from_fn(|i| (f64::from(a[i]) * (1.0 - t) + f64::from(b[i]) * t) as f32)
}

impl NavGraph {
    pub(super) fn pull_path(
        &self,
        nodes: &[NodeId],
        from: Point,
        to: Point,
    ) -> (Vec<Point>, Vec<NodeId>) {
        let mut points = vec![from];
        let mut owners = vec![nodes[0]];
        let mut portals = Vec::new();
        let mut start = from;
        for pair in nodes.windows(2) {
            let (here, next) = (pair[0], pair[1]);
            let edge = self
                .triangle(here)
                .neighbors
                .iter()
                .position(|n| *n == Some(next))
                .unwrap_or(0);
            if let Some(portal) = self.portal(here, edge, next) {
                portals.push(portal);
            } else {
                let departure = self.meshes[here.mesh as usize].edge_midpoint(here.triangle, edge);
                append_chunk(&portals, start, departure, here, &mut points, &mut owners);
                start = self.centroid(next);
                points.push(start);
                owners.push(next);
                portals.clear();
            }
        }
        append_chunk(
            &portals,
            start,
            to,
            nodes[nodes.len() - 1],
            &mut points,
            &mut owners,
        );
        (points, owners)
    }

    fn portal(&self, from: NodeId, edge: usize, to: NodeId) -> Option<Portal> {
        let source = &self.meshes[from.mesh as usize];
        let triangle = &source.triangles[from.triangle as usize];
        let a = source.vertices[triangle.vertices[edge] as usize];
        let b = source.vertices[triangle.vertices[(edge + 1) % 3] as usize];
        let interior = area(a, b, self.centroid(from));
        if interior * area(a, b, self.centroid(to)) >= 0.0 || interior == 0.0 {
            return None;
        }
        let destination = &self.meshes[to.mesh as usize];
        let vertices = destination.triangles[to.triangle as usize]
            .vertices
            .map(|v| destination.vertices[v as usize]);
        for e in 0..3 {
            if let Some((lo, hi)) = overlap(a, b, vertices[e], vertices[(e + 1) % 3]) {
                let (a, b) = (lerp(a, b, lo), lerp(a, b, hi));
                let (left, right) = if interior < 0.0 { (b, a) } else { (a, b) };
                return Some(Portal {
                    left,
                    right,
                    enters: to,
                });
            }
        }
        None
    }
}

// Unlike Detour's compact link bounds, NAVM links name a triangle. Intersect
// the collinear edges so a partial shared seam does not expose its whole edge.
fn overlap(a: Point, b: Point, c: Point, d: Point) -> Option<(f64, f64)> {
    let dx = f64::from(b[0]) - f64::from(a[0]);
    let dy = f64::from(b[1]) - f64::from(a[1]);
    let length2 = dx * dx + dy * dy;
    if length2 < 1e-12 {
        return None;
    }
    let length = length2.sqrt();
    if area(a, b, c).abs() > 0.01 * length || area(a, b, d).abs() > 0.01 * length {
        return None;
    }
    let parameter = |p: Point| {
        ((f64::from(p[0]) - f64::from(a[0])) * dx + (f64::from(p[1]) - f64::from(a[1])) * dy)
            / length2
    };
    let (tc, td) = (parameter(c), parameter(d));
    if (td - tc).abs() < 1e-12 {
        return None;
    }
    let lo = tc.min(td).max(0.0);
    let hi = tc.max(td).min(1.0);
    if lo > hi {
        return None;
    }
    for t in [lo, hi] {
        let p = lerp(a, b, t);
        let q = lerp(c, d, (t - tc) / (td - tc));
        if (p[2] - q[2]).abs() > 0.01 {
            return None;
        }
    }
    Some((lo, hi))
}

fn corners(portals: &[Portal], from: Point, to: Point) -> Option<Vec<(usize, Point)>> {
    let mut corners = vec![(0, from)];
    let (mut apex, mut left, mut right) = (from, from, from);
    let (mut apex_index, mut left_index, mut right_index) = (0, 0, 0);
    let mut i = 1;
    while i <= portals.len() + 1 {
        let (next_left, next_right) = portals.get(i - 1).map_or((to, to), |p| (p.left, p.right));
        if area(apex, right, next_right) <= 0.0 {
            if same_xy(apex, right) || area(apex, left, next_right) > 0.0 {
                right = next_right;
                right_index = i;
            } else {
                if left_index <= apex_index {
                    return None;
                }
                corners.push((left_index, left));
                apex = left;
                apex_index = left_index;
                left = apex;
                right = apex;
                left_index = apex_index;
                right_index = apex_index;
                i = apex_index + 1;
                continue;
            }
        }
        if area(apex, left, next_left) >= 0.0 {
            if same_xy(apex, left) || area(apex, right, next_left) < 0.0 {
                left = next_left;
                left_index = i;
            } else {
                if right_index <= apex_index {
                    return None;
                }
                corners.push((right_index, right));
                apex = right;
                apex_index = right_index;
                left = apex;
                right = apex;
                left_index = apex_index;
                right_index = apex_index;
                i = apex_index + 1;
                continue;
            }
        }
        i += 1;
    }
    corners.push((portals.len() + 1, to));
    Some(corners)
}

fn crossing(start: Point, end: Point, portal: Portal, minimum: f64) -> Option<(Point, f64)> {
    let a = portal.left;
    let b = portal.right;
    let rx = f64::from(end[0]) - f64::from(start[0]);
    let ry = f64::from(end[1]) - f64::from(start[1]);
    let sx = f64::from(b[0]) - f64::from(a[0]);
    let sy = f64::from(b[1]) - f64::from(a[1]);
    let qx = f64::from(a[0]) - f64::from(start[0]);
    let qy = f64::from(a[1]) - f64::from(start[1]);
    let denominator = rx * sy - ry * sx;
    if denominator.abs() > 1e-12 {
        let t = (qx * sy - qy * sx) / denominator;
        let u = (qx * ry - qy * rx) / denominator;
        if !(minimum - 1e-5..=1.00001).contains(&t) || !(-1e-5..=1.00001).contains(&u) {
            return None;
        }
        return Some((lerp(a, b, u.clamp(0.0, 1.0)), t.clamp(minimum, 1.0)));
    }
    let segment_length2 = rx * rx + ry * ry;
    let portal_length2 = sx * sx + sy * sy;
    if segment_length2 < 1e-12 {
        let u = if portal_length2 < 1e-12 {
            0.0
        } else {
            (-qx * sx - qy * sy) / portal_length2
        };
        let point = lerp(a, b, u.clamp(0.0, 1.0));
        return same_xy(point, start).then_some((point, minimum));
    }
    if area(start, end, a).abs() > 0.01 * segment_length2.sqrt()
        || area(start, end, b).abs() > 0.01 * segment_length2.sqrt()
    {
        return None;
    }
    let ta = (qx * rx + qy * ry) / segment_length2;
    let tb = ((qx + sx) * rx + (qy + sy) * ry) / segment_length2;
    let lo = ta.min(tb).max(minimum);
    let hi = ta.max(tb).min(1.0);
    if lo > hi + 1e-5 {
        return None;
    }
    let t = lo.clamp(minimum, 1.0);
    let u = if portal_length2 < 1e-12 {
        0.0
    } else {
        ((rx * t - qx) * sx + (ry * t - qy) * sy) / portal_length2
    };
    Some((lerp(a, b, u.clamp(0.0, 1.0)), t))
}

fn append_chunk(
    portals: &[Portal],
    from: Point,
    to: Point,
    end_node: NodeId,
    points: &mut Vec<Point>,
    owners: &mut Vec<NodeId>,
) {
    let pulled = corners(portals, from, to).and_then(|corners| {
        let mut crossings = Vec::with_capacity(portals.len());
        for pair in corners.windows(2) {
            let ((start_index, start), (end_index, end)) = (pair[0], pair[1]);
            let mut parameter = 0.0;
            for portal in portals.iter().take(end_index).skip(start_index) {
                let (point, next) = crossing(start, end, *portal, parameter)?;
                crossings.push(point);
                parameter = next;
            }
        }
        (crossings.len() == portals.len()).then_some(crossings)
    });
    for (i, portal) in portals.iter().enumerate() {
        points.push(
            pulled
                .as_ref()
                .map_or_else(|| lerp(portal.left, portal.right, 0.5), |p| p[i]),
        );
        owners.push(portal.enters);
    }
    points.push(to);
    owners.push(end_node);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collinear_crossings_respect_segment_bounds_and_previous_crossing() {
        let portal = |a, b| Portal {
            left: [a, 0.0, 0.0],
            right: [b, 0.0, 0.0],
            enters: NodeId::new(0, 0),
        };
        let (start, end) = ([0.0; 3], [3.0, 0.0, 0.0]);
        assert_eq!(
            crossing(start, end, portal(2.0, 1.0), 0.0).unwrap().0,
            [1.0, 0.0, 0.0]
        );
        assert_eq!(
            crossing(start, end, portal(1.0, 2.0), 0.5).unwrap().0,
            [1.5, 0.0, 0.0]
        );
        assert!(crossing(start, end, portal(4.0, 5.0), 0.0).is_none());
        assert!(crossing(start, end, portal(0.0, 1.0), 0.5).is_none());
        assert!(crossing(start, start, portal(1.0, 2.0), 0.0).is_none());
    }
}
