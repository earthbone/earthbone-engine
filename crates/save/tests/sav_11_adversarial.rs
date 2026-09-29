// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-11 adversarial checks for the generic PMIS, PGRE and PBEA envelope.

use soe_save::{ChangeForm, RefId, Save};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const CORPUS: &str =
    "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves";

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

#[test]
fn sav_11_projectile_type_ids_do_not_invent_payload_layouts() {
    for (index, name) in [(2, "PMIS"), (3, "PGRE"), (4, "PBEA")] {
        let value = form(index, u32::MAX, vec![0, 0x80, 0xff], 0);
        assert_eq!(value.record_type(), name);
        assert_eq!(value.payload().unwrap().as_ref(), &[0, 0x80, 0xff]);
    }
}

#[test]
fn sav_11_unknown_projectile_flags_and_raw_bytes_survive_outer_write() {
    for (index, name) in [(2, "PMIS"), (3, "PGRE"), (4, "PBEA")] {
        for flags in [0, 1, 0x8000_0000, u32::MAX] {
            let bytes = vec![0x00, 0x81, 0xfe, flags as u8];
            let value = form(index, flags, bytes.clone(), 0);
            let mut encoded = Vec::new();
            value.write(&mut encoded);
            assert!(encoded.ends_with(&bytes), "{name} flags={flags:#010x}");
            assert_eq!(value.payload().unwrap().as_ref(), bytes);
        }
    }
}

#[test]
fn sav_11_compressed_payload_is_inflated_and_malformed_stream_rejected() {
    let compressed_abc = [
        0x78, 0x9c, 0x4b, 0x4c, 0x4a, 0x06, 0x00, 0x02, 0x4d, 0x01, 0x27,
    ];
    for kind in [2, 3, 4] {
        assert_eq!(
            form(kind, 0, compressed_abc.to_vec(), 3)
                .payload()
                .unwrap()
                .as_ref(),
            b"abc"
        );
        assert!(form(kind, 0, vec![0x78, 0x9c, 0xff], 3).payload().is_err());
    }
}

#[test]
fn sav_11_external_corpus_accounts_for_projectile_subtypes_and_payload_bytes() {
    let root =
        std::env::var_os("SOE_TEST_SAVES").map_or_else(|| PathBuf::from(CORPUS), PathBuf::from);
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    if paths.is_empty() {
        eprintln!("no external saves found, skipping SAV-11 corpus survey");
        return;
    }
    assert!(
        paths.len() >= 50,
        "SAV-11 survey requires the external corpus"
    );
    let mut totals = BTreeMap::from([
        ("PMIS", (0_usize, 0_usize)),
        ("PGRE", (0, 0)),
        ("PBEA", (0, 0)),
    ]);
    for path in paths {
        let save = Save::open(&path).unwrap();
        for change in &save.change_forms {
            let name = change.record_type();
            let Some(total) = totals.get_mut(name) else {
                continue;
            };
            let payload = change.payload().unwrap();
            total.0 += 1;
            total.1 += payload.len();
            assert_eq!(payload.as_ref(), change.payload().unwrap().as_ref());
            assert!(change.projectile_change().unwrap().is_none());
        }
    }
    assert!(totals["PMIS"].0 > 0 && totals["PGRE"].0 > 0);
    eprintln!("SAV-11 external subtype counts and payload bytes: {totals:?}");
}
