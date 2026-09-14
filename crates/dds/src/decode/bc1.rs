// SPDX-License-Identifier: GPL-3.0-or-later

//! BC1, BC2 and BC3. The colour block arithmetic follows bcdec so that a
//! decode here and a decode through the reference library agree byte for
//! byte, which the gated real-file test relies on.

use super::Block;

fn u16le(b: &[u8]) -> u32 {
    u16::from_le_bytes([b[0], b[1]]) as u32
}

/// Decode the 8-byte colour block. `opaque_only` is set by BC2 and BC3,
/// whose colour block never uses the three-colour mode.
fn color_block(src: &[u8], opaque_only: bool, out: &mut Block) {
    let c0 = u16le(&src[0..2]);
    let c1 = u16le(&src[2..4]);
    let (r0, g0, b0) = ((c0 >> 11) & 0x1F, (c0 >> 5) & 0x3F, c0 & 0x1F);
    let (r1, g1, b1) = ((c1 >> 11) & 0x1F, (c1 >> 5) & 0x3F, c1 & 0x1F);
    let expand = |r: u32, g: u32, b: u32| {
        [
            ((r * 527 + 23) >> 6) as u8,
            ((g * 259 + 33) >> 6) as u8,
            ((b * 527 + 23) >> 6) as u8,
            255,
        ]
    };
    let mut refs = [expand(r0, g0, b0), expand(r1, g1, b1), [0; 4], [0; 4]];
    if c0 > c1 || opaque_only {
        refs[2] = [
            (((2 * r0 + r1) * 351 + 61) >> 7) as u8,
            (((2 * g0 + g1) * 2763 + 1039) >> 11) as u8,
            (((2 * b0 + b1) * 351 + 61) >> 7) as u8,
            255,
        ];
        refs[3] = [
            (((r0 + 2 * r1) * 351 + 61) >> 7) as u8,
            (((g0 + 2 * g1) * 2763 + 1039) >> 11) as u8,
            (((b0 + 2 * b1) * 351 + 61) >> 7) as u8,
            255,
        ];
    } else {
        refs[2] = [
            (((r0 + r1) * 1053 + 125) >> 8) as u8,
            (((g0 + g1) * 4145 + 1019) >> 11) as u8,
            (((b0 + b1) * 1053 + 125) >> 8) as u8,
            255,
        ];
        refs[3] = [0, 0, 0, 0];
    }
    let mut indices = u32::from_le_bytes([src[4], src[5], src[6], src[7]]);
    for px in 0..16 {
        out[px * 4..px * 4 + 4].copy_from_slice(&refs[(indices & 3) as usize]);
        indices >>= 2;
    }
}

pub fn decode_bc1(src: &[u8], out: &mut Block) {
    color_block(src, false, out);
}

pub fn decode_bc2(src: &[u8], out: &mut Block) {
    color_block(&src[8..16], true, out);
    for row in 0..4 {
        let a = u16le(&src[row * 2..row * 2 + 2]);
        for col in 0..4 {
            out[(row * 4 + col) * 4 + 3] = (((a >> (4 * col)) & 0xF) * 17) as u8;
        }
    }
}

/// The 8-byte smooth alpha block shared by BC3, BC4 and BC5, unsigned.
pub fn smooth_alpha(src: &[u8]) -> [u8; 16] {
    let block = u64::from_le_bytes([
        src[0], src[1], src[2], src[3], src[4], src[5], src[6], src[7],
    ]);
    let a0 = (block & 0xFF) as u32;
    let a1 = ((block >> 8) & 0xFF) as u32;
    let mut a = [a0 as u8, a1 as u8, 0, 0, 0, 0, 0, 0];
    if a0 > a1 {
        for (i, w) in (1..=6).enumerate() {
            a[2 + i] = (((7 - w) * a0 + w * a1) / 7) as u8;
        }
    } else {
        for (i, w) in (1..=4).enumerate() {
            a[2 + i] = (((5 - w) * a0 + w * a1) / 5) as u8;
        }
        a[6] = 0;
        a[7] = 255;
    }
    let mut indices = block >> 16;
    let mut out = [0u8; 16];
    for px in out.iter_mut() {
        *px = a[(indices & 7) as usize];
        indices >>= 3;
    }
    out
}

pub fn decode_bc3(src: &[u8], out: &mut Block) {
    color_block(&src[8..16], true, out);
    let alpha = smooth_alpha(&src[0..8]);
    for px in 0..16 {
        out[px * 4 + 3] = alpha[px];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bc1_block_decodes_to_the_expected_pixels() {
        // c0 pure red, c1 pure blue, c0 > c1 so four colours. Indices: row 0
        // all c0, row 1 all c1, row 2 all the 2/3 mix, row 3 all the 1/3 mix.
        let src = [
            0x00,
            0xF8,
            0x1F,
            0x00,
            0b0000_0000,
            0b0101_0101,
            0b1010_1010,
            0b1111_1111,
        ];
        let mut out = [0u8; 64];
        decode_bc1(&src, &mut out);
        assert_eq!(&out[0..4], &[255, 0, 0, 255]);
        assert_eq!(&out[16..20], &[0, 0, 255, 255]);
        assert_eq!(&out[32..36], &[170, 0, 85, 255]);
        assert_eq!(&out[48..52], &[85, 0, 170, 255]);
    }

    #[test]
    fn bc1_three_colour_mode_has_a_transparent_slot() {
        // c0 < c1 selects the three-colour mode. Index 3 is transparent black.
        let src = [0x1F, 0x00, 0x00, 0xF8, 0xFF, 0xFF, 0xFF, 0xFF];
        let mut out = [0u8; 64];
        decode_bc1(&src, &mut out);
        assert!(out.chunks(4).all(|p| p == [0, 0, 0, 0]));
        let src = [0x1F, 0x00, 0x00, 0xF8, 0xAA, 0xAA, 0xAA, 0xAA];
        decode_bc1(&src, &mut out);
        assert!(out.chunks(4).all(|p| p == [128, 0, 128, 255]));
    }

    #[test]
    fn bc2_alpha_is_four_bits_scaled() {
        let mut src = [0u8; 16];
        src[0] = 0xF0; // row 0: pixel 0 alpha 0, pixel 1 alpha 15
        src[8..].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0]);
        let mut out = [0u8; 64];
        decode_bc2(&src, &mut out);
        assert_eq!(out[3], 0);
        assert_eq!(out[7], 255);
        assert_eq!(&out[0..3], &[255, 255, 255]);
    }

    #[test]
    fn bc3_alpha_interpolates_eight_or_six_values() {
        // a0 > a1: eight interpolated values, index 7 is a1.
        let mut src = [0u8; 16];
        src[0] = 224;
        src[1] = 0;
        src[2] = 0b1111_1000; // pixel 0 index 0, pixel 1 index 7
        let mut out = [0u8; 64];
        decode_bc3(&src, &mut out);
        assert_eq!(out[3], 224);
        assert_eq!(out[7], 32, "index 7 is the 1/7 to 6/7 mix, not a1");
        // a0 <= a1: index 6 is 0 and index 7 is 255 regardless of endpoints.
        src[0] = 10;
        src[1] = 20;
        src[2] = 0b0011_1110; // pixel 0 index 6, pixel 1 index 7
        decode_bc3(&src, &mut out);
        assert_eq!(out[3], 0);
        assert_eq!(out[7], 255);
    }
}
