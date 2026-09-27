// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::panic)]

use soe_nif::{header::VERSION_20_2_0_7, BlockRef, Header};
use soe_physics::bhk::{self, Bhk};

fn header() -> Header {
    Header { version: VERSION_20_2_0_7, user_version: 12, bs_version: soe_nif::header::BS_SKYRIM_SE,
        author: String::new(), process_script: String::new(), export_script: String::new(), max_filepath: String::new(),
        block_types: Vec::new(), block_type_index: Vec::new(), block_sizes: Vec::new(), strings: Vec::new(), groups: Vec::new() }
}

#[test]
fn fmt_23_hinge_retains_motor_payload_bytes_and_signed_body_links() {
    let mut data = Vec::new();
    data.extend_from_slice(&2u32.to_le_bytes());
    data.extend_from_slice(&7i32.to_le_bytes());
    data.extend_from_slice(&23i32.to_le_bytes());
    data.extend_from_slice(&5u32.to_le_bytes());
    for _ in 0..8 { for value in [1.0f32, 0.0, 0.0, 0.0] { data.extend_from_slice(&value.to_le_bytes()); } }
    for value in [-1.0f32, 1.0, 0.0] { data.extend_from_slice(&value.to_le_bytes()); }
    data.push(2);
    let payload: Vec<u8> = (0..18).collect();
    data.extend_from_slice(&payload);
    let Bhk::LimitedHingeConstraint(hinge) = bhk::decode("bhkLimitedHingeConstraint", &data, &header()).unwrap() else { panic!() };
    assert_eq!(hinge.body_a, BlockRef(7));
    assert_eq!(hinge.body_b, BlockRef(23));
    assert_eq!(hinge.motor_type, 2);
    assert_eq!(hinge.motor_data, payload);
}

#[test]
fn fmt_23_only_the_nif_none_sentinel_is_a_negative_body_reference() {
    let mut data = Vec::new();
    data.extend_from_slice(&2u32.to_le_bytes());
    data.extend_from_slice(&(-2i32).to_le_bytes());
    data.extend_from_slice(&23i32.to_le_bytes());
    data.extend_from_slice(&5u32.to_le_bytes());
    for _ in 0..8 { for value in [1.0f32, 0.0, 0.0, 0.0] { data.extend_from_slice(&value.to_le_bytes()); } }
    for value in [-1.0f32, 1.0, 0.0] { data.extend_from_slice(&value.to_le_bytes()); }
    data.push(0);
    assert!(bhk::decode("bhkLimitedHingeConstraint", &data, &header()).is_err());
}
