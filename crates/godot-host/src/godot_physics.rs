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

use std::collections::{HashMap, HashSet};

use godot::classes::physics_server_3d::{BodyMode, BodyParameter, BodyState};
use godot::classes::PhysicsServer3D;
use godot::prelude::*;

use soe_esm::GlobalFormId;
use soe_game::world::ContactActor;
use soe_game::world::ReferencePhysicsRequest;
use soe_physics::{Collider, Iso, Shape, Vec3};

use crate::collision::PlacedCollider;
use crate::combat::{plan_body_sync, BodySyncPlan};
use crate::units::{direction_to_skyrim, placement_to_godot, GodotPos, SkyrimPos, METRES_PER_UNIT};

/// Map valid Skyrim Havok motion types to the host body mode they require.
pub fn motion_type_to_body_mode(motion_type: u32) -> Option<BodyMode> {
    match motion_type {
        1 | 2 | 3 | 6 => Some(BodyMode::RIGID),
        4 | 7 => Some(BodyMode::KINEMATIC),
        5 => Some(BodyMode::STATIC),
        _ => None,
    }
}

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

pub(crate) fn is_opened_door(world: &soe_game::World, reference: GlobalFormId) -> bool {
    world.reference_ref(reference).is_some_and(|state| {
        state.opened
            && state
                .base
                .is_some_and(|base| world.tag_of(base) == Some(soe_esm::Tag::new(b"DOOR")))
    })
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

struct MovableEntry {
    reference: GlobalFormId,
    original: Iso,
    source: Collider,
    shape_scale: f32,
    body: Rid,
    shapes: Vec<Rid>,
    synced: Option<(bool, Iso)>,
}

/// The server side handles for one cell's static and movable collision, so a caller can free
/// them when the cell unloads. Dropping this does not free anything, the
/// server owns the ids, call [`CellBodies::free`].
#[derive(Default)]
pub struct CellBodies {
    entries: Vec<StaticEntry>,
    movable: Vec<MovableEntry>,
    grabbed_reference: Option<GlobalFormId>,
    pub stats: GodotCollisionStats,
}

impl CellBodies {
    /// Find the server body carrying a placed reference.
    pub fn body_for_reference(&self, reference: GlobalFormId) -> Option<Rid> {
        self.entries
            .iter()
            .find(|entry| entry.reference == Some(reference))
            .map(|entry| entry.body)
            .or_else(|| {
                self.movable
                    .iter()
                    .find(|entry| entry.reference == reference && entry.body.is_valid())
                    .map(|entry| entry.body)
            })
    }

    /// Put a reference body into kinematic mode while the player holds it.
    pub fn set_held(&self, reference: GlobalFormId, held: bool) -> bool {
        let Some(body) = self.body_for_reference(reference) else {
            return false;
        };
        let mut server = PhysicsServer3D::singleton();
        server.body_set_mode(
            body,
            if held {
                BodyMode::KINEMATIC
            } else {
                BodyMode::RIGID
            },
        );
        true
    }

    /// Move a held reference body by a Godot-space offset from its authored pose.
    pub fn move_held(&self, reference: GlobalFormId, offset: Vector3) -> bool {
        let Some(body) = self.body_for_reference(reference) else {
            return false;
        };
        let mut server = PhysicsServer3D::singleton();
        let mut transform = Transform3D::IDENTITY;
        transform.origin = offset;
        server.body_set_state(body, BodyState::TRANSFORM, &transform.to_variant());
        true
    }

    /// Apply queued Papyrus motion and impulse operations to resident bodies.
    pub fn apply_reference_physics_requests(
        &mut self,
        world: &mut soe_game::World,
        tethers: &mut crate::godot_tether::TetherConstraints,
    ) -> usize {
        let requests = world.take_reference_physics_requests();
        let mut applied = 0;
        let mut server = PhysicsServer3D::singleton();
        for request in requests {
            match request {
                ReferencePhysicsRequest::MotionType {
                    reference,
                    motion_type,
                    allow_activate,
                } => {
                    let Some(entry) = self
                        .movable
                        .iter_mut()
                        .find(|entry| entry.reference == reference && entry.body.is_valid())
                    else {
                        continue;
                    };
                    let Some(mode) = motion_type_to_body_mode(motion_type) else {
                        continue;
                    };
                    server.body_set_mode(entry.body, mode);
                    if motion_type == 1 && !allow_activate {
                        server.body_set_state(entry.body, BodyState::SLEEPING, &true.to_variant());
                    }
                    applied += 1;
                }
                ReferencePhysicsRequest::Impulse {
                    reference,
                    direction,
                    magnitude,
                } => {
                    let Some(entry) = self
                        .movable
                        .iter()
                        .find(|entry| entry.reference == reference && entry.body.is_valid())
                    else {
                        continue;
                    };
                    let direction = crate::units::SkyrimPos::new(
                        direction[0] * magnitude,
                        direction[1] * magnitude,
                        direction[2] * magnitude,
                    )
                    .to_godot();
                    server.body_apply_central_impulse(
                        entry.body,
                        Vector3::new(direction.x, direction.y, direction.z),
                    );
                    applied += 1;
                }
                ReferencePhysicsRequest::Tether {
                    cart,
                    horse,
                    cart_pivot,
                    horse_pivot,
                } => {
                    let (Some(cart_body), Some(horse_body)) = (
                        self.body_for_reference(cart),
                        self.body_for_reference(horse),
                    ) else {
                        continue;
                    };
                    let cart_pivot = crate::units::direction_to_godot(cart_pivot);
                    let horse_pivot = crate::units::direction_to_godot(horse_pivot);
                    if tethers.tether(
                        cart,
                        horse,
                        cart_body,
                        horse_body,
                        Vector3::new(cart_pivot[0], cart_pivot[1], cart_pivot[2]),
                        Vector3::new(horse_pivot[0], horse_pivot[1], horse_pivot[2]),
                    ) {
                        applied += 1;
                    }
                }
            }
        }
        applied
    }

    #[must_use]
    pub fn grabbed_reference(&self) -> Option<GlobalFormId> {
        self.grabbed_reference
    }

    /// Every live body's RID, in insertion order. Diagnostic callers may
    /// inspect these handles, but must not free them, `free` owns that.
    pub fn body_rids(&self) -> impl Iterator<Item = Rid> + '_ {
        self.entries.iter().map(|entry| entry.body).chain(
            self.movable
                .iter()
                .map(|entry| entry.body)
                .filter(Rid::is_valid),
        )
    }

    /// Number of live movable bodies in this cell.
    #[must_use]
    pub fn movable_count(&self) -> usize {
        self.movable
            .iter()
            .filter(|entry| entry.body.is_valid())
            .count()
    }

    /// Materializes movable references before their first visibility sync.
    pub fn ensure_references(&self, world: &mut soe_game::World) {
        for entry in &self.movable {
            world.reference(entry.reference);
        }
    }

    /// Put one resident movable body into host-controlled motion.
    pub fn grab_movable_reference(&mut self, reference: GlobalFormId) -> bool {
        if self.grabbed_reference.is_some() {
            return false;
        }
        let Some(entry) = self
            .movable
            .iter_mut()
            .find(|entry| entry.reference == reference && entry.body.is_valid())
        else {
            return false;
        };
        PhysicsServer3D::singleton().body_set_mode(entry.body, BodyMode::KINEMATIC);
        self.grabbed_reference = Some(reference);
        true
    }

    /// Move the held body to a Skyrim-space position through PhysicsServer3D.
    pub fn move_grabbed_reference(&mut self, position: [f32; 3]) -> bool {
        let Some(reference) = self.grabbed_reference else {
            return false;
        };
        if !position.iter().all(|value| value.is_finite()) {
            return false;
        }
        let Some(entry) = self
            .movable
            .iter_mut()
            .find(|entry| entry.reference == reference && entry.body.is_valid())
        else {
            self.grabbed_reference = None;
            return false;
        };
        let mut server = PhysicsServer3D::singleton();
        let Ok(mut transform) = server
            .body_get_state(entry.body, BodyState::TRANSFORM)
            .try_to::<Transform3D>()
        else {
            return false;
        };
        let godot = crate::units::SkyrimPos::new(position[0], position[1], position[2]).to_godot();
        transform.origin = Vector3::new(godot.x, godot.y, godot.z);
        server.body_set_state(entry.body, BodyState::TRANSFORM, &transform.to_variant());
        true
    }

    /// Return the held body to rigid simulation, preserving its last pose.
    pub fn release_movable_reference(&mut self, reference: GlobalFormId) -> bool {
        if self.grabbed_reference != Some(reference) {
            return false;
        }
        let Some(entry) = self
            .movable
            .iter_mut()
            .find(|entry| entry.reference == reference && entry.body.is_valid())
        else {
            self.grabbed_reference = None;
            return false;
        };
        PhysicsServer3D::singleton().body_set_mode(entry.body, BodyMode::RIGID);
        self.grabbed_reference = None;
        true
    }

    /// Sample live rigid-body positions for host diagnostics.
    pub fn movable_poses(&self) -> Array<VarDictionary> {
        let mut result = Array::new();
        let server = PhysicsServer3D::singleton();
        for entry in self.movable.iter().filter(|entry| entry.body.is_valid()) {
            let Ok(pose) = server
                .body_get_state(entry.body, BodyState::TRANSFORM)
                .try_to::<Transform3D>()
            else {
                continue;
            };
            let position = GodotPos::new(pose.origin.x, pose.origin.y, pose.origin.z).to_skyrim();
            let mut row = VarDictionary::new();
            row.set("file", i64::from(entry.reference.file));
            row.set("object", i64::from(entry.reference.object));
            row.set("body", entry.body);
            row.set("position", Vector3::new(position.x, position.y, position.z));
            result.push(&row);
        }
        result
    }

    /// Releases every id back to the server. A cell that unloads must call
    /// this, the server does not reclaim ids on its own.
    pub fn free(&mut self) {
        let mut server = PhysicsServer3D::singleton();
        for entry in self.entries.drain(..) {
            if entry.body.is_valid() {
                server.free_rid(entry.body);
            }
            for shape in entry.shapes {
                server.free_rid(shape);
            }
        }
        for entry in self.movable.drain(..) {
            if entry.body.is_valid() {
                server.free_rid(entry.body);
            }
            for shape in entry.shapes {
                server.free_rid(shape);
            }
        }
        self.stats = GodotCollisionStats::default();
        self.grabbed_reference = None;
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
            let visible = target.visible && !is_opened_door(world, reference);
            let key = (visible, target_iso);
            if entry.synced == Some(key) {
                continue;
            }
            entry.synced = Some(key);
            if !visible {
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
            match build_body(
                &mut server,
                space,
                &moved,
                BodyMode::STATIC,
                &mut self.stats,
            ) {
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
        for entry in &mut self.movable {
            let Some(target) = world.ref_placement(entry.reference) else {
                continue;
            };
            let iso = Iso::placement(target.position, target.rotation, target.scale);
            let visible = target.visible && !is_opened_door(world, entry.reference);
            let key = (visible, iso);
            if entry.synced == Some(key) {
                continue;
            }
            entry.synced = Some(key);
            if visible {
                if entry.body.is_valid() && entry.shape_scale != iso.scale {
                    server.free_rid(entry.body);
                    for shape in entry.shapes.drain(..) {
                        server.free_rid(shape);
                        self.stats.shapes = self.stats.shapes.saturating_sub(1);
                    }
                    self.stats.bodies = self.stats.bodies.saturating_sub(1);
                    entry.body = Rid::Invalid;
                }
                if !entry.body.is_valid() {
                    let mut source = entry.source.clone();
                    source.transform.scale = iso.scale / entry.original.scale;
                    if let Some((body, shapes)) =
                        build_movable_body(&mut server, space, &source, iso, &mut self.stats)
                    {
                        entry.body = body;
                        entry.shapes = shapes;
                        entry.shape_scale = iso.scale;
                    }
                    continue;
                }
                server.body_set_space(entry.body, space);
                server.body_set_state(
                    entry.body,
                    BodyState::TRANSFORM,
                    &body_transform(&iso).to_variant(),
                );
            } else if entry.body.is_valid() {
                server.free_rid(entry.body);
                for shape in entry.shapes.drain(..) {
                    server.free_rid(shape);
                    self.stats.shapes = self.stats.shapes.saturating_sub(1);
                }
                self.stats.bodies = self.stats.bodies.saturating_sub(1);
                entry.body = Rid::Invalid;
            }
        }
        self.movable.retain(|entry| {
            !world
                .reference_ref(entry.reference)
                .is_some_and(|state| state.deleted)
        });
    }

    /// Copies the latest simulated pose into the record-backed World.
    /// Call after `sync_placements` so a game-side move wins before polling.
    pub fn poll_movable(&mut self, world: &mut soe_game::World) {
        let server = PhysicsServer3D::singleton();
        for entry in &mut self.movable {
            if !entry.body.is_valid()
                || !world
                    .reference(entry.reference)
                    .is_some_and(|state| state.enabled && !state.deleted)
            {
                continue;
            }
            let Ok(transform) = server
                .body_get_state(entry.body, BodyState::TRANSFORM)
                .try_to::<Transform3D>()
            else {
                continue;
            };
            let position =
                GodotPos::new(transform.origin.x, transform.origin.y, transform.origin.z)
                    .to_skyrim();
            let rotation = skyrim_rotation(transform.basis);
            let pose = [position.x, position.y, position.z];
            if world.set_host_ref_pose(entry.reference, pose, rotation) {
                entry.synced = Some((true, Iso::placement(pose, rotation, entry.shape_scale)));
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
        if let Some((body, shapes)) = build_body(
            &mut server,
            space,
            collider,
            BodyMode::STATIC,
            &mut out.stats,
        ) {
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

/// Adds placed static colliders while retaining reference identity for grabbing.
pub fn add_placed_query_statics(space: Rid, colliders: &[PlacedCollider]) -> CellBodies {
    let _span = soe_trace::Span::new("host.godot_physics", "host");
    let mut server = PhysicsServer3D::singleton();
    let mut out = CellBodies::default();
    for placed in colliders.iter().filter(|p| p.collider.is_static()) {
        if let Some((body, shapes)) = build_body(
            &mut server,
            space,
            &placed.collider,
            BodyMode::STATIC,
            &mut out.stats,
        ) {
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

/// Adds placed static and movable collision to `space`, keeping each body's
/// reference and load-time placement for grabbing and runtime synchronization.
pub fn add_placed_statics(space: Rid, colliders: &[PlacedCollider]) -> CellBodies {
    let _span = soe_trace::Span::new("host.godot_physics", "host");
    let mut server = PhysicsServer3D::singleton();
    let mut out = CellBodies::default();

    for placed in colliders.iter().filter(|p| p.collider.is_static()) {
        if let Some((body, shapes)) = build_body(
            &mut server,
            space,
            &placed.collider,
            BodyMode::STATIC,
            &mut out.stats,
        ) {
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
    let mut groups: Vec<(GlobalFormId, Iso, Vec<&Collider>)> = Vec::new();
    for placed in colliders.iter().filter(|p| !p.collider.is_static()) {
        if let Some((_, _, shapes)) = groups
            .iter_mut()
            .find(|(reference, _, _)| *reference == placed.reference)
        {
            shapes.push(&placed.collider);
        } else {
            groups.push((placed.reference, placed.original, vec![&placed.collider]));
        }
    }
    for (reference, original, colliders) in groups {
        let children = colliders
            .iter()
            .map(|collider| {
                let relative = original.inverse().then(&collider.transform);
                let local = Iso {
                    translation: relative.translation * original.scale,
                    scale: relative.scale * original.scale,
                    ..relative
                };
                (local, collider.shape.clone())
            })
            .collect();
        let mut combined = colliders[0].clone();
        combined.mass = colliders
            .iter()
            .map(|collider| collider.mass.max(0.0))
            .sum();
        combined.shape = Shape::Compound(children);
        combined.transform = Iso::IDENTITY;
        if let Some((body, shapes)) =
            build_movable_body(&mut server, space, &combined, original, &mut out.stats)
        {
            out.movable.push(MovableEntry {
                reference,
                original,
                source: combined,
                shape_scale: original.scale,
                body,
                shapes,
                synced: Some((true, original)),
            });
        }
    }
    out
}

fn body_transform(pose: &Iso) -> Transform3D {
    let t = placement_to_godot(
        pose.rotation.rows,
        [pose.translation.x, pose.translation.y, pose.translation.z],
        1.0,
    );
    let b = t.basis;
    Transform3D::new(
        Basis::from_rows(
            Vector3::new(b[0][0], b[0][1], b[0][2]),
            Vector3::new(b[1][0], b[1][1], b[1][2]),
            Vector3::new(b[2][0], b[2][1], b[2][2]),
        ),
        Vector3::new(t.origin.x, t.origin.y, t.origin.z),
    )
}

fn skyrim_rotation(basis: Basis) -> [f32; 3] {
    let axis = |v: Vector3| direction_to_skyrim([v.x, v.y, v.z]);
    let x = axis(basis * Vector3::RIGHT);
    let y = axis(basis * Vector3::FORWARD);
    let z = axis(basis * Vector3::UP);
    let r = [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]];
    let pitch = (-r[0][2]).clamp(-1.0, 1.0).asin();
    let roll = if pitch.cos().abs() > 1e-5 {
        r[1][2].atan2(r[2][2])
    } else {
        (-r[1][0] * r[0][2]).atan2(r[1][1])
    };
    let yaw = if pitch.cos().abs() > 1e-5 {
        r[0][1].atan2(r[0][0])
    } else {
        0.0
    };
    [roll, pitch, yaw]
}

fn build_movable_body(
    server: &mut Gd<PhysicsServer3D>,
    space: Rid,
    collider: &Collider,
    pose: Iso,
    stats: &mut GodotCollisionStats,
) -> Option<(Rid, Vec<Rid>)> {
    let (body, shapes) = build_body(server, space, collider, BodyMode::RIGID, stats)?;
    server.body_set_state(
        body,
        BodyState::TRANSFORM,
        &body_transform(&pose).to_variant(),
    );
    Some((body, shapes))
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
    mode: BodyMode,
    stats: &mut GodotCollisionStats,
) -> Option<(Rid, Vec<Rid>)> {
    let mut prims = Vec::new();
    flatten(&collider.shape, &collider.transform, &mut prims);
    if mode == BodyMode::RIGID {
        let count = prims.len();
        prims.retain(|prim| !matches!(prim, GodotPrim::Trimesh(_)));
        stats.skipped += count - prims.len();
    }
    if prims.is_empty() {
        stats.skipped += 1;
        return None;
    }
    let body = server.body_create();
    server.body_set_mode(body, mode);
    if mode == BodyMode::RIGID {
        server.body_set_param(body, BodyParameter::MASS, &collider.mass.to_variant());
    }
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

/// Query bodies in the controller capsule shape for the actors the shared
/// World reports as resident. Each actor owns a `PhysicsServer3D` body RID,
/// so no scene node exists per reference. [`ActorBodies::sync`] follows the
/// World every call: it creates bodies for new actors, moves kept ones and
/// frees the rest, which is how a cell change drops the leaving cell's
/// actors.
///
/// Contact bodies are STATIC, not KINEMATIC. Godot's Jolt backend stores a
/// kinematic body's new transform and applies it with `MoveKinematic` on the
/// next physics step, so a HitFrame query in the same tick would see the
/// previous pose, and a body created that tick would still sit at the
/// origin. Held actors become KINEMATIC and thrown actors become RIGID until
/// their motion settles. Static bodies are repositioned at once.
#[derive(Debug, Default)]
pub struct ActorBodies {
    space: Option<Rid>,
    bodies: HashMap<GlobalFormId, ActorBody>,
    owners: HashMap<Rid, GlobalFormId>,
    grabbed_actor: Option<GlobalFormId>,
    thrown_actors: HashSet<GlobalFormId>,
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
            self.thrown_actors.remove(form);
            if self.grabbed_actor == Some(*form) {
                self.grabbed_actor = None;
            }
        }
        for actor in &plan.create {
            let body = create_actor_body(&mut server, space, actor);
            self.owners.insert(body.body, actor.actor);
            self.bodies.insert(actor.actor, body);
        }
        for actor in &plan.moved {
            if self.grabbed_actor != Some(actor.actor) && !self.thrown_actors.contains(&actor.actor)
            {
                if let Some(body) = self.bodies.get(&actor.actor) {
                    place(&mut server, body.body, actor);
                }
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

    /// Put one resident actor body under host control for Grab Actor.
    pub fn grab_actor(&mut self, actor: GlobalFormId) -> bool {
        if self.grabbed_actor.is_some() || self.thrown_actors.contains(&actor) {
            return false;
        }
        let Some(body) = self.bodies.get(&actor).filter(|body| body.body.is_valid()) else {
            return false;
        };
        PhysicsServer3D::singleton().body_set_mode(body.body, BodyMode::KINEMATIC);
        self.grabbed_actor = Some(actor);
        true
    }

    /// Move the held actor's feet to a finite Skyrim-space position.
    pub fn move_grabbed_actor(&mut self, actor: GlobalFormId, position: [f32; 3]) -> bool {
        if self.grabbed_actor != Some(actor) || !position.iter().all(|value| value.is_finite()) {
            return false;
        }
        let Some(body) = self.bodies.get(&actor) else {
            self.grabbed_actor = None;
            return false;
        };
        let point = crate::units::SkyrimPos::new(position[0], position[1], position[2]).to_godot();
        let transform = Transform3D::new(Basis::IDENTITY, Vector3::new(point.x, point.y, point.z));
        PhysicsServer3D::singleton().body_set_state(
            body.body,
            BodyState::TRANSFORM,
            &transform.to_variant(),
        );
        true
    }

    /// Release the held actor with a host-space throw impulse.
    pub fn throw_grabbed_actor(&mut self, actor: GlobalFormId, impulse: Vector3) -> bool {
        if self.grabbed_actor != Some(actor) || !impulse.is_finite() {
            return false;
        }
        let Some(body) = self.bodies.get(&actor) else {
            self.grabbed_actor = None;
            return false;
        };
        let mut server = PhysicsServer3D::singleton();
        server.body_set_mode(body.body, BodyMode::RIGID);
        server.body_apply_central_impulse(body.body, impulse);
        self.grabbed_actor = None;
        self.thrown_actors.insert(actor);
        true
    }

    /// Current host poses for thrown actors, updating shared World placement
    /// until a throw settles and its body returns to contact-only mode.
    pub fn thrown_actor_poses(&mut self) -> Vec<(GlobalFormId, [f32; 3])> {
        let mut server = PhysicsServer3D::singleton();
        let mut poses = Vec::with_capacity(self.thrown_actors.len());
        let mut settled = Vec::new();
        for actor in &self.thrown_actors {
            let Some(body) = self.bodies.get(actor) else {
                settled.push(*actor);
                continue;
            };
            let Ok(transform) = server
                .body_get_state(body.body, BodyState::TRANSFORM)
                .try_to::<Transform3D>()
            else {
                server.body_set_mode(body.body, BodyMode::STATIC);
                settled.push(*actor);
                continue;
            };
            if !transform.origin.is_finite() {
                server.body_set_mode(body.body, BodyMode::STATIC);
                settled.push(*actor);
                continue;
            }
            let point = crate::units::GodotPos::new(
                transform.origin.x,
                transform.origin.y,
                transform.origin.z,
            )
            .to_skyrim();
            poses.push((*actor, [point.x, point.y, point.z]));
            let velocity = server
                .body_get_state(body.body, BodyState::LINEAR_VELOCITY)
                .try_to::<Vector3>()
                .unwrap_or(Vector3::ZERO);
            if velocity.length_squared() < 0.01 {
                server.body_set_mode(body.body, BodyMode::STATIC);
                settled.push(*actor);
            }
        }
        for actor in settled {
            self.thrown_actors.remove(&actor);
        }
        poses.sort_unstable_by_key(|(actor, _)| *actor);
        poses
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
        self.grabbed_actor = None;
        self.thrown_actors.clear();
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
    server.body_set_mode(body, BodyMode::STATIC);
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

    #[test]
    fn hst_12_body_pose_round_trips_rotation_and_position() {
        let pose = Iso::placement([100.0, -200.0, 300.0], [0.2, -0.3, 0.4], 1.0);
        let host = body_transform(&pose);
        let rotation = skyrim_rotation(host.basis);
        let round_trip = Iso::placement([100.0, -200.0, 300.0], rotation, 1.0);
        for axis in [Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::Z] {
            let actual = round_trip.apply_direction(axis);
            let expected = pose.apply_direction(axis);
            assert!((actual - expected).length() < 1e-4);
        }
        let position = GodotPos::new(host.origin.x, host.origin.y, host.origin.z).to_skyrim();
        assert!((position.x - 100.0).abs() < 1e-4);
        assert!((position.y + 200.0).abs() < 1e-4);
        assert!((position.z - 300.0).abs() < 1e-4);
    }

    fn close(a: Vector3, b: Vector3) {
        assert!((a - b).length() < 1e-5, "{a:?} != {b:?}");
    }

    #[test]
    fn hst_18_hold_target_offset_uses_skyrim_to_godot_axes() {
        let authored_hit = Vec3::new(10.0, 20.0, 30.0);
        let target = Vec3::new(15.0, 26.0, 42.0);
        let offset = godot_point(Vec3::new(
            target.x - authored_hit.x,
            target.y - authored_hit.y,
            target.z - authored_hit.z,
        ));
        close(
            offset,
            Vector3::new(
                5.0 * METRES_PER_UNIT,
                12.0 * METRES_PER_UNIT,
                -6.0 * METRES_PER_UNIT,
            ),
        );
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
