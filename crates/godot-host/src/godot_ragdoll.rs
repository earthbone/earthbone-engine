// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot Jolt bodies for one dead actor, using decoded HKX shapes and joints.
//!
//! PhysicsServer3D owns the resources by RID. The actor node owns their
//! lifetime and releases every RID on resurrection or exit.

use godot::classes::physics_server_3d::{
    BodyMode, BodyParameter, BodyState, ConeTwistJointParam, HingeJointFlag, HingeJointParam,
};
use godot::classes::PhysicsServer3D;
use godot::prelude::*;
use soe_hkx::ragdoll::{ConstraintKind, Ragdoll, Shape};
use soe_hkx::QsTransform;

use crate::ragdoll::{affine_from_havok, RagdollBoneMap};
use crate::skinned::Affine;
use crate::units::METRES_PER_UNIT;

fn as_godot(a: &Affine) -> Transform3D {
    let a = a.to_godot();
    Transform3D::new(
        Basis::from_cols(
            Vector3::new(a.basis[0][0], a.basis[1][0], a.basis[2][0]),
            Vector3::new(a.basis[0][1], a.basis[1][1], a.basis[2][1]),
            Vector3::new(a.basis[0][2], a.basis[1][2], a.basis[2][2]),
        ),
        Vector3::new(a.origin[0], a.origin[1], a.origin[2]),
    )
}

fn as_godot_scaled(a: &Affine, scale: f32) -> Transform3D {
    let mut transform = as_godot(a);
    transform.origin *= scale;
    transform
}

fn point(p: [f32; 4], scale: f32) -> Vector3 {
    Vector3::new(p[0], p[2], -p[1]) * (METRES_PER_UNIT * scale)
}

fn shape_resource(
    server: &mut Gd<PhysicsServer3D>,
    shape: &Shape,
    scale: f32,
) -> (Rid, Transform3D) {
    match shape {
        Shape::Sphere { radius } => {
            let rid = server.sphere_shape_create();
            server.shape_set_data(rid, &(radius * METRES_PER_UNIT * scale).to_variant());
            (rid, Transform3D::IDENTITY)
        }
        Shape::Box { half_extents, .. } => {
            let rid = server.box_shape_create();
            server.shape_set_data(
                rid,
                &(Vector3::new(half_extents[0], half_extents[2], half_extents[1]).abs()
                    * METRES_PER_UNIT
                    * scale)
                    .to_variant(),
            );
            (rid, Transform3D::IDENTITY)
        }
        Shape::Capsule { radius, a, b } => {
            let (a, b) = (point(*a, scale), point(*b, scale));
            let axis = b - a;
            let length = axis.length();
            let radius = radius * METRES_PER_UNIT * scale;
            if length < 1e-6 {
                let rid = server.sphere_shape_create();
                server.shape_set_data(rid, &radius.to_variant());
                return (rid, Transform3D::new(Basis::IDENTITY, a));
            }
            let y = axis / length;
            let guide = if y.dot(Vector3::RIGHT).abs() < 0.9 {
                Vector3::RIGHT
            } else {
                Vector3::BACK
            };
            let z = guide.cross(y).normalized();
            let x = y.cross(z);
            let rid = server.capsule_shape_create();
            let mut data: Dictionary<Variant, Variant> = Dictionary::new();
            data.set("height", length + 2.0 * radius);
            data.set("radius", radius);
            server.shape_set_data(rid, &data.to_variant());
            (
                rid,
                Transform3D::new(Basis::from_cols(x, y, z), (a + b) * 0.5),
            )
        }
        Shape::ConvexHull(hull) => {
            let rid = server.convex_polygon_shape_create();
            let points: PackedVector3Array = hull
                .vertices
                .iter()
                .map(|v| point([v[0], v[1], v[2], 0.0], scale))
                .collect();
            server.shape_set_data(rid, &points.to_variant());
            (rid, Transform3D::IDENTITY)
        }
        Shape::Translated {
            translation, child, ..
        } => {
            let (rid, transform) = shape_resource(server, child, scale);
            (
                rid,
                Transform3D::new(Basis::IDENTITY, point(*translation, scale)) * transform,
            )
        }
        Shape::Transformed {
            transform, child, ..
        } => {
            let (rid, child_transform) = shape_resource(server, child, scale);
            (
                rid,
                as_godot_scaled(&affine_from_havok(transform), scale) * child_transform,
            )
        }
    }
}

