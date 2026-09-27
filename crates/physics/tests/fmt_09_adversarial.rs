// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::panic)]

use soe_nif::{header::VERSION_20_2_0_7, Header};
use soe_physics::bhk::{self, Bhk};

fn header(version: u32) -> Header {
    Header {
        version,
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

fn payload(versioned_scale: bool) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&9u32.to_le_bytes());
    out.extend_from_slice(&0.1f32.to_le_bytes());
    out.extend_from_slice(&[0; 20]);
    out.extend_from_slice(&1u32.to_le_bytes());
    if versioned_scale {
        for value in [2.0f32, 3.0, 4.0, 0.0] {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&7i32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&[1, 0, 0, 0]);
    out
}

#[test]
fn fmt_09_pre_10_1_layout_omits_scale_and_keeps_filter_cardinality() {
    let block = bhk::decode("bhkNiTriStripsShape", &payload(false), &header(0x0A00_0000)).unwrap();
    assert!(
        matches!(block, Bhk::NiTriStrips { material: 9, scale, strips, filters }
        if scale == soe_physics::Vec3::new(1.0, 1.0, 1.0)
            && strips == [soe_nif::BlockRef(7)] && filters.len() == 1)
    );

    let mut malformed = payload(false);
    let count = malformed.len() - 8;
    malformed[count..count + 4].copy_from_slice(&0u32.to_le_bytes());
    assert!(bhk::decode("bhkNiTriStripsShape", &malformed, &header(0x0A00_0000)).is_err());
    let _ = VERSION_20_2_0_7;
}
