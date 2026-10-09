// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Independent sequence/controller fixtures from nifly Animation.hpp and
//! nif.xml, plus an installed mesh survey for object animation playback.

mod common;

use common::{Buf, NifWriter, IDENTITY};
use soe_nif::controller::{
    self, ControllerFlags, CycleType, Interpolation, Key, KeyGroup, TimeController, TransformData,
    TransformInterpolator,
};
use soe_nif::object_animation::{ObjectAnimation, PropertyChannel};
use soe_nif::Vec3;

fn fixture(manager: bool) -> NifWriter {
    let mut w = NifWriter::new(100);
    let root = w.reserve("NiNode");
    let target = w.reserve("NiNode");
    let controller = w.reserve(if manager {
        "NiMultiTargetTransformController"
    } else {
        "NiTransformController"
    });
    let data = w.transform_data_linear(&[], &[(0.0, [0.0; 3]), (2.0, [20.0, 0.0, 0.0])], &[]);
    let data = w.add("NiTransformData", data);
    let interp = w.transform_interpolator([0.0; 3], [1.0, 0.0, 0.0, 0.0], 1.0, data);
    let interp = w.add("NiTransformInterpolator", interp);
    let mut manager_ref = -1;
    if manager {
        manager_ref = w.reserve("NiControllerManager");
        let mut sequences = Vec::new();
        for (name, cycle) in [("Idle", 0), ("Open", 2), ("Reverse", 1)] {
            let mut b = Buf::default();
            b.u32(w.string(name)).u32(1).u32(1);
            b.i32(interp).i32(controller).u8(0);
            b.u32(w.string("Target"))
                .u32(u32::MAX)
                .u32(w.string("NiMultiTargetTransformController"));
            b.u32(u32::MAX).u32(u32::MAX);
            b.f32(1.0).i32(-1).u32(cycle).f32(1.0).f32(0.0).f32(2.0);
            b.i32(manager_ref).u32(u32::MAX).u16(0);
            sequences.push(w.add("NiControllerSequence", b.0));
        }
        let mut b = Buf::default();
        b.i32(-1)
            .u16(8)
            .f32(1.0)
            .f32(0.0)
            .f32(0.0)
            .f32(2.0)
            .i32(root);
        b.bool(false).refs(&sequences).i32(-1);
        w.fill(manager_ref, b.0);
        let mut b = Buf::default();
        b.i32(-1)
            .u16(40)
            .f32(1.0)
            .f32(0.0)
            .f32(0.0)
            .f32(2.0)
            .i32(root)
            .u16(1)
            .i32(target);
        w.fill(controller, b.0);
    } else {
        let b = w.transform_controller(-1, 8, 1.0, 0.0, 0.0, 2.0, target, interp);
        w.fill(controller, b);
    }
    let b = w.ni_node_with_controller("Target", [0.0; 3], IDENTITY, 1.0, &[], controller);
    w.fill(target, b);
    let b = w.ni_node_with_controller(
        "Root",
        [7.0, 0.0, 0.0],
        IDENTITY,
        1.0,
        &[target],
        manager_ref,
    );
    w.fill(root, b);
    w.roots = vec![root];
    w
}

#[test]
fn rnd_23_standalone_controllers_loop_and_compose_parent_placement() {
    let file = soe_nif::parse(&fixture(false).finish()).unwrap();
    let asset = ObjectAnimation::from_file(&file).unwrap();
    assert_eq!(asset.sample(0.5, None).transforms[1].translation.x, 12.0);
    assert_eq!(asset.sample(2.5, None).transforms[1].translation.x, 12.0);
}

#[test]
fn rnd_23_standalone_frequency_and_phase_set_the_authored_clock() {
    let mut w = fixture(false);
    w.blocks[2].1 = w.transform_controller(-1, 8, 2.0, 0.5, 0.0, 2.0, 1, 4);
    let file = soe_nif::parse(&w.finish()).unwrap();
    let asset = ObjectAnimation::from_file(&file).unwrap();
    assert_eq!(asset.sample(0.25, None).transforms[1].translation.x, 17.0);
    assert_eq!(asset.sample(1.25, None).transforms[1].translation.x, 17.0);
}

