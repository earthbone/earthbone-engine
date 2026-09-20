// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Independent list-change fixtures based on FallrimTools and xEdit.

use std::{collections::BTreeMap, io::Write, path::Path};

use soe_save::{ChangeForm, RefId, Save};

const TYPES: [u8; 4] = [43, 44, 45, 46];
const ADDED: u32 = 1 << 31;

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

fn decoded(form: &ChangeForm) -> soe_save::Result<(u32, Vec<u8>)> {
    if form.record_type() == "FLST" {
        let list = form.form_list_change()?.unwrap();
        Ok((list.change_flags(), list.data_bytes()?))
    } else {
        let list = form.leveled_list_change()?.unwrap();
        Ok((list.change_flags(), list.data_bytes()?))
    }
}

fn entries(kind: u8) -> Vec<u8> {
    if kind == 43 {
        vec![3, 0, 0, 0, 0x40, 1, 2, 0x80, 3, 4, 0, 0, 0]
    } else {
        vec![
            2, 0x40, 1, 2, 255, 0x34, 0x12, 200, 0x80, 3, 4, 0, 0xff, 0xff, 0,
        ]
    }
}

#[test]
fn all_list_types_preserve_each_flag_combination_and_reject_every_truncation() {
    for kind in TYPES {
        for flags in [0, 1, ADDED, ADDED | 1] {
            let mut data = Vec::new();
            if flags & 1 != 0 {
                data.extend([1, 2, 3, 4, 5, 6]);
            }
            if flags & ADDED != 0 {
                data.extend(entries(kind));
            }
            let form = change(kind, flags, data.clone());
            assert_eq!(decoded(&form).unwrap(), (flags, data));
            for cut in 0..form.data.len() {
                let mut short = form.clone();
                short.data.truncate(cut);
                assert!(
                    decoded(&short).is_err(),
                    "type={kind} flags={flags:x} cut={cut}"
                );
            }
            let mut trailing = form;
            trailing.data.push(0);
            assert!(decoded(&trailing).is_err());
        }
    }
}

#[test]
fn list_fields_follow_fixed_wire_values_and_keep_form_flags_at_bit_zero() {
    let mut form = change(43, 1 | ADDED, vec![1, 2, 3, 4, 5, 6]);
    form.data.extend(entries(43));
    let list = form.form_list_change().unwrap().unwrap();
    let flags = list.form_flags.unwrap();
    assert_eq!((flags.flags, flags.unknown), (0x04030201, 0x0605));
    let forms = list.forms.unwrap();
    assert_eq!(
        forms.iter().map(|id| id.raw()).collect::<Vec<_>>(),
        [0x400102, 0x800304, 0]
    );
    for kind in [44, 45, 46] {
        let list = change(kind, ADDED, entries(kind))
            .leveled_list_change()
            .unwrap()
            .unwrap();
        let entries = list.entries.unwrap();
        assert_eq!(
            (
                entries[0].form.raw(),
                entries[0].level,
                entries[0].count,
                entries[0].chance
            ),
            (0x400102, 255, 0x1234, 200)
        );
        assert_eq!(
            (entries[1].form.raw(), entries[1].count),
            (0x800304, 0xffff)
        );
    }
}

#[test]
fn empty_list_sections_remain_distinct_from_omitted_sections() {
    for kind in TYPES {
        let empty = if kind == 43 { vec![0; 4] } else { vec![0] };
        assert_eq!(
            decoded(&change(kind, ADDED, empty.clone())).unwrap(),
            (ADDED, empty)
        );
        assert_eq!(decoded(&change(kind, 0, vec![])).unwrap(), (0, vec![]));
    }
}

#[test]
fn unknown_bits_and_impossible_counts_fail_before_allocation() {
    for kind in TYPES {
        for bit in 1..31 {
            assert!(decoded(&change(kind, 1 << bit, vec![0; 32])).is_err());
        }
        assert!(decoded(&change(kind, ADDED, vec![255; 4])).is_err());
    }
    let unrelated = change(0, u32::MAX, vec![255]);
    assert!(unrelated.form_list_change().unwrap().is_none());
    assert!(unrelated.leveled_list_change().unwrap().is_none());
}

#[test]
fn count_widths_preserve_high_bit_values_and_encoder_rejects_overflow() {
    let mut data = vec![255];
    data.extend(vec![0; 255 * 7]);
    let form = change(45, ADDED, data.clone());
    let mut list = form.leveled_list_change().unwrap().unwrap();
    assert_eq!(list.data_bytes().unwrap(), data);
    let entries = list.entries.as_mut().unwrap();
    entries.push(entries[0]);
    assert!(list.data_bytes().is_err());

    let mut data = 16384u32.to_le_bytes().to_vec();
    data.extend(vec![0; 16384 * 3]);
    assert_eq!(decoded(&change(43, ADDED, data.clone())).unwrap().1, data);
}

#[test]
fn compressed_list_payloads_use_the_same_typed_decoder() {
    for kind in TYPES {
        let data = entries(kind);
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&data).unwrap();
        let mut form = change(kind, ADDED, encoder.finish().unwrap());
        form.uncompressed_len = data.len() as u32;
        assert_eq!(decoded(&form).unwrap().1, data);
    }
}

fn collect(path: &Path, paths: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ess"))
        {
            paths.push(path);
        }
    }
}

#[test]
fn sav_51_all_corpus_lists_decode_and_reencode_exactly() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping list corpus");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50);
    let mut counts = BTreeMap::from([("FLST", 0usize), ("LVLN", 0), ("LVLI", 0), ("LVSP", 0)]);
    let mut variants = BTreeMap::new();
    let mut bytes = 0;
    for path in &paths {
        let save = Save::open(path).unwrap();
        for form in &save.change_forms {
            let Some(count) = counts.get_mut(form.record_type()) else {
                continue;
            };
            let (flags, encoded) = decoded(form).unwrap_or_else(|error| {
                panic!(
                    "{} {} {:06x}: {error}",
                    path.display(),
                    form.record_type(),
                    form.form.raw()
                )
            });
            assert_eq!(flags, form.change_flags);
            assert_eq!(encoded, form.payload().unwrap().as_ref());
            *count += 1;
            bytes += encoded.len();
            *variants
                .entry((save.form_version, form.version, form.record_type(), flags))
                .or_insert(0usize) += 1;
        }
    }
    eprintln!(
        "SAV-51 {} saves, {bytes} bytes, counts={counts:?}, versions/types/flags={variants:x?}",
        paths.len()
    );
    assert!(counts.values().sum::<usize>() > 0);
}
