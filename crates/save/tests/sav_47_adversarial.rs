// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial SAV-47 checks for compressed corruption and exact type dispatch.

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
fn sav_47_compressed_truncation_and_corrupt_streams_are_rejected() {
    for (kind, flags) in [(32, 0xC000_0001), (33, 7)] {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(&[0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6])
            .unwrap();
        let mut compressed = encoder.finish().unwrap();
        compressed.pop();
        let change = form(kind, flags, compressed, 6);
        let result = if kind == 32 {
            change.pack_change().map(|_| ())
        } else {
            change.navmesh_change().map(|_| ())
        };
        assert!(result.is_err(), "kind={kind}");

        let corrupt = form(kind, flags, vec![0xFF, 0x00], 6);
        let result = if kind == 32 {
            corrupt.pack_change().map(|_| ())
        } else {
            corrupt.navmesh_change().map(|_| ())
        };
        assert!(result.is_err(), "kind={kind}");
    }
}

#[test]
fn sav_47_source_indices_dispatch_only_pack_and_navm() {
    let pack = form(32, 0xC000_0001, vec![0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6], 0);
    assert_eq!(pack.record_type(), "PACK");
    assert!(pack.pack_change().unwrap().is_some());
    assert!(pack.navmesh_change().unwrap().is_none());

    let navm = form(33, 7, vec![0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6], 0);
    assert_eq!(navm.record_type(), "NAVM");
    assert!(navm.navmesh_change().unwrap().is_some());
    assert!(navm.pack_change().unwrap().is_none());

    for kind in [31, 34] {
        let other = form(kind, u32::MAX, vec![0xAA], 0);
        assert!(other.pack_change().unwrap().is_none());
        assert!(other.navmesh_change().unwrap().is_none());
    }
}

#[test]
fn sav_47_repeated_decode_does_not_mutate_source_or_typed_output() {
    for (kind, flags) in [(32, 0xC000_0001), (33, 7)] {
        let bytes = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];
        let change = form(kind, flags, bytes.to_vec(), 0);
        if kind == 32 {
            let first = change.pack_change().unwrap().unwrap();
            let second = change.pack_change().unwrap().unwrap();
            assert_eq!(first, second);
            assert_eq!(first.change_flags(), flags);
            assert_eq!(first.data_bytes(), bytes);
        } else {
            let first = change.navmesh_change().unwrap().unwrap();
            let second = change.navmesh_change().unwrap().unwrap();
            assert_eq!(first, second);
            assert_eq!(first.change_flags(), flags);
            assert_eq!(first.data_bytes(), bytes);
        }
        assert_eq!(change.payload().unwrap().as_ref(), bytes);
    }
}
