// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Check for RND-13: a sampled `NiMaterialColorController` track changes only
//! its selected material color channel. Field meaning follows
//! `nifxml/nif.xml` (`NiMaterialColorController`, `MaterialColor`) and
//! controller clock behavior follows OpenMW's
//! `components/nifosg/controller.cpp`, both read from `engine-refs`.

use soe_nif::controller::{
    ControllerFlags, CycleType, Interpolation, Key, KeyGroup, MaterialColorController,
    MaterialColorTarget, MaterialColors, Point3Interpolator, Tbc, TimeController,
};
use soe_nif::types::{BlockRef, Vec3};
use soe_nif::{blocks::Block, Header, NifFile};

fn controller() -> TimeController {
    TimeController {
        next_controller: BlockRef::NONE,
        flags: ControllerFlags {
            cycle_type: CycleType::Clamp,
            active: true,
            ..Default::default()
        },
        frequency: 1.0,
        phase: 0.0,
        start_time: 0.0,
        stop_time: 10.0,
        target: BlockRef(9),
    }
}

fn key(time: f32, value: Vec3) -> Key<Vec3> {
    Key {
        time,
        value,
        in_tan: Vec3::ZERO,
        out_tan: Vec3::ZERO,
        tbc: Tbc::default(),
    }
}

fn material() -> MaterialColors {
    MaterialColors {
        ambient: Vec3::new(0.1, 0.2, 0.3),
        diffuse: Vec3::new(0.2, 0.3, 0.4),
        specular: Vec3::new(0.3, 0.4, 0.5),
        self_illumination: Vec3::new(0.4, 0.5, 0.6),
    }
}

fn assert_vec_close(actual: Vec3, expected: Vec3) {
    assert!((actual.x - expected.x).abs() < 1e-5, "{actual:?}");
    assert!((actual.y - expected.y).abs() < 1e-5, "{actual:?}");
    assert!((actual.z - expected.z).abs() < 1e-5, "{actual:?}");
}

fn file_with_material_controller(track: KeyGroup<Vec3>) -> NifFile {
    let mut time = controller();
    time.target = BlockRef(3);
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
                target_color: MaterialColorTarget::SelfIllumination,
            }),
            Block::Point3Interpolator(Point3Interpolator {
                value: Vec3::new(0.7, 0.8, 0.9),
                data: BlockRef(2),
            }),
            Block::PosData(track),
            Block::Unknown {
                type_name: "NiMaterialProperty".to_string(),
                data: Vec::new(),
            },
        ],
        roots: Vec::new(),
    }
}

#[test]
fn rnd_13_authored_material_controller_changes_only_its_target_channel() {
    let track = KeyGroup {
        interpolation: Interpolation::Linear,
        keys: vec![
            key(0.0, Vec3::new(0.0, 0.1, 0.2)),
            key(10.0, Vec3::new(1.0, 0.9, 0.8)),
        ],
    };
    let mut colors = material();
    let unchanged = colors;
    let file = file_with_material_controller(track);

    assert!(file.apply_material_color_controller(BlockRef(0), &mut colors, 0.0));
    assert_eq!(colors.self_illumination, Vec3::new(0.0, 0.1, 0.2));
    assert_eq!(colors.ambient, unchanged.ambient);
    assert_eq!(colors.diffuse, unchanged.diffuse);
    assert_eq!(colors.specular, unchanged.specular);

    assert!(file.apply_material_color_controller(BlockRef(0), &mut colors, 5.0));
    assert_vec_close(colors.self_illumination, Vec3::new(0.5, 0.5, 0.5));
    assert_eq!(colors.ambient, unchanged.ambient);
    assert_eq!(colors.diffuse, unchanged.diffuse);
    assert_eq!(colors.specular, unchanged.specular);

    assert!(file.apply_material_color_controller(BlockRef(0), &mut colors, 10.0));
    assert_eq!(colors.self_illumination, Vec3::new(1.0, 0.9, 0.8));
    assert_eq!(colors.ambient, unchanged.ambient);
    assert_eq!(colors.diffuse, unchanged.diffuse);
    assert_eq!(colors.specular, unchanged.specular);
}

#[test]
fn rnd_13_material_controller_without_keys_applies_interpolator_default_only() {
    let mut colors = material();
    let initial = colors;
    let empty = KeyGroup::<Vec3>::default();
    let file = file_with_material_controller(empty);

    assert!(file.apply_material_color_controller(BlockRef(0), &mut colors, 5.0));
    assert_eq!(colors.self_illumination, Vec3::new(0.7, 0.8, 0.9));
    assert_eq!(colors.ambient, initial.ambient);
    assert_eq!(colors.diffuse, initial.diffuse);
    assert_eq!(colors.specular, initial.specular);
}
