// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Synthetic and installed-file checks for the serde-hkx ragdoll layouts.

use soe_hkx::ragdoll::{BodyIndex, ConstraintKind, MotionType, Ragdoll, Shape, Transform};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Error, Packfile, QsTransform};

struct Fixture {
    builder: PackfileBuilder,
    ragdoll: u32,
    bodies: [u32; 2],
    capsule: u32,
    joints: [u32; 2],
}

fn transform(b: &mut PackfileBuilder, object: u32, offset: u32, position: [f32; 4]) {
    b.set_vec4(object, offset, [1.0, 0.0, 0.0, 0.0]);
    b.set_vec4(object, offset + 16, [0.0, 1.0, 0.0, 0.0]);
    b.set_vec4(object, offset + 32, [0.0, 0.0, 1.0, 0.0]);
    b.set_vec4(object, offset + 48, position);
}

impl Fixture {
    fn new() -> Self {
        let mut b = PackfileBuilder::new();
        let root = b.object("hkRootLevelContainer", 16);
        let ragdoll = b.object("hkaRagdollInstance", 72);
        let variants = b.array_structs(root, 0, 24, 1);
        b.string(variants, 0, "RagdollInstance");
        b.string(variants, 8, "hkaRagdollInstance");
        b.ptr(variants, 16, ragdoll);
        let skeleton = b.object("hkaSkeleton", 120);
        b.string(skeleton, 16, "Test ragdoll");
        let bones = b.array_structs(skeleton, 40, 16, 3);
        for (i, name) in ["pelvis", "unmapped", "leg"].iter().enumerate() {
            b.string(bones, i as u32 * 16, name);
        }
        b.array_i16(skeleton, 24, &[-1, 0, 0]);
        b.array_qs_transform(skeleton, 56, &[QsTransform::IDENTITY; 3]);
        b.ptr(ragdoll, 64, skeleton);
        b.array_i32(ragdoll, 48, &[1, -1, 0]);
        let capsule = b.object("hkpCapsuleShape", 80);
        b.set_f32(capsule, 32, 0.25);
        b.set_vec4(capsule, 48, [0.0, 0.0, -2.0, 0.0]);
        b.set_vec4(capsule, 64, [0.0, 0.0, 2.0, 0.0]);
        let bodies = [b.object("hkpRigidBody", 720), b.object("hkpRigidBody", 720)];
        for (i, body) in bodies.iter().enumerate() {
            b.string(*body, 176, ["leg body", "pelvis body"][i]);
            b.ptr(*body, 32, capsule);
            b.set_u32(*body, 76, 0x1234_5678);
            b.set_u8(*body, 208, 1);
            b.set_f32(*body, 212, 0.75);
            b.set_f32(*body, 216, 0.125);
            b.set_u8(*body, 352, 3);
            transform(&mut b, *body, 368, [10.0, 20.0, 30.0, 1.0]);
            b.set_vec4(*body, 496, [0.5, 0.25, -0.5, 0.0]);
            b.set_vec4(*body, 544, [0.5, 0.25, 0.125, 0.1]);
            b.set_vec4(*body, 560, [1.0, 2.0, 3.0, 0.0]);
            b.set_u16(*body, 532, 0x3f00);
            b.set_u16(*body, 534, 0x3e80);
            b.set_u16(*body, 536, 0x3f80);
            b.set_u16(*body, 642, 0x3f80);
        }
        b.array_ptrs(ragdoll, 16, &bodies);
        let rag_data = b.object("hkpRagdollConstraintData", 384);
        transform(&mut b, rag_data, 48, [1.0, 2.0, 3.0, 0.0]);
        transform(&mut b, rag_data, 112, [4.0, 5.0, 6.0, 0.0]);
        b.set_bool(rag_data, 194, true);
        b.set_bool(rag_data, 290, true);
        b.set_u8(rag_data, 292, 3);
        b.set_f32(rag_data, 296, 2.5);
        for (off, min, max) in [(300, -0.2, 0.3), (320, -0.4, 0.5), (340, -0.6, 0.7)] {
            b.set_bool(rag_data, off + 2, true);
            b.set_f32(rag_data, off + 8, min);
            b.set_f32(rag_data, off + 12, max);
            b.set_f32(rag_data, off + 16, 0.8);
        }
        b.set_f32(rag_data, 368, 90.0);
        let motor = b.object("hkpPositionConstraintMotor", 48);
        for (off, value) in [
            (24, -10.0),
            (28, 20.0),
            (32, 0.6),
            (36, 0.8),
            (40, 1.2),
            (44, 0.4),
        ] {
            b.set_f32(motor, off, value);
        }
        b.ptr(rag_data, 256, motor);
        b.ptr(rag_data, 272, motor);
        let hinge_data = b.object("hkpLimitedHingeConstraintData", 272);
        transform(&mut b, hinge_data, 48, [7.0, 8.0, 9.0, 0.0]);
        transform(&mut b, hinge_data, 112, [10.0, 11.0, 12.0, 0.0]);
        b.set_bool(hinge_data, 194, true);
        b.set_f32(hinge_data, 204, 0.45);
        b.ptr(hinge_data, 208, motor);
        b.set_f32(hinge_data, 224, 3.5);
        b.set_bool(hinge_data, 230, true);
        b.set_f32(hinge_data, 232, -0.8);
        b.set_f32(hinge_data, 236, 1.2);
        b.set_f32(hinge_data, 240, 0.9);
        let joints = [
            b.object("hkpConstraintInstance", 112),
            b.object("hkpConstraintInstance", 112),
        ];
        b.ptr(joints[0], 24, rag_data);
        b.ptr(joints[0], 40, bodies[1]);
        b.ptr(joints[0], 48, bodies[0]);
        b.ptr(joints[1], 24, hinge_data);
        b.ptr(joints[1], 40, bodies[0]);
        b.array_ptrs(ragdoll, 32, &joints);
        Self {
            builder: b,
            ragdoll,
            bodies,
            capsule,
            joints,
        }
    }

