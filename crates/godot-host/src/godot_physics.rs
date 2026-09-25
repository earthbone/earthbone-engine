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

use std::collections::HashMap;

use godot::classes::physics_server_3d::{BodyMode, BodyState};
use godot::classes::PhysicsServer3D;
use godot::prelude::*;

use soe_esm::GlobalFormId;
use soe_game::world::ContactActor;
use soe_physics::{Collider, Iso, Shape, Vec3};

use crate::collision::PlacedCollider;
use crate::combat::{plan_body_sync, BodySyncPlan};
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

/// One collider's live server state, grouped so a reference's body and
/// shapes can be freed and rebuilt together when it moves. `reference` and
/// `original` are `None` for a body [`add_statics`] created, which has no
/// per-reference identity to sync against.
struct StaticEntry {
    reference: Option<GlobalFormId>,
    /// The reference's placement at load time, the baseline a runtime move
    /// is diffed against. `source.transform` already has this baked in.
    /// Never reassigned: every rebuild recomputes its delta against this
    /// same value, never against the last placement applied, or a second
    /// move of the same reference would drift.
    original: Option<Iso>,
    /// The unmoved collider, in Skyrim space, never mutated. A move
    /// recomputes a fresh top-level transform from this, so repeated moves
    /// never drift from composing deltas onto an already-moved copy.
    source: Collider,
    /// The last visibility and placement `sync_placements` applied, so an
    /// unchanged reference costs one comparison instead of a body rebuild.
    /// `None` until the first sync.
    synced: Option<(bool, Iso)>,
    body: Rid,
    shapes: Vec<Rid>,
}

/// The server side handles for one cell's collision, so a caller can free
/// them when the cell unloads. Dropping this does not free anything, the
/// server owns the ids, call [`CellBodies::free`].
#[derive(Default)]
pub struct CellBodies {
    entries: Vec<StaticEntry>,
    pub stats: GodotCollisionStats,
}

impl CellBodies {
    /// Every static body's RID, in insertion order. Diagnostic callers may
    /// inspect these handles, but must not free them, `free` owns that.
    pub fn body_rids(&self) -> impl Iterator<Item = Rid> + '_ {
        self.entries.iter().map(|entry| entry.body)
    }

    /// Releases every id back to the server. A cell that unloads must call
    /// this, the server does not reclaim ids on its own.
    pub fn free(&mut self) {
        let mut server = PhysicsServer3D::singleton();
        for entry in self.entries.drain(..) {
            server.free_rid(entry.body);
            for shape in entry.shapes {
                server.free_rid(shape);
            }
        }
        self.stats = GodotCollisionStats::default();
    }

    /// Pushes a tracked reference's current placement and visibility to the
    /// physics server, once per distinct change. Called every host tick, but
    /// `entry.synced` makes an unmoved reference cost one comparison: a body
    /// rebuild is real work, not a cheap idempotent server call (contrast
    /// `body_set_space` below, which stays unconditional in its own branches
    /// since re-issuing it costs nothing).
    ///
    /// A disabled reference has its body pulled out of `space`, matching
    /// vanilla's "not present in the simulation" contract rather than a
    /// visual-only hide (`docs/godot-host.md`, "Runtime reference sync").
    /// A moved reference has its body and shapes freed and rebuilt from
    /// `source` through [`flatten`], the same Skyrim-space path
    /// [`add_placed_statics`] used to build them, so no world-space
    /// transform is hand-derived here, and the delta is always taken
    /// against `entry.original`, never against a previous move, so a
    /// reference moved many times over many ticks never drifts. Entries
    /// [`add_statics`] created (`reference: None`) are left untouched.
    pub fn sync_placements(&mut self, world: &soe_game::World, space: Rid) {
        let mut server = PhysicsServer3D::singleton();
        for entry in &mut self.entries {
            let (Some(reference), Some(original)) = (entry.reference, entry.original) else {
                continue;
            };
            let Some(target) = world.ref_placement(reference) else {
                continue;
            };
            let target_iso = Iso::placement(target.position, target.rotation, target.scale);
            let key = (target.visible, target_iso);
            if entry.synced == Some(key) {
                continue;
            }
            entry.synced = Some(key);
            if !target.visible {
                server.body_set_space(entry.body, Rid::Invalid);
                continue;
            }
            if target_iso == original {
                server.body_set_space(entry.body, space);
                continue;
            }
            let delta = target_iso.then(&original.inverse());
            let mut moved = entry.source.clone();
            moved.transform = delta.then(&entry.source.transform);
            server.free_rid(entry.body);
            self.stats.bodies = self.stats.bodies.saturating_sub(1);
            self.stats.shapes = self.stats.shapes.saturating_sub(entry.shapes.len());
            for shape in entry.shapes.drain(..) {
                server.free_rid(shape);
            }
            match build_body(&mut server, space, &moved, &mut self.stats) {
                Some((body, shapes)) => {
                    entry.body = body;
                    entry.shapes = shapes;
                }
                None => {
                    // Flattened to nothing, a degenerate scale. Leave an
                    // empty body rather than a dangling freed one.
                    entry.body = server.body_create();
                    entry.shapes = Vec::new();
                }
            }
        }
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
        if let Some((body, shapes)) = build_body(&mut server, space, collider, &mut out.stats) {
            out.entries.push(StaticEntry {
                reference: None,
                original: None,
                source: collider.clone(),
                synced: None,
                body,
                shapes,
            });
        }
    }
    out
}

