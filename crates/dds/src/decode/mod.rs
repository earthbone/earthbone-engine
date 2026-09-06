// SPDX-License-Identifier: GPL-3.0-or-later

//! Decoders from each format to RGBA8. Block decoders produce a 4 by 4 tile
//! that `write_block` clips into the image, so widths and heights that are
//! not multiples of four work the way the hardware treats them.

mod bc1;
mod bc4;
mod bc7;
mod tables;
mod uncompressed;

use crate::error::{Error, Result};
use crate::format::Format;

/// One decoded 4 by 4 tile, RGBA8 in raster order.
pub type Block = [u8; 64];

/// Decode one slice of a level at `width` by `height` into RGBA8.
pub fn decode_slice(format: Format, width: u32, height: u32, data: &[u8]) -> Result<Vec<u8>> {
    let needed = format.slice_bytes(width, height);
    if data.len() < needed {
        return Err(Error::Truncated { needed, have: data.len() });
    }
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or(Error::TooLarge { width, height, depth: 1 })?;
    let mut out = vec![0u8; pixels];
    match format {
        Format::Uncompressed(m) => uncompressed::decode(m, width, height, data, &mut out),
        _ => {
            let bs = format.block_size().expect("block format");
            let bw = (width as usize).div_ceil(4);
            let bh = (height as usize).div_ceil(4);
            let mut block = [0u8; 64];
            for by in 0..bh {
                for bx in 0..bw {
                    let src = &data[(by * bw + bx) * bs..][..bs];
                    match format {
                        Format::Bc1 => bc1::decode_bc1(src, &mut block),
                        Format::Bc2 => bc1::decode_bc2(src, &mut block),
                        Format::Bc3 => bc1::decode_bc3(src, &mut block),
                        Format::Bc4 { signed } => bc4::decode_bc4(src, signed, &mut block),
                        Format::Bc5 { signed } => bc4::decode_bc5(src, signed, &mut block),
                        Format::Bc7 => bc7::decode(src, &mut block),
                        Format::Uncompressed(_) => unreachable!(),
                    }
                    write_block(&block, bx, by, width, height, &mut out);
                }
            }
        }
    }
    Ok(out)
}

fn write_block(block: &Block, bx: usize, by: usize, width: u32, height: u32, out: &mut [u8]) {
    let (w, h) = (width as usize, height as usize);
    for row in 0..4 {
        let y = by * 4 + row;
        if y >= h {
            break;
        }
        for col in 0..4 {
            let x = bx * 4 + col;
            if x >= w {
                break;
            }
            let src = (row * 4 + col) * 4;
            let dst = (y * w + x) * 4;
            out[dst..dst + 4].copy_from_slice(&block[src..src + 4]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_blocks_are_clipped() {
        // A 5 by 3 BC4 image needs two blocks across and one down.
        let block = [0x80u8, 0x80, 0, 0, 0, 0, 0, 0];
        let data: Vec<u8> = block.iter().chain(block.iter()).copied().collect();
        let out = decode_slice(Format::Bc4 { signed: false }, 5, 3, &data).unwrap();
        assert_eq!(out.len(), 5 * 3 * 4);
        assert!(out.chunks(4).all(|p| p == [0x80, 0, 0, 255]));
    }

    #[test]
    fn short_data_is_a_typed_error() {
        let err = decode_slice(Format::Bc7, 8, 8, &[0u8; 63]).unwrap_err();
        assert!(matches!(err, Error::Truncated { needed: 64, have: 63 }));
    }
}
