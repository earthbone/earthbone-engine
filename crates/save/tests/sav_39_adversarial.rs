// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial SAV-39 checks for compressed bytes and source type indices.

use std::io::Write;

use soe_save::{ChangeForm, RefId};

fn form(kind: u8, flags: u32, data: Vec<u8>, uncompressed_len: u32) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 2]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len,
        data,
    }
}

#[test]
fn sav_39_compressed_payload_decodes_and_repeated_reads_keep_exact_source_fields() {
    for kind in [17, 20, 21] {
        let expected = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&expected).unwrap();
        let change = form(kind, 7, encoder.finish().unwrap(), expected.len() as u32);

        let first = change.light_static_change().unwrap().unwrap();
        let second = change.light_static_change().unwrap().unwrap();
        assert_eq!(first, second);
        assert_eq!(first.change_flags(), 7);
        assert_eq!(first.data_bytes(), expected);
        assert_eq!(change.payload().unwrap().as_ref(), expected);
    }
}

#[test]
fn sav_39_type_indices_match_xedit_and_neighboring_types_stay_opaque() {
    for (kind, expected) in [(17, "LIGH"), (20, "STAT"), (21, "MSTT")] {
        let change = form(kind, 0, vec![], 0);
        assert_eq!(change.record_type(), expected);
        assert!(change.light_static_change().unwrap().is_some());
    }

    for kind in [16, 18, 19, 22] {
        let change = form(kind, u32::MAX, vec![0xAA], 0);
        assert!(change.light_static_change().unwrap().is_none());
    }
}

#[test]
fn sav_39_compressed_truncation_and_corrupt_stream_are_rejected() {
    for kind in [17, 20, 21] {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&[1, 2, 3, 4, 5, 6]).unwrap();
        let mut compressed = encoder.finish().unwrap();
        compressed.pop();
        assert!(form(kind, 1, compressed, 6).light_static_change().is_err());
        assert!(form(kind, 1, vec![0xFF, 0x00], 6)
            .light_static_change()
            .is_err());
    }
}
