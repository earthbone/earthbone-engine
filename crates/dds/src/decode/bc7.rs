// SPDX-License-Identifier: GPL-3.0-or-later

//! BC7, the eight-mode block format. The bit layout follows the Direct3D 11
//! specification, with the partition and anchor tables generated from
//! bcdec so the two decoders can be compared on real textures.

use super::tables::{PARTITIONS_2, PARTITIONS_3};
use super::Block;

struct Bits(u128);

impl Bits {
    fn read(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let v = (self.0 & ((1u128 << n) - 1)) as u32;
        self.0 >>= n;
        v
    }
}

const COLOR_BITS: [u32; 8] = [4, 6, 5, 7, 5, 7, 7, 5];
const ALPHA_BITS: [u32; 8] = [0, 0, 0, 0, 6, 8, 7, 5];
/// Modes with a p-bit per endpoint (0, 3, 6, 7) or shared per subset (1).
const HAS_PBITS: u8 = 0b1100_1011;
const WEIGHT2: [u32; 4] = [0, 21, 43, 64];
const WEIGHT3: [u32; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const WEIGHT4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

fn interpolate(a: u32, b: u32, weights: &[u32], i: u32) -> u8 {
    let w = weights[i as usize];
    (((64 - w) * a + w * b + 32) >> 6) as u8
}

pub fn decode(src: &[u8], out: &mut Block) {
    let mut bits = Bits(u128::from_le_bytes(src[..16].try_into().expect("16-byte block")));
    let mut mode = 0;
    while mode < 8 && bits.read(1) == 0 {
        mode += 1;
    }
    if mode >= 8 {
        out.fill(0);
        return;
    }
    let subsets = match mode {
        0 | 2 => 3,
        1 | 3 | 7 => 2,
        _ => 1,
    };
    let partition = match mode {
        0 => bits.read(4),
        1 | 2 | 3 | 7 => bits.read(6),
        _ => 0,
    } as usize;
    let rotation = if mode == 4 || mode == 5 { bits.read(2) } else { 0 };
    let index_selection = if mode == 4 { bits.read(1) } else { 0 };
    let endpoints_n = subsets * 2;

    let mut endpoints = [[0u32; 4]; 6];
    for ch in 0..3 {
        for ep in endpoints.iter_mut().take(endpoints_n) {
            ep[ch] = bits.read(COLOR_BITS[mode]);
        }
    }
    if ALPHA_BITS[mode] > 0 {
        for ep in endpoints.iter_mut().take(endpoints_n) {
            ep[3] = bits.read(ALPHA_BITS[mode]);
        }
    }
    let has_p = HAS_PBITS & (1 << mode) != 0;
    if has_p {
        for ep in endpoints.iter_mut().take(endpoints_n) {
            for c in ep.iter_mut() {
                *c <<= 1;
            }
        }
        if mode == 1 {
            let p0 = bits.read(1);
            let p1 = bits.read(1);
            for (ep, p) in endpoints.iter_mut().take(4).zip([p0, p0, p1, p1]) {
                for c in ep.iter_mut().take(3) {
                    *c |= p;
                }
            }
        } else {
            for ep in endpoints.iter_mut().take(endpoints_n) {
                let p = bits.read(1);
                for c in ep.iter_mut() {
                    *c |= p;
                }
            }
        }
    }
    let p_extra = u32::from(has_p);
    for ep in endpoints.iter_mut().take(endpoints_n) {
        let cb = COLOR_BITS[mode] + p_extra;
        for c in ep.iter_mut().take(3) {
            *c <<= 8 - cb;
            *c |= *c >> cb;
        }
        if ALPHA_BITS[mode] > 0 {
            let ab = ALPHA_BITS[mode] + p_extra;
            ep[3] <<= 8 - ab;
            ep[3] |= ep[3] >> ab;
        } else {
            ep[3] = 0xFF;
        }
    }

    let index_bits = match mode {
        0 | 1 => 3,
        6 => 4,
        _ => 2,
    };
    let index_bits2 = match mode {
        4 => 3,
        5 => 2,
        _ => 0,
    };
    let weights: &[u32] = match index_bits {
        2 => &WEIGHT2,
        3 => &WEIGHT3,
        _ => &WEIGHT4,
    };
    let weights2: &[u32] = if index_bits2 == 2 { &WEIGHT2 } else { &WEIGHT3 };

    let subset_of = |px: usize| -> u8 {
        match subsets {
            1 => {
                if px == 0 {
                    128
                } else {
                    0
                }
            }
            2 => PARTITIONS_2[partition][px],
            _ => PARTITIONS_3[partition][px],
        }
    };

    let mut indices = [0u32; 16];
    for (px, idx) in indices.iter_mut().enumerate() {
        let anchor = subset_of(px) & 0x80 != 0;
        *idx = bits.read(index_bits - u32::from(anchor));
    }
    for px in 0..16 {
        let s = (subset_of(px) & 3) as usize;
        let e0 = endpoints[s * 2];
        let e1 = endpoints[s * 2 + 1];
        let i = indices[px];
        let mut rgba = [0u8; 4];
        if index_bits2 == 0 {
            for c in 0..4 {
                rgba[c] = interpolate(e0[c], e1[c], weights, i);
            }
        } else {
            let i2 = bits.read(if px == 0 { index_bits2 - 1 } else { index_bits2 });
            let (ci, cw, ai, aw): (u32, &[u32], u32, &[u32]) = if index_selection == 0 {
                (i, weights, i2, weights2)
            } else {
                (i2, weights2, i, weights)
            };
            for c in 0..3 {
                rgba[c] = interpolate(e0[c], e1[c], cw, ci);
            }
            rgba[3] = interpolate(e0[3], e1[3], aw, ai);
        }
        match rotation {
            1 => rgba.swap(3, 0),
            2 => rgba.swap(3, 1),
            3 => rgba.swap(3, 2),
            _ => {}
        }
        out[px * 4..px * 4 + 4].copy_from_slice(&rgba);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A little-endian bit writer for building blocks in tests.
    struct Writer(u128, u32);
    impl Writer {
        fn put(&mut self, v: u32, n: u32) {
            self.0 |= (v as u128) << self.1;
            self.1 += n;
        }
        fn bytes(&self) -> [u8; 16] {
            self.0.to_le_bytes()
        }
    }

    #[test]
    fn mode_6_solid_block_reproduces_its_endpoint() {
        // Mode 6: seven-bit RGBA endpoints plus a p-bit each, 4-bit indices.
        let mut w = Writer(0, 0);
        w.put(0b100_0000, 7); // mode 6 marker: six zeros then a one
        for _ in 0..3 {
            w.put(0x7F, 7); // endpoint 0 colour channels full
            w.put(0x00, 7); // endpoint 1 zero
        }
        w.put(0x7F, 7); // alpha e0
        w.put(0x00, 7); // alpha e1
        w.put(1, 1); // p-bits
        w.put(0, 1);
        w.put(0, 3); // anchor index, 3 bits, all zero => endpoint 0
        for _ in 1..16 {
            w.put(0, 4);
        }
        let mut out = [0u8; 64];
        decode(&w.bytes(), &mut out);
        assert!(out.chunks(4).all(|p| p == [255, 255, 255, 255]), "{:?}", &out[..8]);
        // Index 15 selects endpoint 1 exactly.
        let mut w2 = Writer(0, 0);
        w2.put(0b100_0000, 7);
        for _ in 0..3 {
            w2.put(0x7F, 7);
            w2.put(0x00, 7);
        }
        w2.put(0x7F, 7);
        w2.put(0x00, 7);
        w2.put(1, 1);
        w2.put(0, 1);
        w2.put(0, 3);
        for _ in 1..16 {
            w2.put(15, 4);
        }
        decode(&w2.bytes(), &mut out);
        assert_eq!(&out[0..4], &[255, 255, 255, 255]);
        assert!(out[4..].chunks(4).all(|p| p == [0, 0, 0, 0]));
    }

    #[test]
    fn mode_5_rotation_swaps_alpha_into_a_colour_channel() {
        // Mode 5: 2-bit rotation, 7-bit colour, 8-bit alpha, 2-bit indices twice.
        let mut w = Writer(0, 0);
        w.put(0b10_0000, 6);
        w.put(1, 2); // rotation 1: alpha and red swap
        for _ in 0..3 {
            w.put(0, 7);
            w.put(0, 7);
        }
        w.put(255, 8); // alpha e0
        w.put(255, 8); // alpha e1
        w.put(0, 1); // colour anchor index
        for _ in 1..16 {
            w.put(0, 2);
        }
        w.put(0, 1); // alpha anchor index
        for _ in 1..16 {
            w.put(0, 2);
        }
        let mut out = [0u8; 64];
        decode(&w.bytes(), &mut out);
        // Colour was black with alpha 255. After rotation red is 255, alpha 0.
        assert!(out.chunks(4).all(|p| p == [255, 0, 0, 0]));
    }

    #[test]
    fn a_reserved_mode_decodes_to_transparent_black() {
        let mut out = [1u8; 64];
        decode(&[0u8; 16], &mut out);
        assert!(out.iter().all(|b| *b == 0));
    }

    #[test]
    fn partition_tables_carry_one_anchor_per_extra_subset() {
        for row in PARTITIONS_2.iter() {
            assert_eq!(row[0], 128);
            assert_eq!(row.iter().filter(|v| **v & 0x80 != 0).count(), 2);
        }
        for row in PARTITIONS_3.iter() {
            assert_eq!(row[0], 128);
            assert_eq!(row.iter().filter(|v| **v & 0x80 != 0).count(), 3);
        }
    }
}
