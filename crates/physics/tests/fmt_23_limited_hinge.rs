// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_nif::{header::VERSION_20_2_0_7, BlockRef, Header};
use soe_physics::bhk::{self, Bhk};

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

struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }

    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn f32(&mut self, value: f32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn vec4(&mut self, values: [f32; 4]) {
        for value in values {
            self.f32(value);
        }
    }
}

fn limited_hinge_fixture() -> Vec<u8> {
    let mut writer = Writer(Vec::new());
    writer.u32(2);
    writer.i32(17);
    writer.i32(29);
    writer.u32(1);
    for xyz in [
        [1.0, 0.0, 0.0, 0.1],
        [0.0, 1.0, 0.0, 0.2],
        [0.0, 0.0, 1.0, 0.3],
        [1.0, 2.0, 3.0, 0.4],
        [0.0, 0.0, 1.0, 0.5],
        [1.0, 0.0, 0.0, 0.6],
        [0.0, 1.0, 0.0, 0.7],
        [-1.0, -2.0, -3.0, 0.8],
    ] {
        writer.vec4(xyz);
    }
    writer.f32(-0.75);
    writer.f32(1.25);
    writer.f32(10.0);
    writer.u8(0);
    writer.0
}

#[test]
fn fmt_23_limited_hinge_retains_body_links_frames_and_limits() {
    let data = limited_hinge_fixture();
    let Bhk::LimitedHingeConstraint(hinge) =
        bhk::decode("bhkLimitedHingeConstraint", &data, &header()).unwrap()
    else {
        panic!("limited hinge block did not decode as a constraint");
    };

    assert_eq!(hinge.body_a, BlockRef(17));
    assert_eq!(hinge.body_b, BlockRef(29));
    assert_eq!(hinge.priority, 1);
    assert_eq!(hinge.axis_a, [1.0, 0.0, 0.0, 0.1]);
    assert_eq!(hinge.perpendicular_a1, [0.0, 1.0, 0.0, 0.2]);
    assert_eq!(hinge.perpendicular_a2, [0.0, 0.0, 1.0, 0.3]);
    assert_eq!(hinge.pivot_a, [1.0, 2.0, 3.0, 0.4]);
    assert_eq!(hinge.axis_b, [0.0, 0.0, 1.0, 0.5]);
    assert_eq!(hinge.perpendicular_b1, [1.0, 0.0, 0.0, 0.6]);
    assert_eq!(hinge.perpendicular_b2, [0.0, 1.0, 0.0, 0.7]);
    assert_eq!(hinge.pivot_b, [-1.0, -2.0, -3.0, 0.8]);
    assert_eq!(hinge.min_angle, -0.75);
    assert_eq!(hinge.max_angle, 1.25);
    assert_eq!(hinge.max_friction, 10.0);
    assert_eq!(hinge.motor_type, 0);
    assert!(hinge.motor_data.is_empty());
}

#[test]
fn fmt_23_limited_hinge_accepts_supported_optional_motor_payloads() {
    for (motor_type, payload_len) in [(1, 25), (2, 18), (3, 17)] {
        let mut data = limited_hinge_fixture();
        *data.last_mut().unwrap() = motor_type;
        data.resize(data.len() + payload_len, 0);
        let Bhk::LimitedHingeConstraint(hinge) =
            bhk::decode("bhkLimitedHingeConstraint", &data, &header()).unwrap()
        else {
            panic!("limited hinge block did not decode as a constraint");
        };
        assert_eq!(hinge.motor_type, motor_type);
        assert_eq!(hinge.motor_data.len(), payload_len);
    }
}

#[test]
fn fmt_23_limited_hinge_rejects_invalid_count_limits_and_truncation() {
    let data = limited_hinge_fixture();
    for cut in [0, 4, 12, data.len() - 1] {
        assert!(bhk::decode("bhkLimitedHingeConstraint", &data[..cut], &header()).is_err());
    }

    let mut wrong_count = data.clone();
    wrong_count[0] = 3;
    assert!(bhk::decode("bhkLimitedHingeConstraint", &wrong_count, &header()).is_err());

    let mut reversed_limits = data;
    let min_angle = 0.75f32.to_le_bytes();
    let max_angle = (-0.75f32).to_le_bytes();
    let angles = 4 + 4 + 4 + 4 + 8 * 16;
    reversed_limits[angles..angles + 4].copy_from_slice(&min_angle);
    reversed_limits[angles + 4..angles + 8].copy_from_slice(&max_angle);
    assert!(bhk::decode("bhkLimitedHingeConstraint", &reversed_limits, &header()).is_err());
}
