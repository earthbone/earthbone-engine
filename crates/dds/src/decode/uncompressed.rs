// SPDX-License-Identifier: GPL-3.0-or-later

//! Mask-described pixels, 8 to 32 bits each, little-endian.

use crate::format::Masks;

fn extract(pixel: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let bits = (mask >> shift).count_ones();
    let v = (pixel & mask) >> shift;
    match bits {
        8 => v as u8,
        b if b > 8 => (v >> (b - 8)) as u8,
        b => {
            let max = (1u32 << b) - 1;
            ((v * 255 + max / 2) / max) as u8
        }
    }
}

pub fn decode(m: Masks, width: u32, height: u32, data: &[u8], out: &mut [u8]) {
    let bpp = m.bits as usize / 8;
    let n = width as usize * height as usize;
    for px in 0..n {
        let src = &data[px * bpp..px * bpp + bpp];
        let mut pixel = 0u32;
        for (i, b) in src.iter().enumerate() {
            pixel |= (*b as u32) << (8 * i);
        }
        let r = extract(pixel, m.r);
        let (g, b) = if m.grey {
            (r, r)
        } else {
            (extract(pixel, m.g), extract(pixel, m.b))
        };
        let a = if m.a == 0 { 255 } else { extract(pixel, m.a) };
        out[px * 4..px * 4 + 4].copy_from_slice(&[r, g, b, a]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_and_rgba_orders_agree_after_decoding() {
        let mut out = [0u8; 8];
        decode(Masks::BGRA8, 2, 1, &[3, 2, 1, 4, 30, 20, 10, 40], &mut out);
        assert_eq!(out, [1, 2, 3, 4, 10, 20, 30, 40]);
        decode(Masks::RGBA8, 2, 1, &[1, 2, 3, 4, 10, 20, 30, 40], &mut out);
        assert_eq!(out, [1, 2, 3, 4, 10, 20, 30, 40]);
    }

    #[test]
    fn packed_16_bit_channels_scale_to_full_range() {
        let mut out = [0u8; 4];
        decode(Masks::B5G6R5, 1, 1, &[0xFF, 0xFF], &mut out);
        assert_eq!(out, [255, 255, 255, 255]);
        decode(Masks::B5G5R5A1, 1, 1, &[0x00, 0x80], &mut out);
        assert_eq!(out, [0, 0, 0, 255]);
        decode(Masks::B4G4R4A4, 1, 1, &[0x0F, 0x70], &mut out);
        assert_eq!(out, [0, 0, 255, 119]);
    }

    #[test]
    fn luminance_replicates_and_missing_alpha_is_opaque() {
        let mut out = [0u8; 8];
        decode(Masks::L8, 2, 1, &[7, 9], &mut out);
        assert_eq!(out, [7, 7, 7, 255, 9, 9, 9, 255]);
        decode(Masks::BGR8, 1, 1, &[1, 2, 3], &mut out[..4]);
        assert_eq!(&out[..4], &[3, 2, 1, 255]);
    }
}
