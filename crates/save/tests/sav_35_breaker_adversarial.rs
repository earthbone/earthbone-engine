// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for source-defined BOOK, NOTE and KEYM change fields.

use std::{collections::BTreeMap, path::Path};

use soe_save::{book_note_key_flags as flags, ChangeForm, RefId, Save};

const FORM_BYTES: [u8; 6] = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];

fn form(kind: u8, change_flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 0x23]),
        change_flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn reencoded(change: &ChangeForm) -> soe_save::Result<(u32, Vec<u8>)> {
    match change.record_type() {
        "BOOK" => change
            .book_change()?
            .map(|value| (value.change_flags(), value.data_bytes()))
            .ok_or_else(|| soe_save::Error::RecordChange {
                reason: "BOOK decoder returned no value".into(),
            }),
        "KEYM" => change
            .key_change()?
            .map(|value| (value.change_flags(), value.data_bytes()))
            .ok_or_else(|| soe_save::Error::RecordChange {
                reason: "KEYM decoder returned no value".into(),
            }),
        "NOTE" => change
            .note_change()?
            .map(|value| (value.change_flags(), value.data_bytes()))
            .ok_or_else(|| soe_save::Error::RecordChange {
                reason: "NOTE decoder returned no value".into(),
            }),
        _ => Err(soe_save::Error::RecordChange {
            reason: "unexpected record type".into(),
        }),
    }
}

#[test]
fn sav_35_breaker_every_source_flag_subset_roundtrips() {
    for (kind, known) in [
        (13u8, flags::BOOK_KNOWN),
        (25, flags::KEY_KNOWN),
        (28, flags::NOTE_KNOWN),
    ] {
        let bits: Vec<u32> = (0..32).filter(|bit| known & (1 << bit) != 0).collect();
        for subset in 0..1 << bits.len() {
            let change_flags = bits.iter().enumerate().fold(0, |value, (index, bit)| {
                value
                    | if subset & (1 << index) != 0 {
                        1 << bit
                    } else {
                        0
                    }
            });
            let payload = if change_flags & flags::FORM != 0 {
                FORM_BYTES.to_vec()
            } else {
                Vec::new()
            };
            let change = form(kind, change_flags, payload.clone());
            assert_eq!(reencoded(&change).unwrap(), (change_flags, payload));
        }
    }
}

#[test]
fn sav_35_breaker_form_flags_precede_opaque_suffix() {
    for (kind, change_flags) in [
        (13, flags::FORM | flags::BOOK_READ),
        (25, flags::FORM | flags::VALUE),
        (28, flags::FORM | flags::NOTE_READ),
    ] {
        let mut payload = FORM_BYTES.to_vec();
        payload.extend([0x08, 0x00, 0xFE]);
        let change = form(kind, change_flags, payload.clone());
        assert_eq!(reencoded(&change).unwrap(), (change_flags, payload));
    }
}

#[test]
fn sav_35_breaker_each_decoder_ignores_other_record_types() {
    for kind in [13, 25, 28] {
        let change = form(kind, 0, vec![]);
        assert_eq!(change.book_change().unwrap().is_some(), kind == 13);
        assert_eq!(change.key_change().unwrap().is_some(), kind == 25);
        assert_eq!(change.note_change().unwrap().is_some(), kind == 28);
    }
}

#[test]
fn sav_35_breaker_repeated_decode_keeps_all_fields_and_bytes() {
    let change = form(
        13,
        flags::FORM | flags::VALUE | flags::FULL_NAME | flags::BOOK_TEACHES | flags::BOOK_READ,
        [FORM_BYTES.as_slice(), &[0xAA, 0xBB]].concat(),
    );
    let first = reencoded(&change).unwrap();
    for _ in 0..4 {
        assert_eq!(reencoded(&change).unwrap(), first);
    }
    assert_eq!(first.0, change.change_flags);
    assert_eq!(first.1, [FORM_BYTES.as_slice(), &[0xAA, 0xBB]].concat());
}

#[test]
fn sav_35_breaker_rejects_corrupt_compressed_payloads() {
    for (kind, marker) in [
        (13, flags::BOOK_READ),
        (25, flags::VALUE),
        (28, flags::NOTE_READ),
    ] {
        let mut change = form(kind, marker, vec![0xFF, 0x00]);
        change.uncompressed_len = 6;
        let result = match kind {
            13 => change
                .book_change()
                .map(|value| value.map(|v| v.data_bytes())),
            25 => change
                .key_change()
                .map(|value| value.map(|v| v.data_bytes())),
            _ => change
                .note_change()
                .map(|value| value.map(|v| v.data_bytes())),
        };
        assert!(
            result.is_err(),
            "type index {kind} accepted invalid zlib data"
        );
    }
}

fn collect(path: &Path, paths: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ess"))
        {
            paths.push(path);
        }
    }
}

#[test]
fn sav_35_breaker_external_corpus_reencodes_every_matching_change() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping BOOK/NOTE/KEYM corpus");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected at least 50 external saves");

    let mut counts = BTreeMap::from([("BOOK", 0usize), ("KEYM", 0), ("NOTE", 0)]);
    for path in paths {
        let save = Save::open(path).unwrap();
        for change in &save.change_forms {
            let Some(count) = counts.get_mut(change.record_type()) else {
                continue;
            };
            let payload = change.payload().unwrap();
            assert_eq!(
                reencoded(change).unwrap(),
                (change.change_flags, payload.into_owned()),
                "{} {} {:06x}",
                change.record_type(),
                change.form.raw(),
                change.change_flags
            );
            *count += 1;
        }
    }
    eprintln!("SAV-35 adversarial corpus counts={counts:?}");
    assert!(
        counts.values().sum::<usize>() > 0,
        "no matching changes found"
    );
}
