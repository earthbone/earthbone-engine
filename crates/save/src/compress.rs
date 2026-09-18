// SPDX-License-Identifier: GPL-3.0-or-later

//! The two compression schemes a Special Edition save uses, both through
//! crates the workspace already depends on for archives: `flate2` for zlib and
//! `lz4_flex` for LZ4.
//!
//! The file body uses whichever scheme the header names, and in practice that
//! is LZ4. Change form payloads use zlib regardless, which is why zlib is here
//! even though the body rarely needs it. LZ4 is the block format, not the
//! frame format, so the decoder is told the output size rather than reading it
//! from a frame header.

use crate::error::{Error, Result};
use std::io::Read;

pub fn inflate_zlib(data: &[u8], expected: u32) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(expected as usize);
    flate2::read::ZlibDecoder::new(data)
        .read_to_end(&mut out)
        .map_err(|error| Error::Decompress {
            scheme: "zlib",
            reason: error.to_string(),
        })?;
    Ok(out)
}

pub fn deflate_zlib(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibEncoder::new(data, flate2::Compression::default())
        .read_to_end(&mut out)
        .map_err(|error| Error::Decompress {
            scheme: "zlib",
            reason: error.to_string(),
        })?;
    Ok(out)
}

pub fn inflate_lz4(data: &[u8], expected: u32) -> Result<Vec<u8>> {
    lz4_flex::block::decompress(data, expected as usize).map_err(|error| Error::Decompress {
        scheme: "lz4",
        reason: error.to_string(),
    })
}

pub fn deflate_lz4(data: &[u8]) -> Vec<u8> {
    lz4_flex::block::compress(data)
}
