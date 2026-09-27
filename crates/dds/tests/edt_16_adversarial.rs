// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used)]
#![cfg(feature = "encode")]

use std::io::Cursor;

use image_dds::image::{DynamicImage, ImageOutputFormat, Rgba, RgbaImage};
use soe_dds::encode::{export_png, MipChain, SlotFormat};
use soe_dds::{ColorSpace, Dds, Format};

#[test]
fn edt_16_linear_mask_uses_single_channel_linear_format_and_full_chain() {
    let image = RgbaImage::from_pixel(8, 4, Rgba([128, 33, 7, 255]));
    let mut png = Vec::new();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut png), ImageOutputFormat::Png)
        .unwrap();
    let encoded = export_png(&png, SlotFormat::LinearMask, MipChain::Full).unwrap();
    let dds = Dds::parse(&encoded).unwrap();
    assert_eq!(dds.format(), Format::Bc4 { signed: false });
    assert_eq!(dds.color_space(), ColorSpace::Linear);
    assert_eq!(dds.mip_count(), 4);
}

#[test]
fn edt_16_opaque_diffuse_discards_source_transparency() {
    let image = RgbaImage::from_pixel(8, 8, Rgba([180, 90, 40, 0]));
    let mut png = Vec::new();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut png), ImageOutputFormat::Png)
        .unwrap();
    let encoded = export_png(&png, SlotFormat::DiffuseOpaque, MipChain::BaseOnly).unwrap();
    let dds = Dds::parse(&encoded).unwrap();
    assert_eq!(dds.format(), Format::Bc1);
    let decoded = dds.decode(0, 0).unwrap();
    let (pixels, remainder) = decoded.rgba.as_chunks::<4>();
    assert!(remainder.is_empty());
    assert!(pixels.iter().all(|pixel| pixel[3] == 255));
}
