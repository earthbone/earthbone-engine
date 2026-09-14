// SPDX-License-Identifier: GPL-3.0-or-later

//! Re-encoding through image_dds, for the mod manager's `TransformedTexture`
//! directive: decode, resize to the recorded size, encode to the recorded
//! DXGI format with the recorded mip count. Two block compressors never agree
//! byte for byte, so the output is checked by header, not by hash, the way
//! Wabbajack does it.

use std::io::Cursor;

use image_dds::ddsfile::Dds as DdsFile;
use image_dds::image::{imageops, RgbaImage};
use image_dds::{dds_from_image, ImageFormat, Mipmaps, Quality};

use crate::error::{Error, Result};
use crate::format::dxgi;
use crate::Dds;

/// What to produce: size, mip count (zero means a full chain) and DXGI format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub width: u32,
    pub height: u32,
    pub mip_levels: u32,
    pub dxgi_format: u32,
}

/// The DXGI formats the encoder can write, the same set Wabbajack lists.
pub fn image_format(dxgi_format: u32) -> Option<ImageFormat> {
    Some(match dxgi_format {
        dxgi::R8G8B8A8_UNORM => ImageFormat::R8G8B8A8Unorm,
        dxgi::R8G8B8A8_UNORM_SRGB => ImageFormat::R8G8B8A8Srgb,
        dxgi::R8_UNORM => ImageFormat::R8Unorm,
        dxgi::BC1_UNORM => ImageFormat::BC1Unorm,
        dxgi::BC1_UNORM_SRGB => ImageFormat::BC1Srgb,
        dxgi::BC2_UNORM => ImageFormat::BC2Unorm,
        dxgi::BC2_UNORM_SRGB => ImageFormat::BC2Srgb,
        dxgi::BC3_UNORM => ImageFormat::BC3Unorm,
        dxgi::BC3_UNORM_SRGB => ImageFormat::BC3Srgb,
        dxgi::BC4_UNORM => ImageFormat::BC4Unorm,
        dxgi::BC4_SNORM => ImageFormat::BC4Snorm,
        dxgi::BC5_UNORM => ImageFormat::BC5Unorm,
        dxgi::BC5_SNORM => ImageFormat::BC5Snorm,
        dxgi::B8G8R8A8_UNORM => ImageFormat::B8G8R8A8Unorm,
        dxgi::B8G8R8A8_UNORM_SRGB => ImageFormat::B8G8R8A8Srgb,
        dxgi::BC6H_UF16 => ImageFormat::BC6Ufloat,
        dxgi::BC6H_SF16 => ImageFormat::BC6Sfloat,
        dxgi::BC7_UNORM => ImageFormat::BC7Unorm,
        dxgi::BC7_UNORM_SRGB => ImageFormat::BC7Srgb,
        _ => return None,
    })
}

/// Encode RGBA8 pixels to a DDS file in memory.
pub fn encode_rgba8(
    width: u32,
    height: u32,
    rgba: &[u8],
    mip_levels: u32,
    dxgi_format: u32,
) -> Result<Vec<u8>> {
    let format = image_format(dxgi_format)
        .ok_or_else(|| Error::Encode(format!("DXGI format {dxgi_format} cannot be encoded")))?;
    let image = RgbaImage::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| Error::Encode("pixel buffer does not match the size".into()))?;
    let mipmaps = match mip_levels {
        0 => Mipmaps::GeneratedAutomatic,
        1 => Mipmaps::Disabled,
        n => Mipmaps::GeneratedExact(n),
    };
    let dds = dds_from_image(&image, format, Quality::Normal, mipmaps)
        .map_err(|e| Error::Encode(e.to_string()))?;
    write(&dds)
}

fn write(dds: &DdsFile) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    dds.write(&mut Cursor::new(&mut bytes))
        .map_err(|e| Error::Encode(e.to_string()))?;
    Ok(bytes)
}

/// Decode the base level with this crate's decoder, resize with Lanczos3
/// when the target size differs, and encode. Cube maps and arrays are
/// refused since the target describes one 2D surface.
pub fn recompress(src: &[u8], target: Target) -> Result<Vec<u8>> {
    let dds = Dds::parse(src)?;
    if dds.layer_count() > 1 {
        return Err(Error::Encode(
            "cube maps and texture arrays are not supported".into(),
        ));
    }
    let base = dds.decode(0, 0)?;
    if base.depth > 1 {
        return Err(Error::Encode("volume textures are not supported".into()));
    }
    let image =
        RgbaImage::from_raw(base.width, base.height, base.rgba).expect("decoded size matches");
    let image = if image.width() != target.width || image.height() != target.height {
        imageops::resize(
            &image,
            target.width,
            target.height,
            imageops::FilterType::Lanczos3,
        )
    } else {
        image
    };
    encode_rgba8(
        target.width,
        target.height,
        image.as_raw(),
        target.mip_levels,
        target.dxgi_format,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Format;

    #[test]
    fn a_solid_image_survives_bc1_and_bc7_round_trips() {
        let rgba: Vec<u8> = (0..16 * 16).flat_map(|_| [200u8, 100, 50, 255]).collect();
        for (fmt, expect) in [
            (dxgi::BC1_UNORM, Format::Bc1),
            (dxgi::BC7_UNORM_SRGB, Format::Bc7),
        ] {
            let bytes = encode_rgba8(16, 16, &rgba, 0, fmt).unwrap();
            let dds = Dds::parse(&bytes).unwrap();
            assert_eq!(dds.format(), expect);
            assert_eq!(dds.mip_count(), 5);
            let img = dds.decode(0, 0).unwrap();
            for p in img.rgba.chunks(4) {
                assert!(
                    (p[0] as i32 - 200).abs() <= 8
                        && (p[1] as i32 - 100).abs() <= 8
                        && (p[2] as i32 - 50).abs() <= 8,
                    "{p:?}"
                );
                assert_eq!(p[3], 255);
            }
        }
    }

    #[test]
    fn recompress_resizes_and_changes_format() {
        let rgba: Vec<u8> = (0..32 * 32).flat_map(|_| [10u8, 20, 30, 255]).collect();
        let src = encode_rgba8(32, 32, &rgba, 1, dxgi::B8G8R8A8_UNORM).unwrap();
        let out = recompress(
            &src,
            Target {
                width: 8,
                height: 8,
                mip_levels: 2,
                dxgi_format: dxgi::BC3_UNORM,
            },
        )
        .unwrap();
        let dds = Dds::parse(&out).unwrap();
        assert_eq!((dds.width(), dds.height(), dds.mip_count()), (8, 8, 2));
        assert_eq!(dds.format(), Format::Bc3);
        assert!(matches!(
            encode_rgba8(4, 4, &[0; 64], 1, 10),
            Err(Error::Encode(_))
        ));
    }
}
