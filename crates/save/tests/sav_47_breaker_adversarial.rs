// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial PACK and NAVM change-form checks against xEdit's save layout.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use soe_save::quest::FormFlags;
use soe_save::{ChangeForm, RefId, Save};

const SAVE_CORPUS: &str =
    "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves";
const FORM_FLAG_BYTES: [u8; 6] = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];

fn change(kind: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 0x01, 0x23]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn payload(flags: u32) -> Vec<u8> {
    if flags & 1 == 0 {
        Vec::new()
    } else {
        FORM_FLAG_BYTES.to_vec()
    }
}

#[test]
fn named_flag_combinations_use_only_the_authored_form_flag_payload() {
    for (kind, masks) in [(32_u8, [1, 1 << 30, 1 << 31]), (33, [1, 2, 4])] {
        for subset in 0..8 {
            let flags = masks.iter().enumerate().fold(0, |bits, (index, mask)| {
                bits | if subset & (1 << index) == 0 { 0 } else { *mask }
            });
            let bytes = payload(flags);
            let original = change(kind, flags, bytes.clone());
            if kind == 32 {
                let decoded = original.pack_change().unwrap().unwrap();
                assert_eq!(decoded.form_flags.is_some(), flags & 1 != 0);
                assert_eq!(decoded.waiting, flags & (1 << 30) != 0);
                assert_eq!(decoded.never_run, flags & (1 << 31) != 0);
                assert_eq!(decoded.change_flags(), flags);
                assert_eq!(decoded.data_bytes(), bytes);
            } else {
                let decoded = original.navmesh_change().unwrap().unwrap();
                assert_eq!(decoded.form_flags.is_some(), flags & 1 != 0);
                assert_eq!(decoded.value_changed, flags & 2 != 0);
                assert_eq!(decoded.full_name_changed, flags & 4 != 0);
                assert_eq!(decoded.change_flags(), flags);
                assert_eq!(decoded.data_bytes(), bytes);
            }
        }
    }
}

#[test]
fn uninterpreted_change_bits_survive_with_all_named_markers() {
    let cases = [
        (
            32_u8,
            0xFFFF_FFFF,
            FormFlags {
                flags: 0xB4A3_9281,
                unknown: 0xD6C5,
            },
        ),
        (
            33,
            0xFFFF_FFFF,
            FormFlags {
                flags: 0xB4A3_9281,
                unknown: 0xD6C5,
            },
        ),
    ];
    for (kind, bits, expected_form_flags) in cases {
        let original = change(kind, bits, FORM_FLAG_BYTES.to_vec());
        if kind == 32 {
            let decoded = original.pack_change().unwrap().unwrap();
            assert_eq!(decoded.form_flags, Some(expected_form_flags));
            assert!(decoded.waiting);
            assert!(decoded.never_run);
            assert_eq!(decoded.unnamed_flags, bits & 0x3FFF_FFFE);
            assert_eq!(decoded.change_flags(), bits);
            assert_eq!(decoded.data_bytes(), original.data);
        } else {
            let decoded = original.navmesh_change().unwrap().unwrap();
            assert_eq!(decoded.form_flags, Some(expected_form_flags));
            assert!(decoded.value_changed);
            assert!(decoded.full_name_changed);
            assert_eq!(decoded.unnamed_flags, bits & 0xFFFF_FFF8);
            assert_eq!(decoded.change_flags(), bits);
            assert_eq!(decoded.data_bytes(), original.data);
        }
    }
}

#[test]
fn incomplete_or_extra_payload_bytes_are_rejected_for_each_marker_shape() {
    for (kind, flags) in [(32_u8, 0xC000_0001), (33, 7)] {
        for length in 0..FORM_FLAG_BYTES.len() {
            let invalid = change(kind, flags, FORM_FLAG_BYTES[..length].to_vec());
            let result = if kind == 32 {
                invalid.pack_change().map(|_| ())
            } else {
                invalid.navmesh_change().map(|_| ())
            };
            assert!(result.is_err(), "kind {kind}, prefix length {length}");
        }
        let mut extra = FORM_FLAG_BYTES.to_vec();
        extra.push(0xEE);
        let invalid = change(kind, flags, extra);
        let result = if kind == 32 {
            invalid.pack_change().map(|_| ())
        } else {
            invalid.navmesh_change().map(|_| ())
        };
        assert!(result.is_err(), "kind {kind}, extra payload byte");
    }
}

#[test]
fn repeated_decodes_do_not_consume_marker_or_payload_state() {
    for (kind, flags) in [(32_u8, 0xC000_0001), (33, 7)] {
        let original = change(kind, flags, FORM_FLAG_BYTES.to_vec());
        let first = if kind == 32 {
            let decoded = original.pack_change().unwrap().unwrap();
            (decoded.change_flags(), decoded.data_bytes())
        } else {
            let decoded = original.navmesh_change().unwrap().unwrap();
            (decoded.change_flags(), decoded.data_bytes())
        };
        let second = if kind == 32 {
            let decoded = original.pack_change().unwrap().unwrap();
            (decoded.change_flags(), decoded.data_bytes())
        } else {
            let decoded = original.navmesh_change().unwrap().unwrap();
            (decoded.change_flags(), decoded.data_bytes())
        };
        assert_eq!(first, second);
        assert_eq!(first, (flags, FORM_FLAG_BYTES.to_vec()));
    }
}

fn collect_saves(path: &Path, output: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_saves(&path, output);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ess"))
        {
            output.push(path);
        }
    }
}

#[test]
fn external_corpus_accounts_for_every_pack_and_navm_payload() {
    let root: PathBuf = std::env::var_os("SOE_TEST_SAVES")
        .map_or_else(|| PathBuf::from(SAVE_CORPUS), PathBuf::from);
    assert!(root.is_dir(), "save corpus missing: {}", root.display());
    let mut paths = Vec::new();
    collect_saves(&root, &mut paths);
    paths.sort();
    assert!(
        paths.len() >= 50,
        "expected at least 50 saves, got {}",
        paths.len()
    );
    let save_count = paths.len();

    let mut counts = BTreeMap::from([("PACK", 0_usize), ("NAVM", 0_usize)]);
    for path in paths {
        let save = Save::open(&path).unwrap();
        for form in &save.change_forms {
            let Some(count) = counts.get_mut(form.record_type()) else {
                continue;
            };
            let raw = form.payload().unwrap();
            let (flags, encoded) = if form.record_type() == "PACK" {
                let decoded = form.pack_change().unwrap().unwrap();
                (decoded.change_flags(), decoded.data_bytes())
            } else {
                let decoded = form.navmesh_change().unwrap().unwrap();
                (decoded.change_flags(), decoded.data_bytes())
            };
            assert_eq!(flags, form.change_flags, "{}", path.display());
            assert_eq!(encoded, raw.as_ref(), "{}", path.display());
            *count += 1;
        }
    }
    eprintln!("accounted for {save_count} saves, PACK/NAVM forms: {counts:?}");
}
