// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! BC6H decoding against image_dds and the DXGI format identity.

use soe_dds::header::{caps, flags, pf, Dx10, Header, PixelFormat};
use soe_dds::{dxgi, Dds, Format};

fn bc6h_dds(width: u32, height: u32, format: u32, blocks: &[u8]) -> Vec<u8> {
    let header = Header {
        flags: flags::CAPS | flags::HEIGHT | flags::WIDTH | flags::PIXELFORMAT,
        height,
        width,
        pitch_or_linear_size: 0,
        depth: 0,
        mip_map_count: 0,
        pixel_format: PixelFormat {
            flags: pf::FOURCC,
            four_cc: *b"DX10",
            rgb_bit_count: 0,
            r_mask: 0,
            g_mask: 0,
            b_mask: 0,
            a_mask: 0,
        },
        caps: caps::TEXTURE,
        caps2: 0,
        dx10: Some(Dx10 {
            dxgi_format: format,
            dimension: 3,
            misc_flag: 0,
            array_size: 1,
            misc_flags2: 0,
        }),
    };
    let mut bytes = header.to_bytes();
    bytes.extend_from_slice(blocks);
    bytes
}

fn noise(seed: &mut u64, length: usize) -> Vec<u8> {
    (0..length)
        .map(|_| {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 7;
            *seed ^= *seed << 17;
            (*seed >> 24) as u8
        })
        .collect()
}

#[test]
fn fmt_05_bc6h_unsigned_modes_match_reference_pixels() {
    let mode_prefixes = [0, 1, 2, 6, 10, 14, 18, 22, 26, 30, 3, 7, 11, 15];
    let mut seed = 0xA076_1D64_78BD_642Fu64;
    for prefix in mode_prefixes {
        let mut block = noise(&mut seed, 16);
        block[0] = (block[0] & !0x1f) | prefix;
        let bytes = bc6h_dds(4, 4, dxgi::BC6H_UF16, &block);
        let dds = Dds::parse(&bytes).unwrap();
        assert_eq!(dds.format(), Format::Bc6h { signed: false });
        let actual = dds.decode(0, 0).unwrap();
        let reference = image_dds::ddsfile::Dds::read(&mut std::io::Cursor::new(&bytes)).unwrap();
        let expected = image_dds::image_from_dds(&reference, 0).unwrap();
        assert_eq!(
            actual.rgba.as_slice(),
            expected.as_raw(),
            "mode prefix {prefix}"
        );
        assert!(actual
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255));
    }
}

#[test]
fn fmt_05_bc6h_signed_dxgi_format_decodes() {
    let mut block = noise(&mut 0xE703_7ED1_A0B4_28DBu64, 16);
    block[0] = (block[0] & !0x1f) | 3;
    let signed_bytes = bc6h_dds(4, 4, dxgi::BC6H_SF16, &block);
    let unsigned_bytes = bc6h_dds(4, 4, dxgi::BC6H_UF16, &block);
    let signed = Dds::parse(&signed_bytes).unwrap();
    let unsigned = Dds::parse(&unsigned_bytes).unwrap();
    assert_eq!(signed.format(), Format::Bc6h { signed: true });
    assert!(signed.decode(0, 0).is_ok());
    assert_ne!(
        signed.decode(0, 0).unwrap().rgba,
        unsigned.decode(0, 0).unwrap().rgba
    );
}

#[test]
fn fmt_05_bc6h_partial_edge_blocks_decode_to_expected_pixels() {
    let blocks = [0u8; 32];
    let bytes = bc6h_dds(5, 3, dxgi::BC6H_UF16, &blocks);
    let dds = Dds::parse(&bytes).unwrap();
    let actual = dds.decode(0, 0).unwrap();
    let reference = image_dds::ddsfile::Dds::read(&mut std::io::Cursor::new(&bytes)).unwrap();
    let expected = image_dds::image_from_dds(&reference, 0).unwrap();
    assert_eq!(actual.width, 5);
    assert_eq!(actual.height, 3);
    assert_eq!(actual.rgba.as_slice(), expected.as_raw());
}
