// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! BOOK, NOTE and KEYM saved fields from xEdit's save definitions, checked
//! against the external Special Edition save corpus.

use std::{collections::BTreeMap, path::Path};

use soe_save::{book_note_key_flags as flags, ChangeForm, RefId, Save};

const ID: [u8; 3] = [0x40, 1, 0x23];
const FORM_BYTES: [u8; 6] = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];

fn form(kind: u8, change_flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes(ID),
        change_flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn bytes_for(flags: u32) -> Vec<u8> {
    if flags & flags::FORM == 0 {
        Vec::new()
    } else {
        FORM_BYTES.to_vec()
    }
}

fn decoded(change: &ChangeForm) -> soe_save::Result<(u32, Vec<u8>)> {
    match change.record_type() {
        "BOOK" => {
            let value = change.book_change()?.ok_or(soe_save::Error::RecordChange {
                reason: "BOOK decoder returned no value".into(),
            })?;
            Ok((value.change_flags(), value.data_bytes()))
        }
        "KEYM" => {
            let value = change.key_change()?.ok_or(soe_save::Error::RecordChange {
                reason: "KEYM decoder returned no value".into(),
            })?;
            Ok((value.change_flags(), value.data_bytes()))
        }
        "NOTE" => {
            let value = change.note_change()?.ok_or(soe_save::Error::RecordChange {
                reason: "NOTE decoder returned no value".into(),
            })?;
            Ok((value.change_flags(), value.data_bytes()))
        }
        _ => Err(soe_save::Error::RecordChange {
            reason: "unexpected record type".into(),
        }),
    }
}

#[test]
fn sav_35_all_source_flag_combinations_retain_typed_fields_and_exact_bytes() {
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
            let data = bytes_for(change_flags);
            let change = form(kind, change_flags, data.clone());
            assert_eq!(
                decoded(&change).unwrap(),
                (change_flags, data),
                "{} flags {change_flags:#010x}",
                change.record_type()
            );
        }
    }
}

#[test]
fn sav_35_typed_flags_expose_each_named_marker() {
    let book_flags =
        flags::FORM | flags::VALUE | flags::FULL_NAME | flags::BOOK_TEACHES | flags::BOOK_READ;
    let book = form(13, book_flags, FORM_BYTES.to_vec())
        .book_change()
        .unwrap()
        .unwrap();
    assert_eq!(book.form_flags.unwrap().flags, 0xB4A3_9281);
    assert_eq!(book.form_flags.unwrap().unknown, 0xD6C5);
    assert!(book.value_changed && book.full_name_changed && book.teaches_skill && book.read);

    let key = form(25, flags::VALUE | flags::FULL_NAME, vec![])
        .key_change()
        .unwrap()
        .unwrap();
    assert!(key.value_changed && key.full_name_changed);

    let note = form(28, flags::NOTE_READ, vec![])
        .note_change()
        .unwrap()
        .unwrap();
    assert!(note.read);
}

#[test]
fn sav_35_rejects_truncated_form_flags_and_retains_unnamed_bits() {
    for (kind, change_flags) in [
        (13, flags::FORM | flags::BOOK_TEACHES | flags::BOOK_READ),
        (25, flags::FORM | flags::VALUE | flags::FULL_NAME),
        (28, flags::FORM | flags::NOTE_READ),
    ] {
        for cut in 0..FORM_BYTES.len() {
            assert!(
                decoded(&form(kind, change_flags, FORM_BYTES[..cut].to_vec())).is_err(),
                "kind {kind}, cut {cut}"
            );
        }
    }

    for (kind, unnamed) in [(13, 1 << 3), (25, 1 << 3), (28, 1 << 1)] {
        let change = form(kind, unnamed, vec![]);
        let (rebuilt_flags, bytes) = decoded(&change).unwrap();
        assert_eq!(rebuilt_flags, unnamed);
        assert!(bytes.is_empty());
    }
}

#[test]
fn sav_35_preserves_bytes_after_source_defined_fields() {
    for (kind, change_flags, suffix) in [
        (13, flags::BOOK_READ, vec![0x08, 0xFE]),
        (25, flags::VALUE | flags::FULL_NAME, vec![0xA7, 0x00]),
        (28, flags::NOTE_READ, vec![0x31, 0x42]),
    ] {
        let change = form(kind, change_flags, suffix.clone());
        let encoded = decoded(&change).unwrap();
        assert_eq!(encoded, (change_flags, suffix), "{}", change.record_type());
    }
}

#[test]
fn sav_35_zero_byte_markers_need_no_payload() {
    let book_flags = flags::VALUE | flags::FULL_NAME | flags::BOOK_TEACHES | flags::BOOK_READ;
    let book = form(13, book_flags, vec![]).book_change().unwrap().unwrap();
    assert!(book.value_changed && book.full_name_changed && book.teaches_skill && book.read);
    assert_eq!(book.data_bytes(), []);

    let key_flags = flags::VALUE | flags::FULL_NAME;
    let key = form(25, key_flags, vec![]).key_change().unwrap().unwrap();
    assert!(key.value_changed && key.full_name_changed);
    assert_eq!(key.data_bytes(), []);

    let note = form(28, flags::NOTE_READ, vec![])
        .note_change()
        .unwrap()
        .unwrap();
    assert!(note.read);
    assert_eq!(note.data_bytes(), []);
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
fn sav_35_external_corpus_accounts_for_every_matching_change() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping BOOK/NOTE/KEYM corpus");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected at least 50 external saves");

    let mut counts = BTreeMap::from([("BOOK", 0usize), ("NOTE", 0), ("KEYM", 0)]);
    let mut flag_counts = BTreeMap::new();
    let mut payload_bytes = 0usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        for change in &save.change_forms {
            let Some(count) = counts.get_mut(change.record_type()) else {
                continue;
            };
            let payload = change.payload().unwrap();
            let (decoded_flags, encoded) = decoded(change).unwrap_or_else(|error| {
                panic!(
                    "{} {} {:06x} flags {:08x} payload {:02x?}: {error}",
                    path.display(),
                    change.record_type(),
                    change.form.raw(),
                    change.change_flags,
                    payload.as_ref(),
                )
            });
            assert_eq!(decoded_flags, change.change_flags);
            assert_eq!(encoded, payload.as_ref());
            *count += 1;
            payload_bytes += encoded.len();
            *flag_counts
                .entry((
                    save.form_version,
                    change.version,
                    change.record_type(),
                    change.change_flags,
                ))
                .or_insert(0usize) += 1;
        }
    }
    eprintln!(
        "SAV-35 {} saves, {payload_bytes} payload bytes, counts={counts:?}, flags={flag_counts:x?}",
        paths.len()
    );
    assert!(
        counts.values().sum::<usize>() > 0,
        "no matching changes found"
    );
}