#[test]
fn rnd_23_inactive_standalone_and_manager_controllers_hold_authored_pose() {
    for manager in [false, true] {
        let mut w = fixture(manager);
        let kind = if manager {
            "NiControllerManager"
        } else {
            "NiTransformController"
        };
        let block = w.blocks.iter_mut().find(|(name, _)| name == kind).unwrap();
        block.1[4..6].copy_from_slice(&0_u16.to_le_bytes());
        let file = soe_nif::parse(&w.finish()).unwrap();
        let asset = ObjectAnimation::from_file(&file).unwrap();
        assert!(!asset.is_animated());
        assert_eq!(asset.sample(1.0, None).transforms[1].translation.x, 7.0);
    }
}

#[test]
fn rnd_23_quaternion_rotation_uses_slerp_and_emissive_color_uses_vector_keys() {
    let mut w = fixture(false);
    w.blocks[3].1 = w.transform_data_linear(
        &[
            (0.0, [1.0, 0.0, 0.0, 0.0]),
            (
                2.0,
                [
                    std::f32::consts::FRAC_1_SQRT_2,
                    0.0,
                    0.0,
                    std::f32::consts::FRAC_1_SQRT_2,
                ],
            ),
        ],
        &[],
        &[],
    );
    let mut b = Buf::default();
    b.u32(2)
        .u32(1)
        .f32(0.0)
        .vec3([0.0; 3])
        .f32(2.0)
        .vec3([1.0, 0.5, 0.25]);
    let data = w.add("NiPosData", b.0);
    let mut b = Buf::default();
    b.vec3([0.0; 3]).i32(data);
    let interp = w.add("NiPoint3Interpolator", b.0);
    let mut b = Buf::default();
    b.i32(-1)
        .u16(8)
        .f32(1.0)
        .f32(0.0)
        .f32(0.0)
        .f32(2.0)
        .i32(0)
        .i32(interp)
        .u32(0);
    w.add("BSEffectShaderPropertyColorController", b.0);
    let file = soe_nif::parse(&w.finish()).unwrap();
    let pose = ObjectAnimation::from_file(&file).unwrap().sample(1.0, None);
    let direction = pose.transforms[1].apply_direction(Vec3::new(1.0, 0.0, 0.0));
    assert!((direction.x - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);
    assert!((direction.y - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);
    assert_eq!(pose.properties[0].value, Vec3::new(0.5, 0.25, 0.125));
}

#[test]
fn rnd_23_manager_idle_and_named_sequences_obey_authored_cycles() {
    let file = soe_nif::parse(&fixture(true).finish()).unwrap();
    let asset = ObjectAnimation::from_file(&file).unwrap();
    assert_eq!(
        asset.sequence_names().collect::<Vec<_>>(),
        ["Idle", "Open", "Reverse"]
    );
    assert_eq!(asset.sample(2.5, None).transforms[1].translation.x, 12.0);
    assert_eq!(
        asset.sample(2.5, Some("open")).transforms[1].translation.x,
        27.0
    );
    assert_eq!(
        asset.sample(2.5, Some("Reverse")).transforms[1]
            .translation
            .x,
        22.0
    );
}

#[test]
fn rnd_23_cumulative_manager_accumulates_root_translation_across_loops() {
    let mut w = fixture(true);
    let name = w.string("Target");
    let manager = w
        .blocks
        .iter_mut()
        .find(|(kind, _)| kind == "NiControllerManager")
        .unwrap();
    manager.1[26] = 1;
    let sequence = w
        .blocks
        .iter_mut()
        .find(|(kind, _)| kind == "NiControllerSequence")
        .unwrap();
    sequence.1[69..73].copy_from_slice(&name.to_le_bytes());
    let file = soe_nif::parse(&w.finish()).unwrap();
    let asset = ObjectAnimation::from_file(&file).unwrap();
    assert_eq!(asset.sample(2.5, None).transforms[1].translation.x, 32.0);
    assert_eq!(asset.sample(4.5, None).transforms[1].translation.x, 52.0);
}

#[test]
fn rnd_23_invalid_transform_defaults_retain_authored_channels() {
    let mut w = fixture(false);
    w.blocks[4].1 = w.transform_interpolator([-f32::MAX; 3], [-f32::MAX; 4], -f32::MAX, -1);
    let file = soe_nif::parse(&w.finish()).unwrap();
    let pose = ObjectAnimation::from_file(&file).unwrap().sample(0.5, None);
    assert_eq!(pose.transforms[1].translation.x, 7.0);
    assert_eq!(pose.transforms[1].scale, 1.0);
    assert_eq!(pose.transforms[1].rotation.rows, IDENTITY);
}

#[test]
fn rnd_23_float_shader_controllers_animate_uv_alpha_and_emissive() {
    let mut w = fixture(false);
    let mut b = Buf::default();
    b.u32(2).u32(1).f32(0.0).f32(0.2).f32(2.0).f32(0.8);
    let data = w.add("NiFloatData", b.0);
    let mut b = Buf::default();
    b.f32(0.0).i32(data);
    let interp = w.add("NiFloatInterpolator", b.0);
    for variable in [0, 5, 6, 8, 7, 9] {
        let mut b = Buf::default();
        b.i32(-1)
            .u16(8)
            .f32(1.0)
            .f32(0.0)
            .f32(0.0)
            .f32(2.0)
            .i32(0)
            .i32(interp)
            .u32(variable);
        w.add("BSEffectShaderPropertyFloatController", b.0);
    }
    let mut b = Buf::default();
    b.i32(-1)
        .u16(8)
        .f32(1.0)
        .f32(0.0)
        .f32(0.0)
        .f32(2.0)
        .i32(0)
        .i32(interp);
    w.add("NiAlphaController", b.0);
    for variable in [11, 12, 20, 21, 22, 23] {
        let mut b = Buf::default();
        b.i32(-1)
            .u16(8)
            .f32(1.0)
            .f32(0.0)
            .f32(0.0)
            .f32(2.0)
            .i32(0)
            .i32(interp)
            .u32(variable);
        w.add("BSLightingShaderPropertyFloatController", b.0);
    }
    for variable in 0..5 {
        let mut b = Buf::default();
        b.i32(-1)
            .u16(8)
            .f32(1.0)
            .f32(0.0)
            .f32(0.0)
            .f32(2.0)
            .i32(0)
            .i32(interp)
            .bool(false)
            .u32(0)
            .u32(variable);
        w.add("NiTextureTransformController", b.0);
    }
    let file = soe_nif::parse(&w.finish()).unwrap();
    let asset = ObjectAnimation::from_file(&file).unwrap();
    for channel in [
        PropertyChannel::EmissiveMultiple,
        PropertyChannel::Alpha,
        PropertyChannel::UOffset,
        PropertyChannel::VOffset,
        PropertyChannel::UScale,
        PropertyChannel::VScale,
        PropertyChannel::UvRotation,
    ] {
        let sampled = asset.sample(1.0, None);
        let value = sampled
            .properties
            .iter()
            .find(|p| p.channel == channel)
            .unwrap();
        assert!((value.value.x - 0.5).abs() < 1e-6, "{channel:?}");
    }
    assert_eq!(asset.sample(1.0, None).properties.len(), 18);
    assert!(asset
        .sample(1.0, None)
        .properties
        .iter()
        .all(|p| { (p.value.x - 0.5).abs() < 1e-6 }));
}

#[test]
fn rnd_23_malformed_sequence_counts_links_and_trailing_bytes_are_errors() {
    for mutation in [0, 1, 2] {
        let mut w = fixture(true);
        let seq = w
            .blocks
            .iter_mut()
            .find(|(kind, _)| kind == "NiControllerSequence")
            .unwrap();
        match mutation {
            0 => seq.1[4..8].copy_from_slice(&u32::MAX.to_le_bytes()),
            1 => seq.1[12..16].copy_from_slice(&123456_i32.to_le_bytes()),
            _ => seq.1.push(0),
        }
        let file = soe_nif::parse(&w.finish()).unwrap();
        assert!(
            ObjectAnimation::from_file(&file).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn rnd_23_quadratic_tbc_constant_rotation_and_backwards_interpolate() {
    let mut time = TimeController {
        frequency: 1.0,
        stop_time: 2.0,
        flags: ControllerFlags {
            active: true,
            cycle_type: CycleType::Clamp,
            ..Default::default()
        },
        ..Default::default()
    };
    let interp = TransformInterpolator::default();
    let mut data = TransformData {
        translations: KeyGroup {
            interpolation: Interpolation::Quadratic,
            keys: vec![
                Key {
                    time: 0.0,
                    value: Vec3::ZERO,
                    out_tan: Vec3::new(20.0, 0.0, 0.0),
                    ..Default::default()
                },
                Key {
                    time: 2.0,
                    value: Vec3::new(20.0, 0.0, 0.0),
                    ..Default::default()
                },
            ],
        },
        ..Default::default()
    };
    assert_eq!(
        controller::evaluate_transform(&time, &interp, Some(&data), 1.0)
            .translation
            .x,
        12.5
    );
    data.translations.interpolation = Interpolation::Tbc;
    for key in &mut data.translations.keys {
        key.tbc.tension = 1.0;
    }
    assert_eq!(
        controller::evaluate_transform(&time, &interp, Some(&data), 0.5)
            .translation
            .x,
        3.125
    );
    data.translations.interpolation = Interpolation::Constant;
    assert_eq!(
        controller::evaluate_transform(&time, &interp, Some(&data), 0.5)
            .translation
            .x,
        0.0
    );
    time.flags.play_backwards = true;
    assert_eq!(
        controller::evaluate_transform(&time, &interp, Some(&data), 0.5)
            .translation
            .x,
        20.0
    );
}

#[test]
fn rnd_23_vanilla_banners_wheels_flames_and_containers_have_changing_poses() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let mut sampled = std::collections::BTreeMap::<String, usize>::new();
    for name in ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"] {
        let archive = soe_bsa::Archive::open(&std::path::Path::new(&data).join(name)).unwrap();
        for path in archive.files().iter().filter(|p| {
            std::path::Path::new(p)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("nif"))
                && [
                    "banner",
                    "windmill",
                    "waterwheel",
                    "candle",
                    "chest",
                    "lever",
                ]
                .iter()
                .any(|part| p.contains(part))
        }) {
            let file = soe_nif::parse(&archive.read(path).unwrap()).unwrap();
            let asset = ObjectAnimation::from_file(&file).unwrap_or_else(|e| panic!("{path}: {e}"));
            if !asset.is_animated() {
                continue;
            }
            if path.contains("whiterun") && path.contains("banner") {
                println!("rnd_23 Whiterun capture model: {path}");
            }
            let names: Vec<_> = asset.sequence_names().collect();
            let changed = std::iter::once(None)
                .chain(names.into_iter().map(Some))
                .any(|name| asset.sample(0.1, name) != asset.sample(0.7, name));
            if changed {
                for category in [
                    "banner",
                    "windmill",
                    "waterwheel",
                    "candle",
                    "chest",
                    "lever",
                ] {
                    if path.contains(category) {
                        if !sampled.contains_key(category) {
                            println!("rnd_23 {category} example: {path}");
                        }
                        *sampled.entry(category.to_owned()).or_default() += 1;
                    }
                }
            }
        }
    }
    println!("rnd_23 changing vanilla assets: {sampled:?}");
    for category in [
        "banner",
        "windmill",
        "waterwheel",
        "candle",
        "chest",
        "lever",
    ] {
        assert!(
            sampled.get(category).copied().unwrap_or(0) > 0,
            "no animated {category}"
        );
    }
}

#[test]
fn rnd_23_all_installed_animation_blocks_decode_without_losing_static_models() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let mut count = 0;
    for name in ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"] {
        let archive = soe_bsa::Archive::open(&std::path::Path::new(&data).join(name)).unwrap();
        for path in archive.files().iter().filter(|p| {
            std::path::Path::new(p)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("nif"))
        }) {
            let file = soe_nif::parse(&archive.read(path).unwrap()).unwrap();
            ObjectAnimation::from_file(&file).unwrap_or_else(|e| panic!("{path}: {e}"));
            count += 1;
        }
    }
    assert!(count > 20000, "survey must cover the vanilla archives");
    println!("rnd_23 animation block survey: {count} NIFs");
}
