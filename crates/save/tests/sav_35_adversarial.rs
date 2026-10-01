// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{book_note_key_flags as flags, ChangeForm, RefId};
use std::path::{Path, PathBuf};

const CORPUS: &str =
    "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves";

fn form(kind: u8, change_flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 2]),
        change_flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_35_unnamed_xedit_flag_bits_survive_typed_roundtrip() {
    for (kind, known) in [
        (13, flags::BOOK_KNOWN),
        (25, flags::KEY_KNOWN),
        (28, flags::NOTE_KNOWN),
    ] {
        for bit in 0..32 {
            let mask = 1_u32 << bit;
            if known & mask != 0 {
                continue;
            }
            let change = form(kind, mask, vec![]);
            let (rebuilt, unnamed) = match kind {
                13 => {
                    let typed = change.book_change().unwrap().unwrap();
                    (typed.change_flags(), typed.unnamed_flags)
                }
                25 => {
                    let typed = change.key_change().unwrap().unwrap();
                    (typed.change_flags(), typed.unnamed_flags)
                }
                _ => {
                    let typed = change.note_change().unwrap().unwrap();
                    (typed.change_flags(), typed.unnamed_flags)
                }
            };
            assert_eq!(rebuilt, mask, "kind={kind} bit={bit}");
            assert_eq!(unnamed, mask, "kind={kind} bit={bit}");
        }
    }
}

#[test]
fn sav_35_reserved_flags_after_form_prefix_keep_exact_bytes() {
    let data = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6, 0xAA, 0x00];
    let change = form(13, flags::FORM | flags::BOOK_READ, data.to_vec());
    let decoded = change.book_change().unwrap().unwrap();
    assert_eq!(decoded.data_bytes(), data);
}

fn collect(path: &Path, output: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, output);
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ess"))
        {
            output.push(path);
        }
    }
}

#[test]
fn sav_35_external_corpus_accounts_all_matching_forms() {
    let mut paths = Vec::new();
    collect(Path::new(CORPUS), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50);
    let mut counts = [0usize; 3];
    for path in paths {
        let save = soe_save::Save::open(&path).unwrap();
        for change in &save.change_forms {
            let index = match change.record_type() {
                "BOOK" => Some(0),
                "NOTE" => Some(1),
                "KEYM" => Some(2),
                _ => None,
            };
            let Some(index) = index else { continue };
            let payload = change.payload().unwrap();
            let (change_flags, data_bytes) = match change.record_type() {
                "BOOK" => {
                    let decoded = change.book_change().unwrap().unwrap();
                    (decoded.change_flags(), decoded.data_bytes())
                }
                "NOTE" => {
                    let decoded = change.note_change().unwrap().unwrap();
                    (decoded.change_flags(), decoded.data_bytes())
                }
                _ => {
                    let decoded = change.key_change().unwrap().unwrap();
                    (decoded.change_flags(), decoded.data_bytes())
                }
            };
            assert_eq!(change_flags, change.change_flags, "{}", path.display());
            assert_eq!(data_bytes, payload.as_ref(), "{}", path.display());
            counts[index] += 1;
        }
    }
    eprintln!("SAV-35 corpus BOOK/NOTE/KEYM counts={counts:?}");
}