    fn read(self) -> Result<Ragdoll, Error> {
        let pf = Packfile::parse(&self.builder.finish())?;
        Ragdoll::from_packfile(&pf)
    }
}

#[test]
fn ragdoll_preserves_reordered_bone_map_and_unmapped_bone() {
    let rag = Fixture::new().read().unwrap();
    assert_eq!(
        rag.bone_to_body,
        [Some(BodyIndex(1)), None, Some(BodyIndex(0))]
    );
    assert_eq!(
        rag.bodies[rag.bone_to_body[0].unwrap().0].name,
        "pelvis body"
    );
}

#[test]
fn body_decodes_mass_material_motion_and_capsule() {
    let rag = Fixture::new().read().unwrap();
    let body = &rag.bodies[0];
    assert_eq!(body.collision_filter_info, 0x1234_5678);
    assert_eq!(body.material.friction, 0.75);
    assert_eq!(body.material.restitution, 0.125);
    assert_eq!(body.motion.kind, MotionType::BoxInertia);
    assert_eq!(
        body.motion.inertia_and_mass_inverse,
        [0.5, 0.25, 0.125, 0.1]
    );
    assert_eq!(body.motion.center_of_mass_local, [0.5, 0.25, -0.5, 0.0]);
    assert_eq!(body.motion.transform.translation, [10.0, 20.0, 30.0, 1.0]);
    assert_eq!(body.motion.linear_damping, 0.5);
    assert_eq!(body.motion.angular_damping, 0.25);
    assert_eq!(body.motion.gravity_factor, 1.0);
    assert_eq!(
        body.shape,
        Shape::Capsule {
            radius: 0.25,
            a: [0.0, 0.0, -2.0, 0.0],
            b: [0.0, 0.0, 2.0, 0.0],
        }
    );
}

#[test]
fn ragdoll_joint_decodes_distinct_frames_limits_friction_and_motor_slots() {
    let rag = Fixture::new().read().unwrap();
    let joint = &rag.constraints[0];
    assert_eq!(joint.bodies, [Some(BodyIndex(1)), Some(BodyIndex(0))]);
    let ConstraintKind::Ragdoll(joint) = &joint.kind else {
        panic!("ragdoll joint expected")
    };
    assert_eq!(joint.frames[0].translation, [1.0, 2.0, 3.0, 0.0]);
    assert_eq!(joint.frames[1].translation, [4.0, 5.0, 6.0, 0.0]);
    assert_eq!((joint.twist.min_angle, joint.twist.max_angle), (-0.2, 0.3));
    assert_eq!((joint.cone.min_angle, joint.cone.max_angle), (-0.4, 0.5));
    assert_eq!(
        (joint.planes.min_angle, joint.planes.max_angle),
        (-0.6, 0.7)
    );
    assert_eq!(joint.friction.max_torque, 2.5);
    assert_eq!(joint.ball_socket.max_impulse, 90.0);
    assert!(joint.motors.enabled);
    assert_eq!(joint.motors.motors[0].as_ref().unwrap().max_force, 20.0);
    assert_eq!(joint.motors.motors[1], None);
    assert_eq!(joint.motors.motors[0], joint.motors.motors[2]);
}

