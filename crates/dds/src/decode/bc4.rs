// SPDX-License-Identifier: GPL-3.0-or-later

//! BC4 (one channel) and BC5 (two channels). Unsigned output lands in red,
//! and red and green, with the other channels zero and alpha opaque. Signed
//! blocks are decoded as the format defines, on -127 to 127, then mapped to
//! 0 to 255 so a normal map reads the same whichever way it was stored.

use super::bc1::smooth_alpha;
use super::Block;

fn signed_channel(src: &[u8]) -> [u8; 16] {
    let block = u64::from_le_bytes([
        src[0], src[1], src[2], src[3], src[4], src[5], src[6], src[7],
    ]);
    let clamp = |v: i32| v.max(-127);
    let a0 = clamp((block & 0xFF) as u8 as i8 as i32);
    let a1 = clamp(((block >> 8) & 0xFF) as u8 as i8 as i32);
    let mut a = [a0, a1, 0, 0, 0, 0, 0, 0];
    if a0 > a1 {
        for (i, w) in (1..=6).enumerate() {
            a[2 + i] = ((7 - w) * a0 + w * a1) / 7;
        }
    } else {
        for (i, w) in (1..=4).enumerate() {
            a[2 + i] = ((5 - w) * a0 + w * a1) / 5;
        }
        a[6] = -127;
        a[7] = 127;
    }
    let mut indices = block >> 16;
    let mut out = [0u8; 16];
    for px in out.iter_mut() {
        let v = a[(indices & 7) as usize];
        *px = (((v + 127) * 255 + 127) / 254) as u8;
        indices >>= 3;
    }
    out
}

fn channel(src: &[u8], signed: bool) -> [u8; 16] {
    if signed {
        signed_channel(src)
    } else {
        smooth_alpha(src)
    }
}

pub fn decode_bc4(src: &[u8], signed: bool, out: &mut Block) {
    let r = channel(&src[0..8], signed);
    for px in 0..16 {
        out[px * 4..px * 4 + 4].copy_from_slice(&[r[px], 0, 0, 255]);
    }
}

pub fn decode_bc5(src: &[u8], signed: bool, out: &mut Block) {
    let r = channel(&src[0..8], signed);
    let g = channel(&src[8..16], signed);
    for px in 0..16 {
        out[px * 4..px * 4 + 4].copy_from_slice(&[r[px], g[px], 0, 255]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bc5_puts_the_two_channels_in_red_and_green() {
        let mut src = [0u8; 16];
        src[0] = 200;
        src[1] = 200;
        src[8] = 40;
        src[9] = 40;
        let mut out = [0u8; 64];
        decode_bc5(&src, false, &mut out);
        assert!(out.chunks(4).all(|p| p == [200, 40, 0, 255]));
    }

    #[test]
    fn signed_bc4_maps_the_full_range_onto_bytes() {
        let mut src = [0u8; 8];
        src[0] = 127u8; // +127
        src[1] = 0x81; // -127
        src[2] = 0b0000_1000; // pixel 0 index 0, pixel 1 index 1
        let mut out = [0u8; 64];
        decode_bc4(&src, true, &mut out);
        assert_eq!(out[0], 255);
        assert_eq!(out[4], 0);
        // -128 clamps to -127, so both endpoints equal and every pixel is 0.
        src[0] = 0x80;
        src[2] = 0;
        decode_bc4(&src, true, &mut out);
        assert_eq!(out[0], 0);
    }
}
