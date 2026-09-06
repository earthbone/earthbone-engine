// SPDX-License-Identifier: GPL-3.0-or-later

//! The collision world. [`PhysicsWorld`] is the seam a real engine (Jolt,
//! through its C bindings) fills in later. [`SimpleWorld`] is the plain
//! Rust implementation that exists today: statics only, a capsule
//! character controller resolved by closest-point pushes, and rays.
//!
//! What it does not do: rigid body simulation, constraints, ragdolls,
//! continuous collision, or friction. Those are exactly the parts Jolt
//! brings, and the trait is shaped so they slot in without changing how
//! shapes are produced or how the game layer moves a character.

use std::collections::HashMap;

use crate::bhk::Layer;
use crate::hull;
use crate::math::{Aabb, Iso, Vec3};
use crate::shape::{Collider, Shape};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyId(pub u32);

/// Standing gravity in game units per second squared: 9.81 metres times
/// the Havok scale. Skyrim's `fGravity` setting is close to this.
pub const GRAVITY: f32 = 9.81 * crate::bhk::UNITS_PER_METER;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterSettings {
    pub radius: f32,
    /// Total height including both caps.
    pub height: f32,
    /// Contact normals with a Z above this count as ground.
    pub max_slope_cos: f32,
    pub gravity: f32,
}