fn create_anchor(server: &mut Gd<PhysicsServer3D>, space: Rid, anchors: &mut Vec<Rid>) -> Rid {
    let body = server.body_create();
    server.body_set_mode(body, BodyMode::STATIC);
    server.body_set_space(body, space);
    anchors.push(body);
    body
}

/// One actor's dynamic bodies, joint RIDs, and starting transforms for skinning.
pub struct LiveRagdoll {
    bodies: Vec<Rid>,
    shapes: Vec<Rid>,
    joints: Vec<Rid>,
    anchors: Vec<Rid>,
    initial: Vec<Transform3D>,
    actor_transform: Transform3D,
}

impl LiveRagdoll {
    /// Install an HKX ragdoll in the supplied Jolt space at the animated pose.
    pub fn create(
        space: Rid,
        actor_transform: Transform3D,
        ragdoll: &Ragdoll,
        map: &RagdollBoneMap,
        animated_model: &[QsTransform],
    ) -> Result<Self, soe_hkx::Error> {
        let reference = ragdoll
            .skeleton
            .to_model_space(&ragdoll.skeleton.reference_pose)?;
        let mut server = PhysicsServer3D::singleton();
        let scale = actor_transform.basis.col_a().length();
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let rigid_actor = Transform3D::new(
            Basis::from_cols(
                actor_transform.basis.col_a() / scale,
                actor_transform.basis.col_b() / scale,
                actor_transform.basis.col_c() / scale,
            ),
            actor_transform.origin,
        );
        let mut out = Self {
            bodies: Vec::with_capacity(ragdoll.bodies.len()),
            shapes: Vec::with_capacity(ragdoll.bodies.len()),
            joints: Vec::with_capacity(ragdoll.constraints.len()),
            anchors: Vec::new(),
            initial: Vec::with_capacity(ragdoll.bodies.len()),
            actor_transform,
        };
        for (index, source) in ragdoll.bodies.iter().enumerate() {
            let body = server.body_create();
            let (shape, local) = shape_resource(&mut server, &source.shape, scale);
            server
                .body_add_shape_ex(body, shape)
                .transform(local)
                .done();
            let authored = as_godot_scaled(&affine_from_havok(&source.motion.transform), scale);
            let delta = ragdoll
                .bone_to_body
                .iter()
                .position(|entry| *entry == Some(soe_hkx::ragdoll::BodyIndex(index)))
                .and_then(|bone| map.body_to_animated[index].map(|high| (bone, high)))
                .and_then(|(bone, high)| {
                    animated_model.get(high).map(|pose| {
                        as_godot(&Affine::from_qs(pose))
                            * as_godot(&Affine::from_qs(&reference[bone])).affine_inverse()
                    })
                })
                .unwrap_or(Transform3D::IDENTITY);
            let mut delta = delta;
            delta.origin *= scale;
            let initial = rigid_actor * delta * authored;
            let inverse_mass = source.motion.inertia_and_mass_inverse[3];
            if inverse_mass.is_finite() && inverse_mass > 0.0 {
                server.body_set_param(
                    body,
                    BodyParameter::MASS,
                    &(1.0 / inverse_mass).to_variant(),
                );
            }
            server.body_set_param(
                body,
                BodyParameter::FRICTION,
                &source.material.friction.to_variant(),
            );
            server.body_set_param(
                body,
                BodyParameter::BOUNCE,
                &source.material.restitution.to_variant(),
            );
            server.body_set_state(body, BodyState::TRANSFORM, &initial.to_variant());
            server.body_set_mode(body, BodyMode::RIGID);
            server.body_set_space(body, space);
            out.bodies.push(body);
            out.shapes.push(shape);
            out.initial.push(initial);
        }
        for constraint in &ragdoll.constraints {
            if constraint.bodies == [None, None] {
                continue;
            }
            let frames = match &constraint.kind {
                ConstraintKind::Ragdoll(data) => &data.frames,
                ConstraintKind::LimitedHinge(data) => &data.frames,
            };
            let mut frame_a = as_godot_scaled(&affine_from_havok(&frames[0]), scale);
            let mut frame_b = as_godot_scaled(&affine_from_havok(&frames[1]), scale);
            let a_body = match constraint.bodies[0] {
                Some(a) => out.bodies[a.0],
                None => create_anchor(&mut server, space, &mut out.anchors),
            };
            let b_body = match constraint.bodies[1] {
                Some(b) => out.bodies[b.0],
                None => create_anchor(&mut server, space, &mut out.anchors),
            };
            match constraint.bodies {
                [Some(a), Some(b)] => {
                    frame_b = out.initial[b.0].affine_inverse() * out.initial[a.0] * frame_a;
                }
                [Some(a), None] => frame_b = out.initial[a.0] * frame_a,
                [None, Some(b)] => frame_a = out.initial[b.0] * frame_b,
                [None, None] => continue,
            }
            let joint = server.joint_create();
            match &constraint.kind {
                ConstraintKind::Ragdoll(data) => {
                    server.joint_make_cone_twist(joint, a_body, frame_a, b_body, frame_b);
                    if data.cone.enabled {
                        server.cone_twist_joint_set_param(
                            joint,
                            ConeTwistJointParam::SWING_SPAN,
                            data.cone.max_angle.abs().max(data.cone.min_angle.abs()),
                        );
                    }
                    if data.twist.enabled {
                        server.cone_twist_joint_set_param(
                            joint,
                            ConeTwistJointParam::TWIST_SPAN,
                            data.twist.max_angle.abs().max(data.twist.min_angle.abs()),
                        );
                    }
                }
                ConstraintKind::LimitedHinge(data) => {
                    server.joint_make_hinge(joint, a_body, frame_a, b_body, frame_b);
                    if data.limit.enabled {
                        server.hinge_joint_set_flag(joint, HingeJointFlag::USE_LIMIT, true);
                        server.hinge_joint_set_param(
                            joint,
                            HingeJointParam::LIMIT_LOWER,
                            data.limit.min_angle,
                        );
                        server.hinge_joint_set_param(
                            joint,
                            HingeJointParam::LIMIT_UPPER,
                            data.limit.max_angle,
                        );
                    }
                }
            }
            server.joint_disable_collisions_between_bodies(joint, true);
            out.joints.push(joint);
        }
        Ok(out)
    }

