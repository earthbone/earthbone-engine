// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::panic)]

use soe_nif::{header::VERSION_20_2_0_7, Header};
use soe_physics::bhk::{self, Bhk};
use soe_physics::Vec3;

fn header() -> Header {
    Header {
        version: VERSION_20_2_0_7,
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

#[test]
fn fmt_21_non_unit_normal_scales_the_plane_constant_together() {
    let mut data = Vec::new();
    data.extend_from_slice(&7u32.to_le_bytes());
    data.extend_from_slice(&[0; 12]);
    for value in [
        0.0f32, 0.0, 2.0, 4.0, 3.0, 3.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    let Bhk::Plane {
        normal,
        distance,
        half_extents,
        center,
        ..
    } = bhk::decode("bhkPlaneShape", &data, &header()).unwrap()
    else {
        panic!()
    };
    assert_eq!(normal, Vec3::new(0.0, 0.0, 2.0));
    assert_eq!(distance / normal.length(), 2.0);
    assert_eq!(half_extents, Vec3::new(3.0, 3.0, 3.0));
    assert_eq!(center, Vec3::ZERO);
}
