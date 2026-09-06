// SPDX-License-Identifier: GPL-3.0-or-later

//! BC7 against an independent decoder on blocks the Intel encoder produces
//! from noise, which exercises every mode, partition and rotation the
//! encoder can choose. Vanilla Skyrim SE textures are BC1, BC3 and
//! uncompressed, so this is where BC7's tables are proven.
#![cfg(feature = "encode")]

use soe_dds::encode::encode_rgba8;
use soe_dds::{dxgi, Dds, Format};

fn noise(seed: &mut u64, n: usize) -> Vec<u8> {
    (0..n)
        .map(|_| {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 7;
            *seed ^= *seed << 17;
            (*seed >> 24) as u8
        })
        .collect()
}

#[test]
fn bc7_blocks_from_the_intel_encoder_decode_like_the_reference() {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut modes_seen = std::collections::BTreeSet::new();
    for round in 0..6 {
        let (w, h) = (64u32, 64u32);
        // Alternate pure noise with smooth gradients plus noise, so the
        // encoder picks both the multi-subset and the single-subset modes.
        let mut rgba = noise(&mut seed, (w * h * 4) as usize);
        if round % 2 == 1 {
            for (i, px) in rgba.chunks_mut(4).enumerate() {
                let x = (i as u32 % w) as u8;
                let y = (i as u32 / w) as u8;
                px[0] = x.wrapping_mul(4).wrapping_add(px[0] / 8);
                px[1] = y.wrapping_mul(4).wrapping_add(px[1] / 8);
                px[2] = 128u8.wrapping_add(px[2] / 8);
                px[3] = if round == 5 { px[3] } else { 255 };
            }
        }
        // The crate's own encoder uses the encoder's normal profile, which
        // stays in modes 4 to 7. The slow profile also chooses the opaque
        // multi-subset modes 0 to 3, so alternate to cover both tables.
        let bytes = if round < 3 {
            encode_rgba8(w, h, &rgba, 1, dxgi::BC7_UNORM).unwrap()
        } else {
            let image = image_dds::image::RgbaImage::from_raw(w, h, rgba.clone()).unwrap();
            let dds = image_dds::dds_from_image(&image, image_dds::ImageFormat::BC7Unorm, image_dds::Quality::Slow, image_dds::Mipmaps::Disabled).unwrap();
            let mut out = Vec::new();
            dds.write(&mut std::io::Cursor::new(&mut out)).unwrap();
            out
        };
        let dds = Dds::parse(&bytes).unwrap();
        assert_eq!(dds.format(), Format::Bc7);
        for block in dds.level_bytes(0, 0).unwrap().chunks(16) {
            modes_seen.insert(block[0].trailing_zeros().min(8));
        }
        let ours = dds.decode(0, 0).unwrap();
        let reference = image_dds::ddsfile::Dds::read(&mut std::io::Cursor::new(&bytes)).unwrap();
        let theirs = image_dds::image_from_dds(&reference, 0).unwrap();
        assert_eq!(ours.rgba, theirs.as_raw().as_slice(), "round {round}");
    }
    eprintln!("BC7 modes exercised: {modes_seen:?}");
    assert!(modes_seen.len() >= 6, "the encoders only used modes {modes_seen:?}");
}

/// Random blocks with the mode bits forced, so every mode including the
/// three-subset modes 0 and 2 that encoders rarely pick is compared with the
/// reference decoder on thousands of random partitions, endpoints and
/// indices.
#[test]
fn every_bc7_mode_decodes_like_the_reference_on_random_blocks() {
    use soe_dds::header::{caps, flags, pf, Dx10, Header, PixelFormat};
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let (w, h) = (64u32, 64u32);
    for mode in 0..8u32 {
        let mut data = noise(&mut seed, (w * h) as usize);
        for block in data.chunks_mut(16) {
            let below: u8 = ((1u16 << (mode + 1)) - 1) as u8;
            block[0] = (block[0] & !below) | (1 << mode);
        }
        let header = Header {
            flags: flags::CAPS | flags::HEIGHT | flags::WIDTH | flags::PIXELFORMAT,
            height: h,
            width: w,
            pitch_or_linear_size: 0,
            depth: 0,
            mip_map_count: 0,
            pixel_format: PixelFormat { flags: pf::FOURCC, four_cc: *b"DX10", rgb_bit_count: 0, r_mask: 0, g_mask: 0, b_mask: 0, a_mask: 0 },
            caps: caps::TEXTURE,
            caps2: 0,
            dx10: Some(Dx10 { dxgi_format: dxgi::BC7_UNORM, dimension: 3, misc_flag: 0, array_size: 1, misc_flags2: 0 }),
        };
        let mut bytes = header.to_bytes();
        bytes.extend_from_slice(&data);
        let ours = Dds::parse(&bytes).unwrap().decode(0, 0).unwrap();
        let reference = image_dds::ddsfile::Dds::read(&mut std::io::Cursor::new(&bytes)).unwrap();
        let theirs = image_dds::image_from_dds(&reference, 0).unwrap();
        let mismatches = ours.rgba.chunks(64).zip(theirs.as_raw().chunks(64)).filter(|(a, b)| a != b).count();
        assert_eq!(mismatches, 0, "mode {mode}: {mismatches} differing 16-pixel runs of {}", w * h / 16);
    }
}
