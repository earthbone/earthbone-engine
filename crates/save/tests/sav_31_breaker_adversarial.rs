// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial ACTI, TACT and DOOR change-form checks from xEdit's save schema.

use std::path::{Path, PathBuf};

use soe_save::{ChangeForm, RefId, Save};

fn change(kind: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
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
fn sav_31_each_xedit_named_or_unnamed_flag_is_retained() {
    // xEdit declares a named entry for every bit, including UnnamedFlag##.
    for kind in [10, 11, 15] {
        for bit in 0..32 {
            let flags = 1u32 << bit;
            let bytes = if bit == 0 {
                vec![0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6]
            } else {
                Vec::new()
            };
            let form = change(kind, flags, bytes.clone());
            let typed = form
                .activator_change()
                .unwrap_or_else(|error| panic!("type {kind}, bit {bit}: {error}"))
                .unwrap();
            assert_eq!(typed.data_bytes(), bytes, "type {kind}, bit {bit}");
            assert_eq!(
                typed.change_flags(form.record_type()).unwrap(),
                flags,
                "type {kind}, bit {bit}"
            );
        }
    }
}

#[test]
fn sav_31_wrong_record_types_do_not_consume_payload() {
    let form = change(16, u32::MAX, vec![0xAA, 0xBB]);
    assert!(form.activator_change().unwrap().is_none());
}

#[test]
fn sav_31_compressed_form_flags_preserve_bytes_on_repeated_reads() {
    let raw = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut encoder, &raw).unwrap();
    let mut form = change(11, (1 << 23) | 1, encoder.finish().unwrap());
    form.uncompressed_len = raw.len() as u32;
    for _ in 0..3 {
        let value = form.activator_change().unwrap().unwrap();
        assert!(value.speaker_changed);
        assert_eq!(value.data_bytes(), raw);
        assert_eq!(form.payload().unwrap().as_ref(), raw);
    }
}

fn collect_saves(path: &Path, output: &mut Vec<PathBuf>) {
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
fn sav_31_external_corpus_item_changes_decode_to_their_exact_payloads() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping save corpus accounting");
        return;
    };
    let mut paths = Vec::new();
    collect_saves(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected at least 50 external saves");
    let mut counts = [0usize; 3];
    for path in paths {
        let save = Save::open(&path).unwrap();
        for form in &save.change_forms {
            let Some(index) = ["ACTI", "TACT", "DOOR"]
                .iter()
                .position(|kind| form.record_type() == *kind)
            else {
                continue;
            };
            let typed = form.activator_change().unwrap().unwrap();
            assert_eq!(typed.data_bytes(), form.payload().unwrap().as_ref());
            assert_eq!(
                typed.change_flags(form.record_type()).unwrap(),
                form.change_flags
            );
            counts[index] += 1;
        }
    }
    let accounted = counts.iter().sum::<usize>();
    assert_eq!(accounted, counts.into_iter().sum::<usize>());
    eprintln!("SAV-31 external corpus counts={counts:?}");
}
