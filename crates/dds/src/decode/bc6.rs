// SPDX-License-Identifier: GPL-3.0-or-later

//! BC6H blocks to RGBA8 through the safe bcdec_rs decoder.

use super::Block;

/// Decode one 16-byte BC6H block to RGBA8.
pub fn decode(src: &[u8], signed: bool, out: &mut Block) {
    let mut rgb = [0.0f32; 4 * 4 * 3];
    bcdec_rs::bc6h_float(src, &mut rgb, 4 * 3, signed);
    for (pixel, [red, green, blue]) in rgb.as_chunks::<3>().0.iter().enumerate() {
        let out_index = pixel * 4;
        out[out_index] = (red * 255.0) as u8;
        out[out_index + 1] = (green * 255.0) as u8;
        out[out_index + 2] = (blue * 255.0) as u8;
        out[out_index + 3] = 255;
    }
}
