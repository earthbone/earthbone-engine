// SPDX-License-Identifier: GPL-3.0-or-later

//! The rendered path's collision, built on Godot's own physics server.
//!
//! Godot 4.4 and later bundle Jolt as a physics module, selectable through the
//! `physics/3d/physics_engine` project setting, so the engine links no physics
//! library of its own. This module hands a cell's Havok collision to
//! `PhysicsServer3D` the same way [`crate::godot_layer`] hands its meshes to
//! `RenderingServer`: server side, by resource id, with no node per body.
//!
//! The headless path is a different implementation of the same idea and it
//! stays. `docs/physics.md` records why, in short: the authoritative
//! simulation has to run in our process where the per tick state hash can see
//! it and a test can reproduce it from a seed, and Godot's server cannot offer
//! that. So this is the collision the renderer and the player see, while
//! [`soe_physics::SimpleWorld`] is what the simulation and the tests step.

use godot::classes::PhysicsServer3D;
use godot::prelude::*;

use soe_physics::{Collider, Iso, Shape, Vec3};

use crate::units::SkyrimPos;

/// What went into the space, so a caller can report it the way the cell
/// streamer reports its surfaces.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GodotCollisionStats {
    pub bodies: usize,
    pub shapes: usize,
    pub triangles: usize,
    /// Shapes the converter has no Godot equivalent for. Compounds are
    /// flattened rather than skipped, so this counts only genuinely unknown
    /// geometry.
    pub skipped: usize,
}

/// The server side handles for one cell's collision, so a caller can free
/// them when the cell unloads. Dropping this does not free anything, the
/// server owns the ids, call [`CellBodies::free`].
#[derive(Debug, Default)]
pub struct CellBodies {
    pub bodies: Vec<Rid>,
    pub shapes: Vec<Rid>,
    pub stats: GodotCollisionStats,
}

impl CellBodies {
    /// Releases every id back to the server. A cell that unloads must call
    /// this, the server does not reclaim ids on its own.
    pub fn free(&mut self) {
        let mut server = PhysicsServer3D::singleton();
        for body in self.bodies.drain(..) {
            server.free_rid(body);
        }
        for shape in self.shapes.drain(..) {
            server.free_rid(shape);
        }
        self.stats = GodotCollisionStats::default();
    }
}

/// Adds every static collider to `space` as an immovable body.
///
/// Transforms are baked into the geometry by the shape layer already, so each
/// body sits at the origin and carries world space vertices. That matches what
/// the mesh path does and keeps the two from disagreeing about placement.
pub fn add_statics(space: Rid, colliders: &[Collider]) -> CellBodies {
    let _span = soe_trace::Span::new("host.godot_physics", "host");
    let mut server = PhysicsServer3D::singleton();
    let mut out = CellBodies::default();

    for collider in colliders.iter().filter(|c| c.is_static()) {
        let mut prims = Vec::new();
        flatten(&collider.shape, &collider.transform, &mut prims);
        if prims.is_empty() {
            out.stats.skipped += 1;
            continue;
        }
        let body = server.body_create();
        server.body_set_mode(body, godot::classes::physics_server_3d::BodyMode::STATIC);
        server.body_set_space(body, space);
        for prim in prims {
            let shape = match prim {
                GodotPrim::Trimesh(faces) => {
                    out.stats.triangles += faces.len() / 3;
                    let rid = server.concave_polygon_shape_create();
                    let mut data: Dictionary<Variant, Variant> = Dictionary::new();
                    let packed: PackedVector3Array = faces.into_iter().collect();
                    data.set("faces", &packed);
                    data.set("backface_collision", true);
                    server.shape_set_data(rid, &data.to_variant());
                    rid
                }
                GodotPrim::Sphere(radius) => {
                    let rid = server.sphere_shape_create();
                    server.shape_set_data(rid, &radius.to_variant());
                    rid
                }
                GodotPrim::Capsule { height, radius } => {
                    let rid = server.capsule_shape_create();
                    let mut data: Dictionary<Variant, Variant> = Dictionary::new();
                    data.set("height", height);
                    data.set("radius", radius);
                    server.shape_set_data(rid, &data.to_variant());
                    rid
                }
            };
            server.body_add_shape(body, shape);
            out.shapes.push(shape);
            out.stats.shapes += 1;
        }
        out.bodies.push(body);
        out.stats.bodies += 1;
    }
    out
}

