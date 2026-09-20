// SPDX-License-Identifier: GPL-3.0-or-later

//! Havok 2010.2 ragdolls from Skyrim SE skeleton packfiles.
//!
//! Layouts come from serde-hkx's extracted class descriptions. Body-space
//! joint frames follow activeragdoll/src/RE/havok.cpp. Coordinates, angles
//! and masses retain the file's units. This module reads data, it does not
//! simulate physics or map the ragdoll pose onto the animation skeleton.

mod constraint;
mod shape;

use std::collections::BTreeMap;

use crate::packfile::root_variant_of;
use crate::skeleton::Skeleton;
use crate::{Error, Obj, Packfile, Result};

pub use constraint::{
    AngularFriction, AngularLimit, BallSocket, ConeLimit, Constraint, ConstraintKind, HingeMotor,
    LimitedHingeJoint, PositionMotor, RagdollJoint, RagdollMotors, Stabilization, TwistLimit,
};
pub use shape::{ConvexHull, Shape};

/// Index into a ragdoll's body array, independent of packfile offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BodyIndex(pub usize);

/// A column-major `hkTransform`, with three rotation columns and a translation.
/// Each column retains the fourth component stored by Havok.
#[derive(Debug, Clone, PartialEq)]
pub struct Transform {
    pub rotation: [[f32; 4]; 3],
    pub translation: [f32; 4],
}

impl Transform {
    fn from_obj(o: Obj<'_>) -> Result<Self> {
        Ok(Self {
            rotation: [o.vec4(0)?, o.vec4(16)?, o.vec4(32)?],
            translation: o.vec4(48)?,
        })
    }
}

/// Serialized `hkpMotion::MotionType` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionType {
    Invalid,
    Dynamic,
    SphereInertia,
    BoxInertia,
    Keyframed,
    Fixed,
    ThinBoxInertia,
    Character,
    Other(u8),
}

/// Authored rigid-body mass properties and motion state.
#[derive(Debug, Clone, PartialEq)]
pub struct Motion {
    pub kind: MotionType,
    pub transform: Transform,
    pub center_of_mass_local: [f32; 4],
    /// Inverse diagonal inertia in xyz and inverse mass in w.
    pub inertia_and_mass_inverse: [f32; 4],
    pub linear_velocity: [f32; 4],
    pub angular_velocity: [f32; 4],
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub time_factor: f32,
    pub gravity_factor: f32,
}

impl Motion {
    fn from_obj(o: Obj<'_>) -> Result<Self> {
        o.bytes(0, 320)?;
        Ok(Self {
            kind: match o.u8(16)? {
                0 => MotionType::Invalid,
                1 => MotionType::Dynamic,
                2 => MotionType::SphereInertia,
                3 => MotionType::BoxInertia,
                4 => MotionType::Keyframed,
                5 => MotionType::Fixed,
                6 => MotionType::ThinBoxInertia,
                7 => MotionType::Character,
                n => MotionType::Other(n),
            },
            transform: Transform::from_obj(o.at(32))?,
            center_of_mass_local: o.vec4(160)?,
            inertia_and_mass_inverse: o.vec4(208)?,
            linear_velocity: o.vec4(224)?,
            angular_velocity: o.vec4(240)?,
            linear_damping: half(o, 196)?,
            angular_damping: half(o, 198)?,
            time_factor: half(o, 200)?,
            gravity_factor: half(o, 306)?,
        })
    }
}

/// Serialized contact material, including Havok's response-type value.
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub response_type: u8,
    pub rolling_friction_multiplier: f32,
    pub friction: f32,
    pub restitution: f32,
}

/// A rigid body with its authored shape, collision filter and motion state.
#[derive(Debug, Clone, PartialEq)]
pub struct RigidBody {
    pub name: String,
    pub user_data: u64,
    pub collision_filter_info: u32,
    pub quality_type: i8,
    pub allowed_penetration_depth: f32,
    pub material: Material,
    pub motion: Motion,
    pub shape: Shape,
}

impl RigidBody {
    /// Decode an `hkpRigidBody`, rejecting unknown shape classes.
    pub fn from_obj(o: Obj<'_>) -> Result<Self> {
        o.packfile().require_64bit()?;
        o.expect_class("hkpRigidBody")?;
        o.bytes(0, 720)?;
        Ok(Self {
            name: o.string_or_empty(176)?,
            user_data: o.u64(24)?,
            collision_filter_info: o.u32(76)?,
            quality_type: o.i8(74)?,
            allowed_penetration_depth: o.f32(136)?,
            material: Material {
                response_type: o.u8(208)?,
                rolling_friction_multiplier: half(o, 210)?,
                friction: o.f32(212)?,
                restitution: o.f32(216)?,
            },
            motion: Motion::from_obj(o.at(336))?,
            shape: Shape::from_obj(o.ptr_required(32, "rigid body shape")?)?,
        })
    }
}

/// An `hkaRagdollInstance` and its low resolution skeleton.
#[derive(Debug, Clone)]
pub struct Ragdoll {
    pub skeleton: Skeleton,
    pub bodies: Vec<RigidBody>,
    pub constraints: Vec<Constraint>,
    /// One entry per ragdoll bone. An authored -1 means no rigid body.
    pub bone_to_body: Vec<Option<BodyIndex>>,
}

impl Ragdoll {
    /// Read the ragdoll named by the packfile root container.
    pub fn from_packfile(pf: &Packfile) -> Result<Self> {
        Self::from_obj(root_variant_of(pf, "hkaRagdollInstance")?)
    }

    /// Decode a ragdoll, preserving array order and checking all references.
    pub fn from_obj(o: Obj<'_>) -> Result<Self> {
        o.packfile().require_64bit()?;
        o.expect_class("hkaRagdollInstance")?;
        o.bytes(0, 72)?;
        let skeleton = Skeleton::from_obj(o.ptr_required(64, "ragdoll skeleton")?)?;
        let mut body_indices = BTreeMap::new();
        let mut bodies = Vec::new();
        for body in o.array_ptrs(16)? {
            let body = body.ok_or(Error::NullPointer("ragdoll body"))?;
            let index = BodyIndex(bodies.len());
            if body_indices.insert(body.base, index).is_some() {
                return Err(Error::BadRagdoll("duplicate rigid body pointer".into()));
            }
            bodies.push(RigidBody::from_obj(body)?);
        }
        let map = o.array_i32(48)?;
        if map.len() != skeleton.bones.len() {
            return Err(Error::BadRagdoll(format!(
                "{} bones but {} bone map entries",
                skeleton.bones.len(),
                map.len()
            )));
        }
        let bone_to_body = map
            .into_iter()
            .map(|index| {
                if index == -1 {
                    Ok(None)
                } else if let Ok(index) = usize::try_from(index) {
                    if index < bodies.len() {
                        Ok(Some(BodyIndex(index)))
                    } else {
                        Err(Error::BadRagdoll(format!(
                            "body index {index} is out of range"
                        )))
                    }
                } else {
                    Err(Error::BadRagdoll(format!("invalid body index {index}")))
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let constraints = o
            .array_ptrs(32)?
            .into_iter()
            .map(|joint| {
                Constraint::from_obj(
                    joint.ok_or(Error::NullPointer("ragdoll constraint"))?,
                    &body_indices,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            skeleton,
            bodies,
            constraints,
            bone_to_body,
        })
    }
}

fn half(o: Obj<'_>, off: u32) -> Result<f32> {
    // FORMAT: hkHalf stores the upper 16 bits of an f32, not IEEE binary16.
    Ok(f32::from_bits(u32::from(o.u16(off)?) << 16))
}