    /// Local skinning delta of one body since death, in the actor's frame.
    pub fn local_delta(&self, index: usize) -> Option<Transform3D> {
        let body = *self.bodies.get(index)?;
        let initial = *self.initial.get(index)?;
        let current = PhysicsServer3D::singleton()
            .body_get_state(body, BodyState::TRANSFORM)
            .try_to::<Transform3D>()
            .ok()?;
        Some(
            self.actor_transform.affine_inverse()
                * current
                * initial.affine_inverse()
                * self.actor_transform,
        )
    }

    /// The dynamic body handles for contact registration and diagnostics.
    pub fn bodies(&self) -> &[Rid] {
        &self.bodies
    }

    /// Apply one Skyrim-space impulse to the root body in Godot metres.
    pub fn apply_impulse(&self, vector: [f32; 3]) {
        let Some(body) = self.bodies.first().copied() else {
            return;
        };
        let direction = crate::units::direction_to_godot(vector);
        let impulse = Vector3::new(
            direction[0] * crate::units::METRES_PER_UNIT,
            direction[1] * crate::units::METRES_PER_UNIT,
            direction[2] * crate::units::METRES_PER_UNIT,
        );
        PhysicsServer3D::singleton().body_apply_central_impulse(body, impulse);
    }

    /// Placement held at the death transition, before bodies move independently.
    pub fn actor_transform(&self) -> Transform3D {
        self.actor_transform
    }

    /// Release every Godot resource in reverse dependency order.
    pub fn free(&mut self) {
        let mut server = PhysicsServer3D::singleton();
        for joint in self.joints.drain(..) {
            server.free_rid(joint);
        }
        for body in self.bodies.drain(..) {
            server.free_rid(body);
        }
        for anchor in self.anchors.drain(..) {
            server.free_rid(anchor);
        }
        for shape in self.shapes.drain(..) {
            server.free_rid(shape);
        }
        self.initial.clear();
    }
}
