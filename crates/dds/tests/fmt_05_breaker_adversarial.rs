// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_dds::header::{caps, flags, pf, Dx10, Header, PixelFormat};
use soe_dds::{dxgi, Dds, Format};

fn bc6h(width: u32, height: u32, format: u32, blocks: &[u8]) -> Vec<u8> {
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

#[test]
fn fmt_05_bc6h_both_dxgi_variants_produce_opaque_rgba_pixels() {
    let block = [0u8; 16];
    for (format, signed) in [
        (dxgi::BC6H_UF16, false),
        (dxgi::BC6H_SF16, true),
    ] {
        let bytes = bc6h(4, 4, format, &block);
        let dds = Dds::parse(&bytes).unwrap();
        assert_eq!(dds.format(), Format::Bc6h { signed });
        let image = dds.decode(0, 0).unwrap();
        assert_eq!(image.rgba.len(), 4 * 4 * 4);
        assert!(image.rgba.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255));
        assert_eq!(image, dds.decode(0, 0).unwrap());
        let reference = image_dds::ddsfile::Dds::read(&mut std::io::Cursor::new(&bytes)).unwrap();
        let expected = image_dds::image_from_dds(&reference, 0).unwrap();
        assert_eq!(image.rgba.as_slice(), expected.as_raw());
    }
}

#[test]
fn fmt_05_bc6h_rejects_a_missing_block_byte() {
    let bytes = bc6h(4, 4, dxgi::BC6H_UF16, &[0; 15]);
    assert!(matches!(Dds::parse(&bytes), Err(soe_dds::Error::Truncated { .. })));
}
