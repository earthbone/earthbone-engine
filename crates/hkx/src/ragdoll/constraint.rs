// SPDX-License-Identifier: GPL-3.0-or-later

//! Ragdoll and limited-hinge atoms from serde-hkx's hkp constraint layouts.
//! The body-space frames and angular limits match activeragdoll's joint conversion.

use std::collections::BTreeMap;

use crate::{Error, Obj, Result};

use super::{BodyIndex, Transform};

/// A constraint instance whose entity references index its ragdoll's bodies.
#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    pub name: String,
    /// A null endpoint is a constraint to the world.
    pub bodies: [Option<BodyIndex>; 2],
    pub priority: u8,
    pub user_data: u64,
    pub data_user_data: u64,
    pub kind: ConstraintKind,
}

/// Joint classes used by Skyrim SE ragdolls.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintKind {
    Ragdoll(Box<RagdollJoint>),
    LimitedHinge(Box<LimitedHingeJoint>),
}

/// Cone, twist and plane limits with authored motor targets and joint frames.
#[derive(Debug, Clone, PartialEq)]
pub struct RagdollJoint {
    pub frames: [Transform; 2],
    pub stabilization: Stabilization,
    pub motors: RagdollMotors,
    pub friction: AngularFriction,
    pub twist: TwistLimit,
    pub cone: ConeLimit,
    pub planes: ConeLimit,
    pub ball_socket: BallSocket,
}

/// A hinge with an angular range, motor and body-space joint frames.
#[derive(Debug, Clone, PartialEq)]
pub struct LimitedHingeJoint {
    pub frames: [Transform; 2],
    pub stabilization: Stabilization,
    pub motor: HingeMotor,
    pub friction: AngularFriction,
    pub limit: AngularLimit,
    pub free_axis: u8,
    pub ball_socket: BallSocket,
}

/// Stabilization settings shared by the two joint classes.
#[derive(Debug, Clone, PartialEq)]
pub struct Stabilization {
    pub enabled: bool,
    pub max_angle: f32,
}

/// Linear pivot constraint settings, including authored impulse limits.
#[derive(Debug, Clone, PartialEq)]
pub struct BallSocket {
    pub solving_method: u8,
    pub bodies_to_notify: u8,
    pub velocity_stabilization_factor: u8,
    pub max_impulse: f32,
    pub inertia_stabilization_factor: f32,
}

/// The angular friction atom's axis range and maximum torque.
#[derive(Debug, Clone, PartialEq)]
pub struct AngularFriction {
    pub enabled: bool,
    pub first_axis: u8,
    pub axis_count: u8,
    pub max_torque: f32,
}

/// Ragdoll twist limits in radians.
#[derive(Debug, Clone, PartialEq)]
pub struct TwistLimit {
    pub enabled: bool,
    pub twist_axis: u8,
    pub reference_axis: u8,
    pub min_angle: f32,
    pub max_angle: f32,
    pub tau_factor: f32,
}

/// Ragdoll cone or plane limits in radians, with their measurement mode.
#[derive(Debug, Clone, PartialEq)]
pub struct ConeLimit {
    pub enabled: bool,
    pub twist_axis_in_a: u8,
    pub reference_axis_in_b: u8,
    pub measurement_mode: u8,
    pub min_angle: f32,
    pub max_angle: f32,
    pub tau_factor: f32,
}

/// Hinge angular limits in radians.
#[derive(Debug, Clone, PartialEq)]
pub struct AngularLimit {
    pub enabled: bool,
    pub axis: u8,
    pub min_angle: f32,
    pub max_angle: f32,
    pub tau_factor: f32,
}

/// An authored `hkpPositionConstraintMotor`.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionMotor {
    pub min_force: f32,
    pub max_force: f32,
    pub tau: f32,
    pub damping: f32,
    pub proportional_recovery_velocity: f32,
    pub constant_recovery_velocity: f32,
}

/// Three optional motors, in twist, plane, cone order.
#[derive(Debug, Clone, PartialEq)]
pub struct RagdollMotors {
    pub enabled: bool,
    pub target_rotation: [[f32; 4]; 3],
    pub motors: [Option<PositionMotor>; 3],
}

/// The limited hinge's target angle and optional motor.
#[derive(Debug, Clone, PartialEq)]
pub struct HingeMotor {
    pub enabled: bool,
    pub axis: u8,
    pub target_angle: f32,
    pub motor: Option<PositionMotor>,
}

