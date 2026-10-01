// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use soe_dds::{ColorSpace, Dds, Image};
use soe_godot_host::facegen::{
    face_tint_path, generate_face_tint, FaceTintError, FaceTintLayer, FACE_TINT_SIZE,
};

fn solid_mask(alpha: u8) -> Image {
    let mut rgba = Vec::with_capacity((FACE_TINT_SIZE * FACE_TINT_SIZE * 4) as usize);
    for _ in 0..FACE_TINT_SIZE * FACE_TINT_SIZE {
        rgba.extend_from_slice(&[255, 255, 255, alpha]);
    }
    Image {
        width: FACE_TINT_SIZE,
        height: FACE_TINT_SIZE,
        depth: 1,
        rgba,
    }
}

#[test]
fn edt_23_tint_layers_generate_expected_composite_dds() {
    let red = solid_mask(255);
    let blue = solid_mask(128);
    let bytes = generate_face_tint(&[
        FaceTintLayer {
            mask: &red,
            color: [200, 0, 0, 255],
            alpha: 1.0,
        },
        FaceTintLayer {
            mask: &blue,
            color: [0, 0, 200, 255],
            alpha: 0.5,
        },
    ])
    .unwrap();
    let dds = Dds::parse(&bytes).unwrap();
    assert_eq!(dds.format(), soe_dds::Format::Bc1);
    assert_eq!((dds.width(), dds.height(), dds.mip_count()), (512, 512, 1));
    let image = dds.decode(0, 0).unwrap();
    let pixel = &image.rgba[..4];
    assert!(pixel[0] >= 145 && pixel[0] <= 170, "{pixel:?}");
    assert!(pixel[1] <= 8, "{pixel:?}");
    assert!(pixel[2] >= 35 && pixel[2] <= 60, "{pixel:?}");
    assert_eq!(pixel[3], 255);
    assert_eq!(dds.color_space(), ColorSpace::Srgb);
}

#[test]
fn edt_23_tint_output_uses_plugin_and_local_form_path() {
    assert_eq!(
        face_tint_path("Example.esp", 0x0012_3456).unwrap(),
        Path::new("textures/actors/character/facegendata/facetint/Example.esp/00123456.dds")
    );
}

#[test]
fn edt_23_tint_generation_rejects_wrong_sized_masks() {
    let mask = Image {
        width: 1,
        height: 1,
        depth: 1,
        rgba: vec![255; 4],
    };
    assert!(matches!(
        generate_face_tint(&[FaceTintLayer {
            mask: &mask,
            color: [255; 4],
            alpha: 1.0,
        }]),
        Err(FaceTintError::InvalidMask)
    ));
}

#[test]
fn edt_23_tint_path_rejects_plugin_path_traversal() {
    assert!(matches!(
        face_tint_path("../Example.esp", 1),
        Err(FaceTintError::InvalidPluginName)
    ));
}
