// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{ChangeForm, RefId};
use std::path::{Path, PathBuf};

const CORPUS: &str = "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves";

fn form(kind: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 2]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_31_breaker_combined_named_and_unnamed_flags_roundtrip() {
    for (kind, record_type, speaker) in [(10, "ACTI", false), (11, "TACT", true), (15, "DOOR", false)] {
        let flags = 0xA520_0007 | if speaker { 1 << 23 } else { 0 };
        let bytes = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];
        let change = form(kind, flags, bytes.to_vec());
        let decoded = change.activator_change().unwrap().unwrap();
        assert_eq!(
            decoded.form_flags,
            Some(soe_save::FormFlags {
                flags: 0xB4A3_9281,
                unknown: 0xD6C5,
            })
        );
        assert!(decoded.value_changed && decoded.full_name_changed);
        assert_eq!(decoded.speaker_changed, speaker);
        assert_eq!(decoded.change_flags(record_type).unwrap(), flags);
        assert_eq!(decoded.data_bytes(), bytes);
    }
}

#[test]
fn sav_31_breaker_rejects_each_partial_form_flags_prefix() {
    for kind in [10, 11, 15] {
        for len in 0..6 {
            let change = form(kind, 1, vec![0xA5; len]);
            assert!(matches!(
                change.activator_change(),
                Err(soe_save::Error::Truncated { .. })
            ), "kind={kind} len={len}");
        }
    }
}

#[test]
fn sav_31_breaker_zero_byte_markers_reject_unexplained_payload() {
    for (kind, flags) in [(10, 0b110), (11, 0x0080_0006), (15, 0b110)] {
        assert!(form(kind, flags, vec![0xAA]).activator_change().is_err());
    }
}

fn collect(path: &Path, output: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, output);
        } else if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("ess")) {
            output.push(path);
        }
    }
}

#[test]
fn sav_31_breaker_external_corpus_accounts_all_matching_forms() {
    let mut paths = Vec::new();
    collect(Path::new(CORPUS), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50);
    let mut counts = [0usize; 3];
    for path in paths {
        let save = soe_save::Save::open(&path).unwrap();
        for change in &save.change_forms {
            let Some(index) = ["ACTI", "TACT", "DOOR"]
                .iter()
                .position(|name| change.record_type() == *name)
            else {
                continue;
            };
            let decoded = change.activator_change().unwrap().unwrap();
            assert_eq!(decoded.change_flags(change.record_type()).unwrap(), change.change_flags);
            assert_eq!(decoded.data_bytes(), change.payload().unwrap().as_ref());
            counts[index] += 1;
        }
    }
    eprintln!("SAV-31 corpus ACTI/TACT/DOOR counts={counts:?}");
}
