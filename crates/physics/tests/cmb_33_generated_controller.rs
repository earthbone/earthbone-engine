// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-33: a runtime-generated character controller capsule for a skeleton
//! with no authored `CHAR_CONTROLLER` phantom, sized by the actor's own
//! scale. `crates/game/tests/cmb_33_actor_controller_scale.rs` proves where
//! the scale comes from, `crates/godot-host/tests/cmb_33_actor_controller_geometry.rs`
//! ties both together end to end.

use soe_nif::{BlockRef, Header};
use soe_physics::{
    collect_blocks, controller_shape, generated_controller_shape, Collected, Iso, Phantom, Shape,
    UNITS_PER_METER,
};

fn header() -> Header {
    Header {
        version: soe_nif::header::VERSION_20_2_0_7,
        user_version: 12,
        bs_version: soe_nif::header::BS_SKYRIM_SE,
        author: String::new(),
        process_script: String::new(),
        export_script: String::new(),
        max_filepath: String::new(),
        block_types: Vec::new(),
        block_type_index: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        groups: Vec::new(),
    }
}

/// Havok-metre template dimensions `generated_controller_shape` scales from,
/// matching `TEMPLATE_CAPSULE_HEIGHT`/`TEMPLATE_CAPSULE_RADIUS` in
/// `crates/physics/src/shape.rs`, sourced from the HKX character metadata
/// survey (`docs/physics.md`, 353 installed `hkbCharacterData` objects).
const TEMPLATE_HEIGHT: f32 = 1.7;
const TEMPLATE_RADIUS: f32 = 0.4;

fn expected_capsule(scale: f32) -> (f32, f32, f32) {
    let height = TEMPLATE_HEIGHT * scale;
    let radius = TEMPLATE_RADIUS * scale;
    let top = height.max(radius * 2.0) - radius;
    (radius, radius, top)
}

#[test]
fn cmb_33_generated_capsule_scales_from_the_template_dimensions() {
    for scale in [1.0f32, 2.0, 0.5, 3.7] {
        let Shape::Capsule { a, b, radius } = generated_controller_shape(scale) else {
            panic!("generated_controller_shape must return a capsule");
        };
        let (expected_radius, a_z, b_z) = expected_capsule(scale);
        let tolerance = 0.0005 * UNITS_PER_METER;
        assert!(
            (radius - expected_radius * UNITS_PER_METER).abs() < tolerance,
            "scale {scale}: radius {radius} vs expected {}",
            expected_radius * UNITS_PER_METER
        );
        assert!((a.x).abs() < 0.0001 && (a.y).abs() < 0.0001);
        assert!((b.x).abs() < 0.0001 && (b.y).abs() < 0.0001);
        assert!(
            (a.z - a_z * UNITS_PER_METER).abs() < tolerance,
            "scale {scale}: lower cap z {} vs expected {}",
            a.z,
            a_z * UNITS_PER_METER
        );
        assert!(
            (b.z - b_z * UNITS_PER_METER).abs() < tolerance,
            "scale {scale}: upper cap z {} vs expected {}",
            b.z,
            b_z * UNITS_PER_METER
        );
    }
}

#[test]
fn cmb_33_generated_capsule_rejects_nonfinite_or_nonpositive_scale() {
    let baseline = generated_controller_shape(1.0);
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -1.0] {
        assert_eq!(
            generated_controller_shape(bad),
            baseline,
            "scale {bad} must fall back to 1.0 rather than produce degenerate geometry"
        );
    }
}

#[test]
fn cmb_33_controller_shape_falls_back_to_generated_when_no_authored_phantom() {
    let collected = Collected::default();
    assert!(!collected.has_authored_controller());
    let (iso, shape) = controller_shape(&collected, 2.0);
    assert_eq!(iso, Iso::IDENTITY);
    assert_eq!(shape, generated_controller_shape(2.0));
}

fn phantom_with_capsule(radius: f32, top: f32) -> Phantom {
    Phantom {
        node: BlockRef(0),
        node_name: "Bip01".into(),
        layer: soe_physics::Layer::CHAR_CONTROLLER,
        filter_flags: 0,
        group: 0,
        transform: Iso::IDENTITY,
        shape: Shape::Capsule {
            a: soe_physics::Vec3::new(0.0, 0.0, radius),
            b: soe_physics::Vec3::new(0.0, 0.0, top),
            radius,
        },
    }
}

#[test]
fn cmb_33_controller_shape_prefers_an_authored_phantom_over_generated() {
    let collected = Collected {
        phantoms: vec![phantom_with_capsule(30.0, 100.0)],
        ..Collected::default()
    };
    assert!(collected.has_authored_controller());
    let (iso, shape) = controller_shape(&collected, 5.0);
    assert_eq!(iso, Iso::IDENTITY);
    assert_eq!(shape, collected.phantoms[0].shape.clone());
    assert_ne!(
        shape,
        generated_controller_shape(5.0),
        "an authored phantom must win even when a generated shape would differ"
    );
}

#[test]
fn cmb_33_controller_shape_picks_the_largest_authored_phantom_by_extent() {
    let small = phantom_with_capsule(5.0, 20.0);
    let large = phantom_with_capsule(40.0, 180.0);
    for phantoms in [
        vec![small.clone(), large.clone()],
        vec![large.clone(), small.clone()],
    ] {
        let collected = Collected {
            phantoms,
            ..Collected::default()
        };
        let (_, shape) = controller_shape(&collected, 1.0);
        assert_eq!(shape, large.shape);
    }
}

/// An empty block list with no root decodes to no colliders and no
/// phantoms, the state a skeleton file that carries geometry but no
/// collision blocks at all would produce through the real parser.
#[test]
fn cmb_33_a_skeleton_with_no_collision_blocks_collects_to_no_authored_controller() {
    let out = collect_blocks(&header(), &[], &[]).unwrap();
    assert!(!out.has_authored_controller());
    let (iso, shape) = controller_shape(&out, 1.3);
    assert_eq!(iso, Iso::IDENTITY);
    assert_eq!(shape, generated_controller_shape(1.3));
}
