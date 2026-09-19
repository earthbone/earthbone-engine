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

use crate::units::{SkyrimPos, METRES_PER_UNIT};

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
/// Native primitives carry a per-shape transform. Meshes and convex point
/// clouds carry world-space coordinates. Bodies remain at the origin.
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
            let mut transform = Transform3D::IDENTITY;
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
                GodotPrim::Box {
                    half_extents,
                    placement,
                } => {
                    transform = placement;
                    let rid = server.box_shape_create();
                    server.shape_set_data(rid, &half_extents.to_variant());
                    rid
                }
                GodotPrim::Convex(points) => {
                    let rid = server.convex_polygon_shape_create();
                    let packed: PackedVector3Array = points.into_iter().collect();
                    server.shape_set_data(rid, &packed.to_variant());
                    rid
                }
                GodotPrim::Sphere { radius, centre } => {
                    transform.origin = centre;
                    let rid = server.sphere_shape_create();
                    server.shape_set_data(rid, &radius.to_variant());
                    rid
                }
                GodotPrim::Capsule {
                    height,
                    radius,
                    placement,
                } => {
                    transform = placement;
                    let rid = server.capsule_shape_create();
                    let mut data: Dictionary<Variant, Variant> = Dictionary::new();
                    data.set("height", height);
                    data.set("radius", radius);
                    server.shape_set_data(rid, &data.to_variant());
                    rid
                }
            };
            server
                .body_add_shape_ex(body, shape)
                .transform(transform)
                .done();
            out.shapes.push(shape);
            out.stats.shapes += 1;
        }
        out.bodies.push(body);
        out.stats.bodies += 1;
    }
    out
}

/// Primitive dimensions are in metres, following PhysicsServer3D's shape data.
#[derive(Debug)]
enum GodotPrim {
    Trimesh(Vec<Vector3>),
    Convex(Vec<Vector3>),
    Box {
        half_extents: Vector3,
        placement: Transform3D,
    },
    Sphere {
        radius: f32,
        centre: Vector3,
    },
    Capsule {
        height: f32,
        radius: f32,
        placement: Transform3D,
    },
}

/// Use native shapes and body_add_shape's local transform. See
/// engine-refs/godot-docs/classes/class_physicsserver3d.rst.
fn flatten(shape: &Shape, t: &Iso, out: &mut Vec<GodotPrim>) {
    match shape {
        Shape::Box { half_extents } => {
            let scale = t.scale.abs() * METRES_PER_UNIT;
            let direction = |v| godot_point(t.rotation.apply(v)) / METRES_PER_UNIT;
            out.push(GodotPrim::Box {
                half_extents: Vector3::new(half_extents.x, half_extents.y, half_extents.z) * scale,
                placement: Transform3D::new(
                    Basis::from_cols(
                        direction(Vec3::new(1.0, 0.0, 0.0)),
                        direction(Vec3::new(0.0, 1.0, 0.0)),
                        direction(Vec3::Z),
                    ),
                    godot_point(t.translation),
                ),
            });
        }
        Shape::ConvexHull { points } => {
            if !points.is_empty() {
                out.push(GodotPrim::Convex(
                    points.iter().map(|p| godot_point(t.apply(*p))).collect(),
                ));
            }
        }
        Shape::Sphere { radius } => {
            out.push(GodotPrim::Sphere {
                radius: radius * t.scale.abs() * METRES_PER_UNIT,
                centre: godot_point(t.translation),
            });
        }
        Shape::Capsule { a, b, radius } => {
            let (a, b) = (godot_point(t.apply(*a)), godot_point(t.apply(*b)));
            let axis = b - a;
            let length = axis.length();
            let radius = radius * t.scale.abs() * METRES_PER_UNIT;
            let centre = (a + b) * 0.5;
            if length < 1e-6 {
                out.push(GodotPrim::Sphere { radius, centre });
            } else {
                let y = axis / length;
                let helper = if y.dot(Vector3::RIGHT).abs() < 0.9 {
                    Vector3::RIGHT
                } else {
                    Vector3::BACK
                };
                let z = helper.cross(y).normalized();
                let x = y.cross(z);
                out.push(GodotPrim::Capsule {
                    height: length + 2.0 * radius,
                    radius,
                    placement: Transform3D::new(Basis::from_cols(x, y, z), centre),
                });
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

/// One game unit point in Godot world space, metres and Godot's axes.
fn godot_point(v: Vec3) -> Vector3 {
    let g = SkyrimPos::new(v.x, v.y, v.z).to_godot();
    Vector3::new(g.x, g.y, g.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vector3, b: Vector3) {
        assert!((a - b).length() < 1e-5, "{a:?} != {b:?}");
    }

    #[test]
    fn translated_sphere_keeps_radius_and_metres() {
        let t = Iso {
            translation: Vec3::new(10.0, 20.0, 30.0),
            scale: 2.0,
            ..Iso::IDENTITY
        };
        let mut out = Vec::new();
        flatten(&Shape::Sphere { radius: 7.0 }, &t, &mut out);
        let GodotPrim::Sphere { radius, centre } = out[0] else {
            panic!("native sphere expected")
        };
        close(centre, godot_point(t.translation));
        assert!((radius - 14.0 * METRES_PER_UNIT).abs() < 1e-6);
    }

    #[test]
    fn capsules_keep_both_endpoints_and_the_cylinder() {
        for (a, b) in [
            (Vec3::new(0.0, 0.0, 10.0), Vec3::new(0.0, 0.0, 50.0)),
            (Vec3::new(20.0, 30.0, 40.0), Vec3::new(70.0, 80.0, 90.0)),
            (Vec3::new(0.0, 0.0, 50.0), Vec3::new(0.0, 0.0, 10.0)),
        ] {
            let t = Iso::placement([10.0, 20.0, 30.0], [0.2, 0.3, 0.4], 1.5);
            let mut out = Vec::new();
            flatten(&Shape::Capsule { a, b, radius: 5.0 }, &t, &mut out);
            assert_eq!(out.len(), 1);
            let GodotPrim::Capsule {
                height,
                radius,
                placement,
            } = out[0]
            else {
                panic!("native capsule expected")
            };
            let half_axis = (height - 2.0 * radius) * 0.5;
            close(
                placement * Vector3::new(0.0, -half_axis, 0.0),
                godot_point(t.apply(a)),
            );
            close(
                placement * Vector3::new(0.0, half_axis, 0.0),
                godot_point(t.apply(b)),
            );
        }
    }

    #[test]
    fn compound_box_transform_preserves_rotated_corners() {
        let child = Iso::placement([10.0, 20.0, 30.0], [0.1, 0.4, 0.2], 2.0);
        let parent = Iso::placement([50.0, 40.0, 30.0], [0.3, 0.2, 0.1], 1.5);
        let h = Vec3::new(2.0, 3.0, 4.0);
        let shape = Shape::Compound(vec![(child, Shape::Box { half_extents: h })]);
        let mut out = Vec::new();
        flatten(&shape, &parent, &mut out);
        let GodotPrim::Box {
            half_extents,
            placement,
        } = out[0]
        else {
            panic!("native box expected")
        };
        close(
            placement * half_extents,
            godot_point(parent.then(&child).apply(h)),
        );
    }
}