impl Default for CharacterSettings {
    /// Roughly the vanilla player: 128 units tall, a 32 unit capsule.
    fn default() -> Self {
        CharacterSettings {
            radius: 32.0,
            height: 128.0,
            max_slope_cos: 0.7,
            gravity: GRAVITY,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Character {
    pub settings: CharacterSettings,
    /// The bottom of the capsule, where the feet are.
    pub position: Vec3,
    pub velocity: Vec3,
    pub on_ground: bool,
    /// What the game layer wants this frame, applied horizontally.
    pub desired_velocity: Vec3,
}

impl Character {
    /// The capsule's axis from the lower cap centre to the upper.
    pub fn axis(&self) -> (Vec3, Vec3) {
        let r = self.settings.radius;
        let h = (self.settings.height - 2.0 * r).max(0.0);
        let a = self.position + Vec3::Z * r;
        (a, a + Vec3::Z * h)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub body: BodyId,
    pub point: Vec3,
    pub normal: Vec3,
    /// Fraction along the ray, 0 at the start and 1 at the end.
    pub fraction: f32,
}

pub trait PhysicsWorld {
    /// Add a collider as an immovable body. The collider's transform is
    /// its world placement, so the same NIF shape can be added once per
    /// placed reference with a different transform.
    fn add_static(&mut self, collider: &Collider) -> BodyId;
    fn remove(&mut self, id: BodyId);
    fn create_character(&mut self, settings: CharacterSettings, position: Vec3) -> BodyId;
    fn character(&self, id: BodyId) -> Option<&Character>;
    /// Set the horizontal velocity the character tries to keep this step.
    fn move_character(&mut self, id: BodyId, desired: Vec3);
    fn step(&mut self, dt: f32);
    fn raycast(&self, from: Vec3, to: Vec3) -> Option<RayHit>;
}

/// A world-space primitive. Boxes and hulls become triangles on the way
/// in so the character only needs three closest-point routines.
#[derive(Clone, Copy, Debug)]
enum Prim {
    Tri(Vec3, Vec3, Vec3),
    Sphere(Vec3, f32),
    Capsule(Vec3, Vec3, f32),
}

impl Prim {
    fn aabb(&self) -> Aabb {
        match *self {
            Prim::Tri(a, b, c) => Aabb::of_points([a, b, c]),
            Prim::Sphere(c, r) => Aabb::of_points([c]).grow(r),
            Prim::Capsule(a, b, r) => Aabb::of_points([a, b]).grow(r),
        }
    }
}

struct StaticBody {
    id: BodyId,
    layer: Layer,
    prims: Vec<Prim>,
    aabb: Aabb,
}

const CELL: f32 = 256.0;

#[derive(Default)]
pub struct SimpleWorld {
    next: u32,
    statics: Vec<StaticBody>,
    characters: HashMap<BodyId, Character>,
    /// Grid cell to (static index, prim index). Rebuilt when `dirty`.
    grid: HashMap<(i32, i32, i32), Vec<(usize, usize)>>,
    dirty: bool,
}

impl SimpleWorld {
    pub fn new() -> SimpleWorld {
        SimpleWorld::default()
    }

    pub fn static_count(&self) -> usize {
        self.statics.len()
    }

    pub fn primitive_count(&self) -> usize {
        self.statics.iter().map(|s| s.prims.len()).sum()
    }

    fn cells(aabb: &Aabb) -> impl Iterator<Item = (i32, i32, i32)> {
        let lo = (
            (aabb.min.x / CELL).floor() as i32,
            (aabb.min.y / CELL).floor() as i32,
            (aabb.min.z / CELL).floor() as i32,
        );
        let hi = (
            (aabb.max.x / CELL).floor() as i32,
            (aabb.max.y / CELL).floor() as i32,
            (aabb.max.z / CELL).floor() as i32,
        );
        (lo.0..=hi.0).flat_map(move |x| {
            (lo.1..=hi.1).flat_map(move |y| (lo.2..=hi.2).map(move |z| (x, y, z)))
        })
    }

    fn rebuild(&mut self) {
        self.grid.clear();
        for (si, s) in self.statics.iter().enumerate() {
            for (pi, p) in s.prims.iter().enumerate() {
                let b = p.aabb();
                if b.is_empty() || !b.min.is_finite() || !b.max.is_finite() {
                    continue;
                }
                for c in Self::cells(&b) {
                    self.grid.entry(c).or_default().push((si, pi));
                }
            }
        }
        self.dirty = false;
    }

    /// Primitives whose cells overlap `aabb`, each once.
    fn candidates(&self, aabb: &Aabb) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for c in Self::cells(aabb) {
            if let Some(list) = self.grid.get(&c) {
                out.extend_from_slice(list);
            }
        }
        out.sort_unstable();
        out.dedup();
        out.retain(|&(si, pi)| self.statics[si].prims[pi].aabb().overlaps(aabb));
        out
    }

    fn step_character(&mut self, id: BodyId, dt: f32) {
        let Some(mut ch) = self.characters.get(&id).copied() else {
            return;
        };
        let s = ch.settings;
        ch.velocity.x = ch.desired_velocity.x;
        ch.velocity.y = ch.desired_velocity.y;
        ch.velocity.z -= s.gravity * dt;
        // Cap the fall so a long step cannot tunnel through a floor thinner
        // than the capsule.
        let max_drop = s.radius / dt.max(1e-6);
        if ch.velocity.z < -max_drop {
            ch.velocity.z = -max_drop;
        }
        ch.position = ch.position + ch.velocity * dt;
        ch.on_ground = false;
        for _ in 0..6 {
            let (a, b) = ch.axis();
            let query = Aabb::of_points([a, b]).grow(s.radius + 1.0);
            let mut moved = false;
            for (si, pi) in self.candidates(&query) {
                if !self.statics[si].layer.blocks_characters() {
                    continue;
                }
                let (on_prim, on_seg) = closest_prim_segment(&self.statics[si].prims[pi], a, b);
                let d = on_seg - on_prim;
                let dist = d.length();
                if dist >= s.radius {
                    continue;
                }
                let normal = if dist > 1e-4 {
                    d * (1.0 / dist)
                } else {
                    prim_normal_toward(&self.statics[si].prims[pi], on_seg)
                };
                let push = s.radius - dist;
                ch.position = ch.position + normal * push;
                let vn = ch.velocity.dot(normal);
                if vn < 0.0 {
                    ch.velocity = ch.velocity - normal * vn;
                }
                if normal.z >= s.max_slope_cos {
                    ch.on_ground = true;
                }
                moved = true;
                // The axis moved, so re-query before touching more.
                break;
            }
            if !moved {
                break;
            }
        }
        if ch.on_ground && ch.velocity.z < 0.0 {
            ch.velocity.z = 0.0;
        }
        self.characters.insert(id, ch);
    }
}

impl PhysicsWorld for SimpleWorld {
    fn add_static(&mut self, collider: &Collider) -> BodyId {
        let id = BodyId(self.next);
        self.next += 1;
        let mut prims = Vec::new();
        flatten(&collider.shape, &collider.transform, &mut prims);
        let aabb = prims
            .iter()
            .map(Prim::aabb)
            .fold(Aabb::empty(), |a, b| a.union(&b));
        self.statics.push(StaticBody {
            id,
            layer: collider.layer,
            prims,
            aabb,
        });
        self.dirty = true;
        id
    }

    fn remove(&mut self, id: BodyId) {
        if self.characters.remove(&id).is_some() {
            return;
        }
        let before = self.statics.len();
        self.statics.retain(|s| s.id != id);
        if self.statics.len() != before {
            self.dirty = true;
        }
    }

    fn create_character(&mut self, settings: CharacterSettings, position: Vec3) -> BodyId {
        let id = BodyId(self.next);
        self.next += 1;
        self.characters.insert(
            id,
            Character {
                settings,
                position,
                velocity: Vec3::ZERO,
                on_ground: false,
                desired_velocity: Vec3::ZERO,
            },
        );
        id
    }

    fn character(&self, id: BodyId) -> Option<&Character> {
        self.characters.get(&id)
    }

    fn move_character(&mut self, id: BodyId, desired: Vec3) {
        if let Some(c) = self.characters.get_mut(&id) {
            c.desired_velocity = desired;
        }
    }

    fn step(&mut self, dt: f32) {
        if self.dirty {
            self.rebuild();
        }
        let ids: Vec<BodyId> = self.characters.keys().copied().collect();
        for id in ids {
            self.step_character(id, dt);
        }
    }

    fn raycast(&self, from: Vec3, to: Vec3) -> Option<RayHit> {
        let query = Aabb::of_points([from, to]);
        let mut best: Option<RayHit> = None;
        let candidates = if self.dirty {
            // A world that was never stepped has no grid yet, fall back to
            // the body boxes.
            let mut v = Vec::new();
            for (si, s) in self.statics.iter().enumerate() {
                if s.aabb.overlaps(&query) {
                    for pi in 0..s.prims.len() {
                        v.push((si, pi));
                    }
                }
            }
            v
        } else {
            self.candidates(&query)
        };
        for (si, pi) in candidates {
            if let Some((t, normal)) = ray_prim(&self.statics[si].prims[pi], from, to) {
                if best.is_none_or(|b| t < b.fraction) {
                    best = Some(RayHit {
                        body: self.statics[si].id,
                        point: from + (to - from) * t,
                        normal,
                        fraction: t,
                    });
                }
            }
        }
        best
    }
}

/// World-space primitives for a shape placed by `t`.
fn flatten(shape: &Shape, t: &Iso, out: &mut Vec<Prim>) {
    match shape {
        Shape::Box { half_extents } => {
            let h = *half_extents;
            let corners: Vec<Vec3> = (0..8)
                .map(|i| {
                    t.apply(Vec3::new(
                        if i & 1 == 0 { -h.x } else { h.x },
                        if i & 2 == 0 { -h.y } else { h.y },
                        if i & 4 == 0 { -h.z } else { h.z },
                    ))
                })
                .collect();
            for tri in hull::triangles(&corners) {
                out.push(Prim::Tri(
                    corners[tri[0] as usize],
                    corners[tri[1] as usize],
                    corners[tri[2] as usize],
                ));
            }
        }
        Shape::Sphere { radius } => out.push(Prim::Sphere(t.apply(Vec3::ZERO), radius * t.scale)),
        Shape::Capsule { a, b, radius } => {
            out.push(Prim::Capsule(t.apply(*a), t.apply(*b), radius * t.scale))
        }
        Shape::ConvexHull { points } => {
            let pts: Vec<Vec3> = points.iter().map(|p| t.apply(*p)).collect();
            for tri in hull::triangles(&pts) {
                out.push(Prim::Tri(
                    pts[tri[0] as usize],
                    pts[tri[1] as usize],
                    pts[tri[2] as usize],
                ));
            }
        }
        Shape::TriangleMesh {
            vertices,
            triangles,
        } => {
            let pts: Vec<Vec3> = vertices.iter().map(|p| t.apply(*p)).collect();
            for tri in triangles {
                if let (Some(&a), Some(&b), Some(&c)) = (
                    pts.get(tri[0] as usize),
                    pts.get(tri[1] as usize),
                    pts.get(tri[2] as usize),
                ) {
                    out.push(Prim::Tri(a, b, c));
                }
            }
        }
        Shape::Compound(children) => {
            for (ct, c) in children {
                flatten(c, &t.then(ct), out);
            }
        }
    }
}

/// Closest points between a primitive's surface and the segment `a b`:
/// `(on the primitive, on the segment)`.
fn closest_prim_segment(p: &Prim, a: Vec3, b: Vec3) -> (Vec3, Vec3) {
    match *p {
        Prim::Tri(t0, t1, t2) => closest_segment_triangle(a, b, t0, t1, t2),
        Prim::Sphere(c, r) => {
            let on_seg = closest_point_segment(c, a, b);
            let dir = (on_seg - c).normalized();
            (c + dir * r, on_seg)
        }
        Prim::Capsule(c0, c1, r) => {
            let (on_axis, on_seg) = closest_segment_segment(c0, c1, a, b);
            let dir = (on_seg - on_axis).normalized();
            (on_axis + dir * r, on_seg)
        }
    }
}

/// A push direction when the segment touches the primitive exactly: the
/// primitive's normal on the side `toward` lies.
fn prim_normal_toward(p: &Prim, toward: Vec3) -> Vec3 {
    match *p {
        Prim::Tri(t0, t1, t2) => {
            let n = (t1 - t0).cross(t2 - t0).normalized();
            if n.dot(toward - t0) < 0.0 {
                -n
            } else {
                n
            }
        }
        Prim::Sphere(c, _) => (toward - c).normalized(),
        Prim::Capsule(c0, c1, _) => (toward - closest_point_segment(toward, c0, c1)).normalized(),
    }
    .normalized()
}

pub(crate) fn closest_point_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = b - a;
    let l2 = ab.length_squared();
    if l2 < 1e-12 {
        return a;
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    a + ab * t
}

/// Ericson, Real-Time Collision Detection, 5.1.5.
pub(crate) fn closest_point_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return a + ab * v;
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return a + ac * w;
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return b + (c - b) * w;
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    a + ab * v + ac * w
}

/// Ericson 5.1.9, closest points of two segments: `(on p, on q)`.
pub(crate) fn closest_segment_segment(p0: Vec3, p1: Vec3, q0: Vec3, q1: Vec3) -> (Vec3, Vec3) {
    let d1 = p1 - p0;
    let d2 = q1 - q0;
    let r = p0 - q0;
    let a = d1.length_squared();
    let e = d2.length_squared();
    let f = d2.dot(r);
    let (s, t);
    if a < 1e-12 && e < 1e-12 {
        return (p0, q0);
    }
    if a < 1e-12 {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e < 1e-12 {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s1 = if denom > 1e-12 {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut t1 = (b * s1 + f) / e;
            if t1 < 0.0 {
                t1 = 0.0;
                s1 = (-c / a).clamp(0.0, 1.0);
            } else if t1 > 1.0 {
                t1 = 1.0;
                s1 = ((b - c) / a).clamp(0.0, 1.0);
            }
            s = s1;
            t = t1;
        }
    }
    (p0 + d1 * s, q0 + d2 * t)
}

/// Closest points between segment `a b` and triangle `t0 t1 t2`:
/// `(on the triangle, on the segment)`. A segment that crosses the
/// triangle's interior returns the crossing point for both.
pub(crate) fn closest_segment_triangle(
    a: Vec3,
    b: Vec3,
    t0: Vec3,
    t1: Vec3,
    t2: Vec3,
) -> (Vec3, Vec3) {
    if let Some(t) = ray_triangle(a, b, t0, t1, t2) {
        let p = a + (b - a) * t;
        return (p, p);
    }
    let mut best = {
        let q = closest_point_triangle(a, t0, t1, t2);
        (q, a, (q - a).length_squared())
    };
    let q = closest_point_triangle(b, t0, t1, t2);
    let d = (q - b).length_squared();
    if d < best.2 {
        best = (q, b, d);
    }
    for (e0, e1) in [(t0, t1), (t1, t2), (t2, t0)] {
        let (on_edge, on_seg) = closest_segment_segment(e0, e1, a, b);
        let d = (on_seg - on_edge).length_squared();
        if d < best.2 {
            best = (on_edge, on_seg, d);
        }
    }
    (best.0, best.1)
}

/// Möller and Trumbore. Fraction along `from to` at which the segment
/// crosses the triangle, either side.
fn ray_triangle(from: Vec3, to: Vec3, t0: Vec3, t1: Vec3, t2: Vec3) -> Option<f32> {
    let dir = to - from;
    let e1 = t1 - t0;
    let e2 = t2 - t0;
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = from - t0;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    if (0.0..=1.0).contains(&t) {
        Some(t)
    } else {
        None
    }
}

fn ray_sphere(from: Vec3, to: Vec3, c: Vec3, r: f32) -> Option<f32> {
    let d = to - from;
    let m = from - c;
    let a = d.length_squared();
    if a < 1e-12 {
        return None;
    }
    let b = m.dot(d);
    let cc = m.length_squared() - r * r;
    if cc > 0.0 && b > 0.0 {
        return None;
    }
    let disc = b * b - a * cc;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / a;
    if (0.0..=1.0).contains(&t) {
        Some(t)
    } else {
        None
    }
}

/// First hit of the segment on a primitive, with the surface normal there.
fn ray_prim(p: &Prim, from: Vec3, to: Vec3) -> Option<(f32, Vec3)> {
    match *p {
        Prim::Tri(t0, t1, t2) => ray_triangle(from, to, t0, t1, t2).map(|t| {
            let n = (t1 - t0).cross(t2 - t0).normalized();
            let n = if n.dot(to - from) > 0.0 { -n } else { n };
            (t, n)
        }),
        Prim::Sphere(c, r) => ray_sphere(from, to, c, r).map(|t| {
            let hit = from + (to - from) * t;
            (t, (hit - c).normalized())
        }),
        Prim::Capsule(c0, c1, r) => {
            // The capsule as the union of its two cap spheres and the
            // cylinder between them, sampled by the nearest of the three.
            let mut best: Option<(f32, Vec3)> = None;
            for c in [c0, c1] {
                if let Some(t) = ray_sphere(from, to, c, r) {
                    let hit = from + (to - from) * t;
                    if best.is_none_or(|b| t < b.0) {
                        best = Some((t, (hit - c).normalized()));
                    }
                }
            }
            if let Some(t) = ray_cylinder(from, to, c0, c1, r) {
                let hit = from + (to - from) * t;
                let on_axis = closest_point_segment(hit, c0, c1);
                if best.is_none_or(|b| t < b.0) {
                    best = Some((t, (hit - on_axis).normalized()));
                }
            }
            best
        }
    }
}

/// Segment against the open cylinder around `c0 c1`.
fn ray_cylinder(from: Vec3, to: Vec3, c0: Vec3, c1: Vec3, r: f32) -> Option<f32> {
    let axis = c1 - c0;
    let al = axis.length();
    if al < 1e-6 {
        return None;
    }
    let u = axis * (1.0 / al);
    let d = to - from;
    let m = from - c0;
    let dp = d - u * d.dot(u);
    let mp = m - u * m.dot(u);
    let a = dp.length_squared();
    if a < 1e-12 {
        return None;
    }
    let b = dp.dot(mp);
    let c = mp.length_squared() - r * r;
    let disc = b * b - a * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / a;
    if !(0.0..=1.0).contains(&t) {
        return None;
    }
    let hit = from + d * t;
    let along = (hit - c0).dot(u);
    if (0.0..=al).contains(&along) {
        Some(t)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_point_on_triangle_interior_edge_and_vertex() {
        let (a, b, c) = (
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        );
        let q = closest_point_triangle(Vec3::new(0.5, 0.5, 3.0), a, b, c);
        assert!((q - Vec3::new(0.5, 0.5, 0.0)).length() < 1e-6);
        let q = closest_point_triangle(Vec3::new(1.0, -1.0, 0.0), a, b, c);
        assert!((q - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-6);
        let q = closest_point_triangle(Vec3::new(-1.0, -1.0, 0.0), a, b, c);
        assert!((q - a).length() < 1e-6);
    }

    #[test]
    fn segment_segment_distance_of_skew_lines() {
        let (p, q) = closest_segment_segment(
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, -1.0, 1.0),
            Vec3::new(0.0, 1.0, 1.0),
        );
        assert!((p - Vec3::ZERO).length() < 1e-6);
        assert!((q - Vec3::new(0.0, 0.0, 1.0)).length() < 1e-6);
    }

    #[test]
    fn segment_crossing_a_triangle_has_zero_distance() {
        let (on_tri, on_seg) = closest_segment_triangle(
            Vec3::new(0.5, 0.5, -1.0),
            Vec3::new(0.5, 0.5, 1.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        );
        assert!((on_tri - on_seg).length() < 1e-6);
        assert!((on_tri - Vec3::new(0.5, 0.5, 0.0)).length() < 1e-6);
    }

    #[test]
    fn ray_hits_sphere_and_capsule_side() {
        let t = ray_sphere(
            Vec3::new(-5.0, 0.0, 0.0),
            Vec3::new(5.0, 0.0, 0.0),
            Vec3::ZERO,
            1.0,
        )
        .unwrap();
        assert!((t - 0.4).abs() < 1e-5);
        let cap = Prim::Capsule(Vec3::new(0.0, 0.0, -2.0), Vec3::new(0.0, 0.0, 2.0), 1.0);
        let (t, n) = ray_prim(&cap, Vec3::new(-5.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0)).unwrap();
        assert!((t - 0.4).abs() < 1e-5);
        assert!((n - Vec3::new(-1.0, 0.0, 0.0)).length() < 1e-5);
    }
}