#[test]
fn limited_hinge_preserves_world_endpoint_and_motor_target() {
    let rag = Fixture::new().read().unwrap();
    let joint = &rag.constraints[1];
    assert_eq!(joint.bodies, [Some(BodyIndex(0)), None]);
    let ConstraintKind::LimitedHinge(joint) = &joint.kind else {
        panic!("hinge expected")
    };
    assert_eq!((joint.limit.min_angle, joint.limit.max_angle), (-0.8, 1.2));
    assert_eq!(joint.motor.target_angle, 0.45);
    assert_eq!(joint.motor.motor.as_ref().unwrap().min_force, -10.0);
    assert_eq!(joint.friction.max_torque, 3.5);
}

#[test]
fn translated_hull_unpacks_vector_groups_without_padding_vertices() {
    let mut f = Fixture::new();
    let hull = f.builder.object("hkpConvexVerticesShape", 144);
    f.builder.set_i32(hull, 96, 3);
    let group = f.builder.array_structs(hull, 80, 48, 1);
    f.builder.set_vec4(group, 0, [1.0, 2.0, 3.0, 999.0]);
    f.builder.set_vec4(group, 16, [4.0, 5.0, 6.0, 999.0]);
    f.builder.set_vec4(group, 32, [7.0, 8.0, 9.0, 999.0]);
    let shape = f.builder.object("hkpConvexTranslateShape", 80);
    f.builder.set_vec4(shape, 64, [10.0, 11.0, 12.0, 0.0]);
    f.builder.ptr(shape, 48, hull);
    f.builder.ptr(f.bodies[0], 32, shape);
    let rag = f.read().unwrap();
    let Shape::Translated {
        translation, child, ..
    } = &rag.bodies[0].shape
    else {
        panic!("translated shape")
    };
    assert_eq!(*translation, [10.0, 11.0, 12.0, 0.0]);
    let Shape::ConvexHull(hull) = child.as_ref() else {
        panic!("convex hull")
    };
    assert_eq!(
        hull.vertices,
        [[1.0, 4.0, 7.0], [2.0, 5.0, 8.0], [3.0, 6.0, 9.0]]
    );
}

#[test]
fn malformed_bone_maps_are_rejected() {
    for map in [&[0, 1][..], &[0, 1, 2], &[0, 1, -2]] {
        let mut f = Fixture::new();
        f.builder.array_i32(f.ragdoll, 48, map);
        assert!(matches!(f.read().unwrap_err(), Error::BadRagdoll(_)));
    }
}

#[test]
fn null_body_slot_is_rejected_without_reindexing_later_bodies() {
    let mut f = Fixture::new();
    f.builder.array_bytes(f.ragdoll, 16, &[0; 16], 2);
    assert_eq!(f.read().unwrap_err(), Error::NullPointer("ragdoll body"));
}

#[test]
fn duplicate_bodies_and_external_constraint_endpoints_are_rejected() {
    let mut duplicate = Fixture::new();
    duplicate.builder.array_ptrs(
        duplicate.ragdoll,
        16,
        &[duplicate.bodies[0], duplicate.bodies[0]],
    );
    assert!(matches!(
        duplicate.read().unwrap_err(),
        Error::BadRagdoll(_)
    ));
    let mut external = Fixture::new();
    let body = external.builder.object("hkpRigidBody", 720);
    external.builder.ptr(external.joints[0], 40, body);
    assert!(matches!(external.read().unwrap_err(), Error::BadRagdoll(_)));
}

#[test]
fn cyclic_shape_is_rejected() {
    let mut f = Fixture::new();
    let shape = f.builder.object("hkpConvexTranslateShape", 80);
    f.builder.ptr(shape, 48, shape);
    f.builder.ptr(f.bodies[0], 32, shape);
    assert!(matches!(f.read().unwrap_err(), Error::BadRagdoll(_)));
}

#[test]
fn unsupported_shape_and_joint_are_reported() {
    let mut f = Fixture::new();
    let shape = f.builder.object("CustomShape", 80);
    f.builder.ptr(f.bodies[0], 32, shape);
    assert_eq!(
        f.read().unwrap_err(),
        Error::Unsupported("ragdoll shape CustomShape".into())
    );
    let mut f = Fixture::new();
    let joint = f.builder.object("CustomConstraintData", 32);
    f.builder.ptr(f.joints[0], 24, joint);
    assert_eq!(
        f.read().unwrap_err(),
        Error::Unsupported("ragdoll constraint CustomConstraintData".into())
    );
}

#[test]
fn invalid_hull_count_is_rejected() {
    let mut f = Fixture::new();
    let hull = f.builder.object("hkpConvexVerticesShape", 144);
    f.builder.set_i32(hull, 96, 5);
    f.builder.array_structs(hull, 80, 48, 1);
    f.builder.ptr(f.bodies[0], 32, hull);
    assert!(matches!(f.read().unwrap_err(), Error::BadRagdoll(_)));
}

