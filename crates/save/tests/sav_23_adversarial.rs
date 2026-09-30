// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-23 adversarial checks for shared PFLA, PHZD and PBAR change flags.

use soe_save::{ChangeForm, ProjectileChangeFlag, RefId, Save};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const CORPUS: &str =
    "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves";

fn collect(path: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path.extension().is_some_and(|ext| ext == "ess") {
            paths.push(path);
        }
    }
}

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
fn sav_23_each_source_flag_is_typed_for_each_supported_subtype() {
    for kind in [5, 40, 41] {
        for flag in ProjectileChangeFlag::ALL {
            let bits = 1_u32 << flag as u8;
            let value = form(kind, bits, vec![0, 0x80, 0xff], 0);
            let decoded = value.projectile_change().unwrap().unwrap();
            assert_eq!(decoded.flags.bits(), bits);
            assert!(decoded.flags.contains(flag));
            assert_eq!(decoded.payload, value.data);
        }
    }
}

#[test]
fn sav_23_combined_and_unnamed_bits_remain_exact() {
    for kind in [5, 40, 41] {
        let value = form(kind, u32::MAX, vec![0xfe, 0, 0x7f], 0);
        let decoded = value.projectile_change().unwrap().unwrap();
        assert_eq!(decoded.flags.bits(), u32::MAX);
        for flag in ProjectileChangeFlag::ALL {
            assert!(decoded.flags.contains(flag));
        }
        assert_eq!(decoded.payload, value.data);
    }
}

#[test]
fn sav_23_other_record_types_are_not_misclassified() {
    for kind in [2, 3, 4, 6, 38] {
        assert!(form(kind, u32::MAX, vec![1], 0)
            .projectile_change()
            .unwrap()
            .is_none());
    }
}

#[test]
fn sav_23_compressed_payload_round_trips_and_bad_stream_rejects() {
    let compressed_abc = [
        0x78, 0x9c, 0x4b, 0x4c, 0x4a, 0x06, 0x00, 0x02, 0x4d, 0x01, 0x27,
    ];
    for kind in [5, 40, 41] {
        let decoded = form(kind, 1 << 31, compressed_abc.to_vec(), 3)
            .projectile_change()
            .unwrap()
            .unwrap();
        assert_eq!(decoded.payload, b"abc");
        assert!(form(kind, 0, vec![0x78, 0x9c, 0xff], 3)
            .projectile_change()
            .is_err());
    }
}

#[test]
fn sav_23_generic_writer_keeps_subtype_flags_and_compressed_envelope() {
    for kind in [5, 40, 41] {
        let value = form(kind, 0xa200_000a, vec![0x78, 0x9c, 0x4b], 3);
        let mut encoded = Vec::new();
        value.write(&mut encoded);
        assert_eq!(
            u32::from_le_bytes(encoded[3..7].try_into().unwrap()),
            value.change_flags
        );
        assert_eq!(&encoded[11..], value.data.as_slice());
    }
}

#[test]
fn sav_23_external_corpus_accounts_for_every_matching_change_and_bytes() {
    let root =
        std::env::var_os("SOE_TEST_SAVES").map_or_else(|| PathBuf::from(CORPUS), PathBuf::from);
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    if paths.is_empty() {
        eprintln!("no external saves found, skipping SAV-23 corpus survey");
        return;
    }
    assert!(
        paths.len() >= 50,
        "SAV-23 survey requires the external corpus"
    );
    let mut totals = BTreeMap::from([
        ("PFLA", (0_usize, 0_usize)),
        ("PHZD", (0, 0)),
        ("PBAR", (0, 0)),
    ]);
    let mut change_forms = 0;
    for path in paths {
        let save = Save::open(path).unwrap();
        change_forms += save.change_forms.len();
        for change in &save.change_forms {
            let name = change.record_type();
            let Some(total) = totals.get_mut(name) else {
                continue;
            };
            let decoded = change.projectile_change().unwrap().unwrap();
            let payload = change.payload().unwrap();
            assert_eq!(decoded.payload, payload.as_ref());
            assert_eq!(decoded.flags.bits(), change.change_flags);
            total.0 += 1;
            total.1 += payload.len();
        }
    }
    assert!(
        change_forms > 100_000,
        "survey must account for all change forms"
    );
    assert!(totals["PHZD"].0 > 0, "survey found no PHZD forms");
    eprintln!("SAV-23 external subtype counts and payload bytes: {totals:?}");
}