/// A Godot shape the converter can build.
enum GodotPrim {
    /// World space triangle corners, three per face.
    Trimesh(Vec<Vector3>),
    Sphere(f32),
    /// Godot's capsule stands on its local Y axis, height is the total
    /// including both caps.
    Capsule {
        height: f32,
        radius: f32,
    },
}

/// Turns one collision shape into Godot shapes, world space, transform baked.
///
/// Boxes and hulls become triangle soup rather than Godot boxes and hulls,
/// because their transform can carry a rotation Godot's box shape has no place
/// for, and a static body never needs the cheaper representation.
fn flatten(shape: &Shape, t: &Iso, out: &mut Vec<GodotPrim>) {
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
            push_hull(&corners, out);
        }
        Shape::ConvexHull { points } => {
            let pts: Vec<Vec3> = points.iter().map(|p| t.apply(*p)).collect();
            push_hull(&pts, out);
        }
        Shape::Sphere { radius } => {
            // The server places the shape, and a sphere's centre is the body's
            // origin, so a translated sphere needs its own body. Bake it as a
            // one triangle-free primitive at the transform instead: the caller
            // sets no per shape transform, so approximate with a hull.
            let c = t.apply(Vec3::ZERO);
            if c.length() < 1e-3 {
                out.push(GodotPrim::Sphere(radius * t.scale));
            } else {
                push_sphere_hull(c, radius * t.scale, out);
            }
        }
        Shape::Capsule { a, b, radius } => {
            let (a, b) = (t.apply(*a), t.apply(*b));
            let axis = b - a;
            let length = axis.length();
            let r = radius * t.scale;
            // Only an upright capsule at the origin maps to Godot's, anything
            // else becomes geometry so the placement survives.
            if a.x.abs() < 1e-3 && a.y.abs() < 1e-3 && axis.x.abs() < 1e-3 && axis.y.abs() < 1e-3 {
                out.push(GodotPrim::Capsule {
                    height: length + 2.0 * r,
                    radius: r,
                });
            } else {
                push_sphere_hull(a, r, out);
                push_sphere_hull(b, r, out);
            }
        }
        Shape::TriangleMesh {
            vertices,
            triangles,
        } => {
            let pts: Vec<Vec3> = vertices.iter().map(|p| t.apply(*p)).collect();
            let mut faces = Vec::with_capacity(triangles.len() * 3);
            for tri in triangles {
                if let (Some(&a), Some(&b), Some(&c)) = (
                    pts.get(tri[0] as usize),
                    pts.get(tri[1] as usize),
                    pts.get(tri[2] as usize),
                ) {
                    faces.push(godot_point(a));
                    faces.push(godot_point(b));
                    faces.push(godot_point(c));
                }
            }
            if !faces.is_empty() {
                out.push(GodotPrim::Trimesh(faces));
            }
        }
        Shape::Compound(children) => {
            for (child_t, child) in children {
                flatten(child, &t.then(child_t), out);
            }
        }
    }
}

/// A point cloud's convex faces as triangle soup.
fn push_hull(points: &[Vec3], out: &mut Vec<GodotPrim>) {
    let mut faces = Vec::new();
    for tri in soe_physics::hull::triangles(points) {
        for i in tri {
            faces.push(godot_point(points[i as usize]));
        }
    }
    if !faces.is_empty() {
        out.push(GodotPrim::Trimesh(faces));
    }
}

/// An axis aligned box standing in for a sphere the server cannot place. Six
/// quads is enough for static collision a character brushes past.
fn push_sphere_hull(centre: Vec3, radius: f32, out: &mut Vec<GodotPrim>) {
    let corners: Vec<Vec3> = (0..8)
        .map(|i| {
            centre
                + Vec3::new(
                    if i & 1 == 0 { -radius } else { radius },
                    if i & 2 == 0 { -radius } else { radius },
                    if i & 4 == 0 { -radius } else { radius },
                )
        })
        .collect();
    push_hull(&corners, out);
}

/// One game unit point in Godot world space, metres and Godot's axes.
fn godot_point(v: Vec3) -> Vector3 {
    let g = SkyrimPos::new(v.x, v.y, v.z).to_godot();
    Vector3::new(g.x, g.y, g.z)
}