#[test]
fn truncated_shape_is_an_error() {
    let mut f = Fixture::new();
    let shape = f.builder.object("hkpCapsuleShape", 16);
    f.builder.ptr(f.bodies[0], 32, shape);
    assert!(matches!(f.read().unwrap_err(), Error::Truncated { .. }));
}

#[test]
fn wrong_body_class_and_missing_ragdoll_are_errors() {
    let mut f = Fixture::new();
    f.builder.array_ptrs(f.ragdoll, 16, &[f.capsule]);
    assert!(matches!(f.read().unwrap_err(), Error::WrongClass { .. }));
    let mut b = PackfileBuilder::new();
    b.object("hkRootLevelContainer", 16);
    let pf = Packfile::parse(&b.finish()).unwrap();
    assert_eq!(
        Ragdoll::from_packfile(&pf).unwrap_err(),
        Error::NullPointer("hkaRagdollInstance")
    );
}

fn archive() -> Option<soe_bsa::Archive> {
    let data = std::env::var_os("SKYRIM_DATA")?;
    Some(
        soe_bsa::Archive::open(&std::path::PathBuf::from(data).join("Skyrim - Animations.bsa"))
            .unwrap(),
    )
}

fn world_point(transform: &Transform, point: [f32; 4]) -> [f32; 3] {
    std::array::from_fn(|axis| {
        transform.translation[axis]
            + (0..3)
                .map(|i| transform.rotation[i][axis] * point[i])
                .sum::<f32>()
    })
}

#[test]
fn anm_06_vanilla_human_has_typed_bodies_and_constraints() {
    let Some(archive) = archive() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let bytes = archive
        .read("meshes/actors/character/character assets/skeleton.hkx")
        .unwrap();
    let pf = Packfile::parse(&bytes).unwrap();
    let rag = Ragdoll::from_packfile(&pf).unwrap();
    assert_eq!(rag.bodies.len(), 18);
    assert_eq!(rag.constraints.len(), 17);
    assert_eq!(rag.skeleton.bones.len(), 18);
    assert_eq!(rag.bodies[0].name, "Ragdoll_NPC COM [COM ]");
    assert!((rag.bodies[0].motion.inertia_and_mass_inverse[3] - 0.2).abs() < 1e-6);
    let mut counts = [0, 0];
    let mut worst_pivot_error = 0.0f32;
    for constraint in &rag.constraints {
        let frames = match &constraint.kind {
            ConstraintKind::Ragdoll(joint) => {
                counts[0] += 1;
                &joint.frames
            }
            ConstraintKind::LimitedHinge(joint) => {
                counts[1] += 1;
                &joint.frames
            }
        };
        let mut points = [[0.0; 3]; 2];
        for i in 0..2 {
            let body = &rag.bodies[constraint.bodies[i].unwrap().0];
            points[i] = world_point(&body.motion.transform, frames[i].translation);
        }
        let error = (0..3)
            .map(|i| (points[0][i] - points[1][i]).powi(2))
            .sum::<f32>()
            .sqrt();
        worst_pivot_error = worst_pivot_error.max(error);
        assert!(
            error < 0.05,
            "{} body-space pivots differ by {error}",
            constraint.name
        );
    }
    assert!(counts.iter().all(|count| *count > 0));
    println!(
        "human: 18 bodies, 17 constraints {counts:?}, maximum pivot error {worst_pivot_error}"
    );
}

#[test]
fn anm_06_all_vanilla_skeleton_ragdolls_decode() {
    let Some(archive) = archive() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let mut paths = archive
        .files()
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".hkx"))
        .collect::<Vec<_>>();
    paths.sort();
    let (mut ragdolls, mut bodies, mut constraints) = (0, 0, 0);
    for path in paths {
        let bytes = archive.read(path).unwrap();
        let pf = Packfile::parse(&bytes).unwrap();
        for object in pf.objects_of("hkaRagdollInstance") {
            let rag = Ragdoll::from_obj(object).unwrap_or_else(|error| panic!("{path}: {error}"));
            assert_eq!(rag.constraints.len() + 1, rag.bodies.len(), "{path}");
            assert_eq!(rag.bone_to_body.len(), rag.skeleton.bones.len(), "{path}");
            for body in &rag.bodies {
                assert!(
                    body.motion
                        .inertia_and_mass_inverse
                        .iter()
                        .all(|x| x.is_finite() && *x > 0.0),
                    "{path} {}",
                    body.name
                );
            }
            ragdolls += 1;
            bodies += rag.bodies.len();
            constraints += rag.constraints.len();
        }
    }
    assert!(ragdolls >= 40 && bodies >= 900 && constraints >= 850,
        "inventory unexpectedly small: {ragdolls} ragdolls, {bodies} bodies, {constraints} constraints");
    println!(
        "{ragdolls} ragdolls, {bodies} bodies, {constraints} typed constraints, no decode failures"
    );
}