/// Adds every static collider to `space`, keeping each body's owning
/// reference and load-time placement so [`CellBodies::sync_placements`] can
/// later hide or move it. The reference-tracked sibling of [`add_statics`].
pub fn add_placed_statics(space: Rid, colliders: &[PlacedCollider]) -> CellBodies {
    let _span = soe_trace::Span::new("host.godot_physics", "host");
    let mut server = PhysicsServer3D::singleton();
    let mut out = CellBodies::default();

    for placed in colliders.iter().filter(|p| p.collider.is_static()) {
        if let Some((body, shapes)) =
            build_body(&mut server, space, &placed.collider, &mut out.stats)
        {
            out.entries.push(StaticEntry {
                reference: Some(placed.reference),
                original: Some(placed.original),
                source: placed.collider.clone(),
                synced: None,
                body,
                shapes,
            });
        }
    }
    out
}

/// Flattens `collider` and builds one static body with one shape per
/// primitive. `None` when it flattens to nothing, the caller counts that as
/// skipped. Shared by [`add_statics`], [`add_placed_statics`] and
/// [`CellBodies::sync_placements`], so a moved reference's rebuilt body goes
/// through exactly the same Skyrim-space-to-Godot conversion as its first
/// build.
fn build_body(
    server: &mut Gd<PhysicsServer3D>,
    space: Rid,
    collider: &Collider,
    stats: &mut GodotCollisionStats,
) -> Option<(Rid, Vec<Rid>)> {
    let mut prims = Vec::new();
    flatten(&collider.shape, &collider.transform, &mut prims);
    if prims.is_empty() {
        stats.skipped += 1;
        return None;
    }
    let body = server.body_create();
    server.body_set_mode(body, godot::classes::physics_server_3d::BodyMode::STATIC);
    server.body_set_space(body, space);
    let mut shapes = Vec::with_capacity(prims.len());
    for prim in prims {
        let mut transform = Transform3D::IDENTITY;
        let shape = match prim {
            GodotPrim::Trimesh(faces) => {
                stats.triangles += faces.len() / 3;
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
        shapes.push(shape);
        stats.shapes += 1;
    }
    stats.bodies += 1;
    Some((body, shapes))
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
        Shape::Material { shape, .. } => flatten(shape, t, out),
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
            ..
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

/// One actor's server body and the scale its capsule was built for.
#[derive(Debug)]
struct ActorBody {
    body: Rid,
    shapes: Vec<Rid>,
    scale: f32,
}

/// Kinematic controller bodies for the actors the shared World reports as
/// resident. Each actor owns a `PhysicsServer3D` body RID, so no scene node
/// exists per reference. [`ActorBodies::sync`] follows the World every call:
/// it creates bodies for new actors, moves kept ones and frees the rest,
/// which is how a cell change drops the leaving cell's actors.
#[derive(Debug, Default)]
pub struct ActorBodies {
    space: Option<Rid>,
    bodies: HashMap<GlobalFormId, ActorBody>,
    owners: HashMap<Rid, GlobalFormId>,
}

impl ActorBodies {
    /// Bring the server bodies in line with `actors`. A different `space`
    /// frees every body first, since a body belongs to one space.
    pub fn sync(&mut self, space: Rid, actors: &[ContactActor]) -> BodySyncPlan {
        if self.space.is_some_and(|held| held != space) {
            self.free();
        }
        self.space = Some(space);
        let scales: HashMap<GlobalFormId, f32> = self
            .bodies
            .iter()
            .map(|(form, body)| (*form, body.scale))
            .collect();
        let plan = plan_body_sync(&scales, actors);
        let mut server = PhysicsServer3D::singleton();
        for form in &plan.removed {
            if let Some(body) = self.bodies.remove(form) {
                self.owners.remove(&body.body);
                release(&mut server, body);
            }
        }
        for actor in &plan.create {
            let body = create_actor_body(&mut server, space, actor);
            self.owners.insert(body.body, actor.actor);
            self.bodies.insert(actor.actor, body);
        }
        for actor in &plan.moved {
            if let Some(body) = self.bodies.get(&actor.actor) {
                place(&mut server, body.body, actor);
            }
        }
        plan
    }

    /// Body RID to actor, for contact queries.
    #[must_use]
    pub fn owners(&self) -> &HashMap<Rid, GlobalFormId> {
        &self.owners
    }

    /// The body presenting `actor`, if it is resident.
    #[must_use]
    pub fn body(&self, actor: GlobalFormId) -> Option<Rid> {
        self.bodies.get(&actor).map(|body| body.body)
    }

    /// Number of actor bodies held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bodies.len()
    }

    /// Whether no actor body is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }

    /// Release every body and shape. The server does not reclaim them.
    pub fn free(&mut self) {
        let mut server = PhysicsServer3D::singleton();
        for (_, body) in self.bodies.drain() {
            release(&mut server, body);
        }
        self.owners.clear();
        self.space = None;
    }
}

fn release(server: &mut Gd<PhysicsServer3D>, body: ActorBody) {
    server.free_rid(body.body);
    for shape in body.shapes {
        server.free_rid(shape);
    }
}

/// The CMB-33 generated controller capsule, feet at the body origin.
fn create_actor_body(
    server: &mut Gd<PhysicsServer3D>,
    space: Rid,
    actor: &ContactActor,
) -> ActorBody {
    let body = server.body_create();
    server.body_set_mode(body, BodyMode::KINEMATIC);
    server.body_set_space(body, space);
    let mut prims = Vec::new();
    flatten(
        &soe_physics::generated_controller_shape(actor.scale),
        &Iso::IDENTITY,
        &mut prims,
    );
    let mut shapes = Vec::with_capacity(prims.len());
    for prim in prims {
        let GodotPrim::Capsule {
            height,
            radius,
            placement,
        } = prim
        else {
            continue;
        };
        let shape = server.capsule_shape_create();
        let mut data: Dictionary<Variant, Variant> = Dictionary::new();
        data.set("height", height);
        data.set("radius", radius);
        server.shape_set_data(shape, &data.to_variant());
        server
            .body_add_shape_ex(body, shape)
            .transform(placement)
            .done();
        shapes.push(shape);
    }
    place(server, body, actor);
    ActorBody {
        body,
        shapes,
        scale: actor.scale,
    }
}

/// Move a body to the actor's feet. The capsule is upright and round, so
/// only the position matters.
fn place(server: &mut Gd<PhysicsServer3D>, body: Rid, actor: &ContactActor) {
    let [x, y, z] = actor.position;
    let transform = Transform3D::new(Basis::IDENTITY, godot_point(Vec3::new(x, y, z)));
    server.body_set_state(body, BodyState::TRANSFORM, &transform.to_variant());
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

    #[test]
    fn distant_mesh_vertices_keep_placement_within_three_millimetres() {
        let vertices = vec![
            Vec3::new(-10.25, -20.5, 0.125),
            Vec3::new(30.75, -20.5, 0.125),
            Vec3::new(30.75, 40.25, 0.125),
        ];
        let shape = Shape::TriangleMesh {
            materials: Vec::new(),
            vertices: vertices.clone(),
            triangles: vec![[0, 1, 2]],
        };
        for cell in [0.0, -256.0, -128.0, -65.0, -64.0, 64.0, 65.0, 128.0, 256.0] {
            for y in [0.0, cell, -cell] {
                let offset = Vec3::new(cell * 4096.0, y * 4096.0, 128.0);
                let mut out = Vec::new();
                flatten(&shape, &Iso::translation(offset), &mut out);
                let GodotPrim::Trimesh(faces) = &out[0] else {
                    panic!("triangle mesh expected")
                };
                assert_eq!(faces.len(), 3);
                for (point, vertex) in faces.iter().zip(&vertices) {
                    let expected = [
                        (f64::from(offset.x) + f64::from(vertex.x)) * 0.0142875,
                        (f64::from(offset.z) + f64::from(vertex.z)) * 0.0142875,
                        -(f64::from(offset.y) + f64::from(vertex.y)) * 0.0142875,
                    ];
                    for (actual, expected) in [point.x, point.y, point.z].into_iter().zip(expected)
                    {
                        assert!(
                            (f64::from(actual) - expected).abs() <= 0.003,
                            "vertex drift at {offset:?}: {actual} versus {expected}"
                        );
                    }
                }
            }
        }
    }
}
