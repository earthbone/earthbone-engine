// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use soe_save::{ChangeForm, RefId, Save};

#[test]
fn sav_47_compressed_lengths_and_repeat_reads_preserve_fields() {
    let bytes = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];
    for (kind, flags) in [(32, 0xC000_0001), (33, 7)] {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&bytes).unwrap();
        let mut change = ChangeForm {
            form: RefId::from_bytes([0x40, 1, 2]),
            change_flags: flags,
            type_field: kind,
            version: 78,
            uncompressed_len: bytes.len() as u32,
            data: encoder.finish().unwrap(),
        };

        for _ in 0..2 {
            let decoded = if kind == 32 {
                let decoded = change.pack_change().unwrap().unwrap();
                (decoded.change_flags(), decoded.data_bytes())
            } else {
                let decoded = change.navmesh_change().unwrap().unwrap();
                (decoded.change_flags(), decoded.data_bytes())
            };
            assert_eq!(decoded, (flags, bytes.to_vec()));
        }

        change.uncompressed_len += 1;
        let invalid = if kind == 32 {
            change.pack_change().map(|_| ())
        } else {
            change.navmesh_change().map(|_| ())
        };
        assert!(invalid.is_err(), "type {kind} accepted a wrong zlib size");
    }
}

fn saves(path: &Path, output: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            saves(&path, output);
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ess"))
        {
            output.push(path);
        }
    }
}

#[test]
fn sav_47_external_corpus_accounts_for_every_pack_and_navm_payload() {
    let root = std::env::var_os("SOE_TEST_SAVES").map_or_else(
        || {
            PathBuf::from(
                "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves",
            )
        },
        PathBuf::from,
    );
    let mut paths = Vec::new();
    saves(&root, &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected at least 50 saves");

    for path in paths {
        let save = Save::open(&path).unwrap();
        for form in &save.change_forms {
            let (flags, bytes) = match form.record_type() {
                "PACK" => {
                    let decoded = form.pack_change().unwrap().unwrap();
                    (decoded.change_flags(), decoded.data_bytes())
                }
                "NAVM" => {
                    let decoded = form.navmesh_change().unwrap().unwrap();
                    (decoded.change_flags(), decoded.data_bytes())
                }
                _ => continue,
            };
            assert_eq!(flags, form.change_flags, "{}", path.display());
            assert_eq!(
                bytes,
                form.payload().unwrap().as_ref(),
                "{}",
                path.display()
            );
        }
    }
}
