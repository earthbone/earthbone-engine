// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::panic)]

use soe_nif::{header::VERSION_20_2_0_7, Header};
use soe_physics::bhk::{self, Bhk};
use soe_physics::Vec3;

fn header() -> Header {
    Header { version: VERSION_20_2_0_7, user_version: 12, bs_version: soe_nif::header::BS_SKYRIM_SE,
        author: String::new(), process_script: String::new(), export_script: String::new(), max_filepath: String::new(),
        block_types: Vec::new(), block_type_index: Vec::new(), block_sizes: Vec::new(), strings: Vec::new(), groups: Vec::new() }
}

#[test]
fn fmt_20_cylinder_keeps_vector4_xyz_and_adds_shell_to_outer_radius() {
    let mut data = Vec::new();
    data.extend_from_slice(&7u32.to_le_bytes());
    data.extend_from_slice(&0.1f32.to_le_bytes());
    data.extend_from_slice(&[0; 8]);
    for value in [1.0f32, 2.0, 3.0, 99.0, -2.0, 4.0, 6.0, -99.0] { data.extend_from_slice(&value.to_le_bytes()); }
    data.extend_from_slice(&0.5f32.to_le_bytes());
    data.extend_from_slice(&[0; 12]);
    let Bhk::Cylinder { shell_radius, radius, first, second, .. } =
        bhk::decode("bhkCylinderShape", &data, &header()).unwrap() else { panic!() };
    assert_eq!(first, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(second, Vec3::new(-2.0, 4.0, 6.0));
    assert!((radius + shell_radius - 0.6).abs() < 1e-6);
}
