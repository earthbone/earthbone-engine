// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial RND-13 checks for authored material controller behavior.

use soe_nif::blocks::Block;
use soe_nif::controller::{
    ControllerFlags, CycleType, Interpolation, Key, KeyGroup, MaterialColorController,
    MaterialColorTarget, MaterialColors, Point3Interpolator, Tbc, TimeController,
};
use soe_nif::types::{BlockRef, Vec3};
use soe_nif::{Header, NifFile};

fn file(active: bool, target: MaterialColorTarget) -> NifFile {
    let time = TimeController {
        flags: ControllerFlags {
            active,
            cycle_type: CycleType::Clamp,
            ..Default::default()
        },
        frequency: 1.0,
        start_time: 0.0,
        stop_time: 1.0,
        target: BlockRef(2),
        ..Default::default()
    };
    let track = KeyGroup {
        interpolation: Interpolation::Linear,
        keys: vec![Key {
            time: 0.0,
            value: Vec3::new(0.9, 0.8, 0.7),
            in_tan: Vec3::ZERO,
            out_tan: Vec3::ZERO,
            tbc: Tbc::default(),
        }],
    };
    NifFile {
        header: Header {
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
        },
        blocks: vec![
            Block::MaterialColorController(MaterialColorController {
                time,
                interpolator: BlockRef(1),
                target_color: target,
            }),
            Block::Point3Interpolator(Point3Interpolator {
                value: Vec3::new(0.1, 0.2, 0.3),
                data: BlockRef(2),
            }),
            Block::PosData(track),
        ],
        roots: Vec::new(),
    }
}

fn colors() -> MaterialColors {
    MaterialColors {
        ambient: Vec3::new(0.1, 0.2, 0.3),
        diffuse: Vec3::new(0.2, 0.3, 0.4),
        specular: Vec3::new(0.3, 0.4, 0.5),
        self_illumination: Vec3::new(0.4, 0.5, 0.6),
    }
}

#[test]
fn rnd_13_inactive_controller_does_not_change_material() {
    let file = file(false, MaterialColorTarget::Diffuse);
    let mut material = colors();
    let initial = material;
    assert!(!file.apply_material_color_controller(BlockRef(0), &mut material, 0.0));
    assert_eq!(material, initial);
}

#[test]
fn rnd_13_missing_controller_does_not_change_material() {
    let file = file(true, MaterialColorTarget::Ambient);
    let mut material = colors();
    let initial = material;
    assert!(!file.apply_material_color_controller(BlockRef(99), &mut material, 0.0));
    assert_eq!(material, initial);
}

#[test]
fn rnd_13_sampled_target_preserves_unselected_channels() {
    for target in [
        MaterialColorTarget::Ambient,
        MaterialColorTarget::Diffuse,
        MaterialColorTarget::Specular,
        MaterialColorTarget::SelfIllumination,
    ] {
        let file = file(true, target);
        let mut material = colors();
        let before = material;
        assert!(file.apply_material_color_controller(BlockRef(0), &mut material, 0.0));
        let expected = Vec3::new(0.9, 0.8, 0.7);
        match target {
            MaterialColorTarget::Ambient => assert_eq!(material.ambient, expected),
            MaterialColorTarget::Diffuse => assert_eq!(material.diffuse, expected),
            MaterialColorTarget::Specular => assert_eq!(material.specular, expected),
            MaterialColorTarget::SelfIllumination => {
                assert_eq!(material.self_illumination, expected);
            }
        }
        if target != MaterialColorTarget::Ambient {
            assert_eq!(material.ambient, before.ambient);
        }
        if target != MaterialColorTarget::Diffuse {
            assert_eq!(material.diffuse, before.diffuse);
        }
        if target != MaterialColorTarget::Specular {
            assert_eq!(material.specular, before.specular);
        }
        if target != MaterialColorTarget::SelfIllumination {
            assert_eq!(material.self_illumination, before.self_illumination);
        }
    }
}