impl Constraint {
    pub(super) fn from_obj(o: Obj<'_>, bodies: &BTreeMap<u32, BodyIndex>) -> Result<Self> {
        o.expect_class("hkpConstraintInstance")?;
        o.bytes(0, 112)?;
        if o.ptr(32)?.is_some() {
            return Err(Error::Unsupported(
                "ragdoll constraint modifier atoms".into(),
            ));
        }
        let endpoint = |off| {
            o.ptr(off)?
                .map(|body| {
                    bodies.get(&body.base).copied().ok_or_else(|| {
                        Error::BadRagdoll(format!(
                            "constraint entity at {} is outside the ragdoll",
                            body.base
                        ))
                    })
                })
                .transpose()
        };
        let data = o.ptr_required(24, "ragdoll constraint data")?;
        let kind = match data.class() {
            Some("hkpRagdollConstraintData") => {
                data.bytes(0, 384)?;
                ConstraintKind::Ragdoll(Box::new(RagdollJoint::from_obj(data.at(32))?))
            }
            Some("hkpLimitedHingeConstraintData") => {
                data.bytes(0, 272)?;
                ConstraintKind::LimitedHinge(Box::new(LimitedHingeJoint::from_obj(data.at(32))?))
            }
            class => {
                return Err(Error::Unsupported(format!(
                    "ragdoll constraint {}",
                    class.unwrap_or("<unnamed>")
                )))
            }
        };
        Ok(Self {
            name: o.string_or_empty(80)?,
            bodies: [endpoint(40)?, endpoint(48)?],
            priority: o.u8(56)?,
            user_data: o.u64(88)?,
            data_user_data: data.u64(16)?,
            kind,
        })
    }
}

impl RagdollJoint {
    fn from_obj(o: Obj<'_>) -> Result<Self> {
        Ok(Self {
            frames: [
                Transform::from_obj(o.at(16))?,
                Transform::from_obj(o.at(80))?,
            ],
            stabilization: Stabilization::from_obj(o.at(144))?,
            motors: RagdollMotors {
                enabled: o.bool(162)?,
                target_rotation: [o.vec4(176)?, o.vec4(192)?, o.vec4(208)?],
                motors: [motor(o, 224)?, motor(o, 232)?, motor(o, 240)?],
            },
            friction: AngularFriction::from_obj(o.at(256))?,
            twist: TwistLimit::from_obj(o.at(268))?,
            cone: ConeLimit::from_obj(o.at(288))?,
            planes: ConeLimit::from_obj(o.at(308))?,
            ball_socket: BallSocket::from_obj(o.at(328))?,
        })
    }
}

impl LimitedHingeJoint {
    fn from_obj(o: Obj<'_>) -> Result<Self> {
        Ok(Self {
            frames: [
                Transform::from_obj(o.at(16))?,
                Transform::from_obj(o.at(80))?,
            ],
            stabilization: Stabilization::from_obj(o.at(144))?,
            motor: HingeMotor {
                enabled: o.bool(162)?,
                axis: o.u8(163)?,
                target_angle: o.f32(172)?,
                motor: motor(o, 176)?,
            },
            friction: AngularFriction::from_obj(o.at(184))?,
            limit: AngularLimit {
                enabled: o.bool(198)?,
                axis: o.u8(199)?,
                min_angle: o.f32(200)?,
                max_angle: o.f32(204)?,
                tau_factor: o.f32(208)?,
            },
            free_axis: o.u8(214)?,
            ball_socket: BallSocket::from_obj(o.at(216))?,
        })
    }
}

impl Stabilization {
    fn from_obj(o: Obj<'_>) -> Result<Self> {
        Ok(Self {
            enabled: o.bool(2)?,
            max_angle: o.f32(4)?,
        })
    }
}

impl BallSocket {
    fn from_obj(o: Obj<'_>) -> Result<Self> {
        Ok(Self {
            solving_method: o.u8(2)?,
            bodies_to_notify: o.u8(3)?,
            velocity_stabilization_factor: o.u8(4)?,
            max_impulse: o.f32(8)?,
            inertia_stabilization_factor: o.f32(12)?,
        })
    }
}

impl AngularFriction {
    fn from_obj(o: Obj<'_>) -> Result<Self> {
        Ok(Self {
            enabled: o.bool(2)?,
            first_axis: o.u8(3)?,
            axis_count: o.u8(4)?,
            max_torque: o.f32(8)?,
        })
    }
}

impl TwistLimit {
    fn from_obj(o: Obj<'_>) -> Result<Self> {
        Ok(Self {
            enabled: o.bool(2)?,
            twist_axis: o.u8(3)?,
            reference_axis: o.u8(4)?,
            min_angle: o.f32(8)?,
            max_angle: o.f32(12)?,
            tau_factor: o.f32(16)?,
        })
    }
}

impl ConeLimit {
    fn from_obj(o: Obj<'_>) -> Result<Self> {
        Ok(Self {
            enabled: o.bool(2)?,
            twist_axis_in_a: o.u8(3)?,
            reference_axis_in_b: o.u8(4)?,
            measurement_mode: o.u8(5)?,
            min_angle: o.f32(8)?,
            max_angle: o.f32(12)?,
            tau_factor: o.f32(16)?,
        })
    }
}

fn motor(o: Obj<'_>, off: u32) -> Result<Option<PositionMotor>> {
    o.ptr(off)?
        .map(|m| {
            if m.class() != Some("hkpPositionConstraintMotor") {
                return Err(Error::Unsupported(format!(
                    "ragdoll motor {}",
                    m.class().unwrap_or("<unnamed>")
                )));
            }
            Ok(PositionMotor {
                min_force: m.f32(24)?,
                max_force: m.f32(28)?,
                tau: m.f32(32)?,
                damping: m.f32(36)?,
                proportional_recovery_velocity: m.f32(40)?,
                constant_recovery_velocity: m.f32(44)?,
            })
        })
        .transpose()
}
