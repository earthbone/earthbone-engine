// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PNG export coverage for EDT-16's slot format, mip chain, and DDS reload check.
#![cfg(feature = "encode")]

use std::io::Cursor;

use image_dds::image::{DynamicImage, ImageOutputFormat, Rgba, RgbaImage};
use soe_dds::encode::{export_png, MipChain, SlotFormat};
use soe_dds::{ColorSpace, Dds, Format};

fn encode_png(image: RgbaImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut bytes), ImageOutputFormat::Png)
        .unwrap();
    bytes
}

fn gradient() -> RgbaImage {
    RgbaImage::from_fn(32, 16, |x, y| {
        Rgba([
            (x * 7) as u8,
            (y * 15) as u8,
            ((x + y) * 4) as u8,
            (96 + x * 5) as u8,
        ])
    })
}

#[test]
fn edt_16_png_export_uses_declared_slot_format_and_full_mip_chain() {
    let png = encode_png(gradient());
    let bytes = export_png(&png, SlotFormat::TangentNormal, MipChain::Full).unwrap();
    let dds = Dds::parse(&bytes).unwrap();

    assert_eq!(dds.format(), Format::Bc3);
    assert_eq!(dds.color_space(), ColorSpace::Linear);
    assert_eq!((dds.width(), dds.height()), (32, 16));
    assert_eq!(dds.mip_count(), 6);
    for (mip, level) in dds.levels().iter().enumerate() {
        assert_eq!(level.mip, mip as u32);
        assert_eq!(level.width, (32 >> mip).max(1));
        assert_eq!(level.height, (16 >> mip).max(1));
    }
}

#[test]
fn edt_16_png_export_reloads_within_declared_image_tolerance() {
    const MAX_CHANNEL_ERROR: i32 = 24;
    let source = gradient();
    let png = encode_png(source.clone());
    let bytes = export_png(&png, SlotFormat::DiffuseAlpha, MipChain::BaseOnly).unwrap();
    let decoded = Dds::parse(&bytes).unwrap().decode(0, 0).unwrap();

    assert_eq!((decoded.width, decoded.height), source.dimensions());
    let worst_error = source
        .as_raw()
        .iter()
        .zip(decoded.rgba.iter())
        .map(|(expected, actual)| (*expected as i32 - *actual as i32).abs())
        .max()
        .unwrap_or_default();
    assert!(
        worst_error <= MAX_CHANNEL_ERROR,
        "worst channel error {worst_error} exceeds {MAX_CHANNEL_ERROR}"
    );
}

#[test]
fn edt_16_png_export_rejects_invalid_png_and_mip_count() {
    assert!(export_png(b"not a png", SlotFormat::DiffuseOpaque, MipChain::Full).is_err());

    let png = encode_png(RgbaImage::from_pixel(8, 4, Rgba([70, 80, 90, 255])));
    assert!(export_png(&png, SlotFormat::DiffuseOpaque, MipChain::Levels(5)).is_err());
}
