// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;

use flate2::write::ZlibEncoder;
use flate2::Compression;
use soe_swf_convert::convert;

fn minimal_swf(signature: [u8; 3], compressed: bool) -> Vec<u8> {
    let body = [0x08, 0, 0, 12, 1, 0, 64, 0, 0, 0];
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&signature);
    bytes.push(9);
    bytes.extend_from_slice(&((body.len() as u32) + 8).to_le_bytes());
    if compressed {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&body).unwrap();
        bytes.extend_from_slice(&encoder.finish().unwrap());
    } else {
        bytes.extend_from_slice(&body);
    }
    bytes
}

#[test]
fn ruffle_gfx_and_cfx_headers_convert_like_fws_and_cws() {
    for (signature, compressed) in [
        (*b"FWS", false),
        (*b"GFX", false),
        (*b"CWS", true),
        (*b"CFX", true),
    ] {
        let bytes = minimal_swf(signature, compressed);
        let first = convert(&bytes).unwrap_or_else(|error| panic!("{signature:?}: {error}"));
        let second = convert(&bytes).unwrap_or_else(|error| panic!("{signature:?}: {error}"));
        let root = first.timelines.get(&0).unwrap();
        assert_eq!(root.declared_frames, 1, "{signature:?}");
        assert_eq!(root.frames.len(), 1, "{signature:?}");
        assert_eq!(first.timelines, second.timelines, "{signature:?}");
    }
}
