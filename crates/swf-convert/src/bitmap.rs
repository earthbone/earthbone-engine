// SPDX-License-Identifier: GPL-3.0-or-later

//! SWF JPEG and lossless bitmap tags, from Ruffle swf types and SWF19.
//! Lossless2 stores premultiplied ARGB and palettes have padded scanlines.

use std::io::Read;

use serde::{Deserialize, Serialize};
use swf::{BitmapFormat, DefineBitsLossless};

use crate::Error;

/// A decoded bitmap, ready for a Godot Image resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bitmap {
    /// Character id.
    pub id: u16,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Straight RGBA8 bytes.
    pub rgba: Vec<u8>,
}

pub(crate) fn inflate(data: &[u8], length: usize) -> Result<Vec<u8>, Error> {
    let mut decoded = Vec::new();
    flate2::read::ZlibDecoder::new(data).take(length as u64 + 1).read_to_end(&mut decoded)?;
    if decoded.len() != length {
        return Err(Error::Invalid(format!("bitmap has {} decoded bytes, expected {length}", decoded.len())));
    }
    Ok(decoded)
}

fn straight(pixel: [u8; 4]) -> [u8; 4] {
    let [r, g, b, a] = pixel;
    let unpremultiply = |channel: u8| if a == 0 { 0 } else { ((channel as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8 };
    [unpremultiply(r), unpremultiply(g), unpremultiply(b), a]
}

pub(crate) fn lossless(tag: &DefineBitsLossless<'_>) -> Result<Bitmap, Error> {
    let width = tag.width as usize;
    let height = tag.height as usize;
    if width == 0 || height == 0 || width * height > 16_777_216 {
        return Err(Error::Invalid("bitmap dimensions are empty or exceed 16 million pixels".into()));
    }
    let alpha = tag.version == 2;
    let (table_len, stride) = match tag.format {
        BitmapFormat::ColorMap8 { num_colors } => ((num_colors as usize + 1) * if alpha { 4 } else { 3 }, (width + 3) & !3),
        BitmapFormat::Rgb15 => (0, (width * 2 + 3) & !3),
        BitmapFormat::Rgb32 => (0, width * 4),
    };
    let bytes = inflate(&tag.data, table_len + stride * height)?;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let row = table_len + y * stride;
            let pixel = match tag.format {
                BitmapFormat::ColorMap8 { num_colors } => {
                    let index = bytes[row + x] as usize;
                    if index > num_colors as usize { return Err(Error::Invalid("bitmap palette index exceeds its table".into())); }
                    let offset = index * if alpha { 4 } else { 3 };
                    [bytes[offset], bytes[offset + 1], bytes[offset + 2], if alpha { bytes[offset + 3] } else { 255 }]
                }
                BitmapFormat::Rgb15 => {
                    let offset = row + x * 2;
                    let value = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
                    let channel = |shift| { let v = ((value >> shift) & 31) as u8; (v << 3) | (v >> 2) };
                    [channel(10), channel(5), channel(0), 255]
                }
                BitmapFormat::Rgb32 => {
                    let offset = row + x * 4;
                    [bytes[offset + 1], bytes[offset + 2], bytes[offset + 3], if alpha { bytes[offset] } else { 255 }]
                }
            };
            rgba.extend_from_slice(&if alpha { straight(pixel) } else { pixel });
        }
    }
    Ok(Bitmap { id: tag.id, width: tag.width as u32, height: tag.height as u32, rgba })
}

pub(crate) fn jpeg(id: u16, data: &[u8], tables: Option<&[u8]>, alpha: Option<&[u8]>) -> Result<Bitmap, Error> {
    let mut input = Vec::new();
    if let Some(tables) = tables {
        input.extend_from_slice(tables.strip_suffix(&[0xff, 0xd9]).unwrap_or(tables));
        input.extend_from_slice(data.strip_prefix(&[0xff, 0xd8]).unwrap_or(data));
    } else {
        input.extend_from_slice(data);
    }
    // CONTEXT: Older encoders inserted EOI/SOI pairs before or inside JPEG data.
    while let Some(offset) = input.windows(4).position(|w| w == [0xff, 0xd9, 0xff, 0xd8]) {
        input.drain(offset..offset + 4);
    }
    let reader = image::ImageReader::new(std::io::Cursor::new(input)).with_guessed_format()?;
    let mut reader = reader;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192); limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let mut decoded = reader.decode()?.into_rgba8();
    if let Some(alpha) = alpha {
        let bytes = inflate(alpha, decoded.width() as usize * decoded.height() as usize)?;
        for (pixel, alpha) in decoded.pixels_mut().zip(bytes) {
            pixel.0[3] = alpha;
            pixel.0 = straight(pixel.0);
        }
    }
    Ok(Bitmap { id, width: decoded.width(), height: decoded.height(), rgba: decoded.into_raw() })
}
