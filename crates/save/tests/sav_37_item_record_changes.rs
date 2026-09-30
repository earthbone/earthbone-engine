// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! INGR, ALCH and ENCH save change payloads from xEdit save definitions.

use std::{collections::BTreeMap, path::Path};

use soe_esm::{Plugin, Tag};
use soe_save::{BaseObjectChange, ChangeForm, RefId, Save};

const FORM: [u8; 3] = [0x40, 0x01, 0x23];
const FORM_FLAGS: [u8; 6] = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];

fn change(kind: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes(FORM),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn payload(flags: u32) -> Vec<u8> {
    if flags & 1 != 0 {
        FORM_FLAGS.to_vec()
    } else {
        Vec::new()
    }
}

fn decode(form: &ChangeForm) -> soe_save::Result<(u32, Vec<u8>)> {
    match form.record_type() {
        "INGR" => {
            let typed = form
                .ingredient_change()?
                .ok_or_else(|| soe_save::Error::RecordChange {
                    reason: "missing INGR typed change".into(),
                })?;
            Ok((typed.change_flags(), typed.data_bytes()))
        }
        "ALCH" => {
            let typed = form
                .alchemy_change()?
                .ok_or_else(|| soe_save::Error::RecordChange {
                    reason: "missing ALCH typed change".into(),
                })?;
            Ok((typed.change_flags(), typed.data_bytes()))
        }
        "ENCH" => {
            let typed =
                form.enchantment_change()?
                    .ok_or_else(|| soe_save::Error::RecordChange {
                        reason: "missing ENCH typed change".into(),
                    })?;
            Ok((typed.change_flags(), typed.data_bytes()))
        }
        _ => unreachable!(),
    }
}

fn flags_with_other_bits(kind: u8, marker_flags: u32, include_other: bool) -> u32 {
    let other = if include_other {
        match kind {
            16 => 0x7FFF_FFFE,
            _ => !7,
        }
    } else {
        0
    };
    marker_flags | other
}

#[test]
fn sav_37_all_source_marker_combinations_preserve_typed_fields_and_bytes() {
    for (kind, marker_mask) in [(16u8, 0x8000_0001u32), (26, 7), (48, 7)] {
        let marker_bits: Vec<_> = (0..32)
            .filter(|bit| marker_mask & (1 << bit) != 0)
            .collect();
        for subset in 0..1 << marker_bits.len() {
            let marker_flags = marker_bits
                .iter()
                .enumerate()
                .fold(0, |flags, (index, bit)| {
                    flags
                        | if subset & (1 << index) != 0 {
                            1 << bit
                        } else {
                            0
                        }
                });
            for include_other in [false, true] {
                let flags = flags_with_other_bits(kind, marker_flags, include_other);
                let bytes = payload(flags);
                let form = change(kind, flags, bytes.clone());
                assert_eq!(decode(&form).unwrap(), (flags, bytes), "{kind} {flags:#x}");
                match kind {
                    16 => {
                        let typed = form.ingredient_change().unwrap().unwrap();
                        assert_eq!(typed.ingredient_use_changed, flags & (1 << 31) != 0);
                        assert_eq!(typed.other_flags, flags & 0x7FFF_FFFE);
                        assert_eq!(typed.form_flags.is_some(), flags & 1 != 0);
                    }
                    _ => {
                        let typed: BaseObjectChange = if kind == 26 {
                            form.alchemy_change().unwrap().unwrap()
                        } else {
                            form.enchantment_change().unwrap().unwrap()
                        };
                        assert_eq!(typed.value_changed, flags & 2 != 0);
                        assert_eq!(typed.fullname_changed, flags & 4 != 0);
                        assert_eq!(typed.other_flags, flags & !7);
                        assert_eq!(typed.form_flags.is_some(), flags & 1 != 0);
                    }
                }
            }
        }
    }
}

#[test]
fn sav_37_truncation_unknown_bits_and_trailing_bytes_are_handled() {
    for (kind, flags) in [
        (16, 1 | (1 << 31) | 0x7FFF_FFFE),
        (26, 7 | !7),
        (48, 7 | !7),
    ] {
        let bytes = payload(flags);
        for cut in 0..bytes.len() {
            assert!(decode(&change(kind, flags, bytes[..cut].to_vec())).is_err());
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(decode(&change(kind, flags, trailing)).is_err());
    }
}

#[test]
fn sav_37_other_record_types_remain_opaque() {
    let other = change(8, u32::MAX, vec![0xA5]);
    assert!(other.ingredient_change().unwrap().is_none());
    assert!(other.alchemy_change().unwrap().is_none());
    assert!(other.enchantment_change().unwrap().is_none());
}

#[test]
fn sav_37_installed_skyrim_records_use_expected_record_types() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping vanilla item record check");
        return;
    };
    let plugin =
        Plugin::parse(&std::fs::read(Path::new(&data).join("Skyrim.esm")).unwrap()).unwrap();
    let counts = [Tag(*b"INGR"), Tag(*b"ALCH"), Tag(*b"ENCH")].map(|tag| {
        plugin
            .records
            .iter()
            .filter(|record| record.tag == tag)
            .count()
    });
    assert!(counts.iter().all(|count| *count > 0), "{counts:?}");
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
fn sav_37_external_corpus_accounts_for_every_item_change() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping INGR/ALCH/ENCH corpus survey");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(
        paths.len() >= 50,
        "coverage survey requires at least 50 saves"
    );

    let mut counts = BTreeMap::from([("INGR", 0usize), ("ALCH", 0), ("ENCH", 0)]);
    let mut variants = BTreeMap::new();
    let mut payload_bytes = 0usize;
    let mut decoded_forms = 0usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        for form in &save.change_forms {
            let record_type = form.record_type();
            let Some(count) = counts.get_mut(record_type) else {
                continue;
            };
            *count += 1;
            let bytes = form.payload().unwrap().into_owned();
            payload_bytes += bytes.len();
            assert_eq!(decode(form).unwrap(), (form.change_flags, bytes));
            decoded_forms += 1;
            *variants
                .entry((
                    save.form_version,
                    form.version,
                    record_type,
                    form.change_flags,
                ))
                .or_insert(0usize) += 1;
        }
    }
    assert_eq!(decoded_forms, counts.values().sum());
    eprintln!(
        "SAV-37 coverage: {} saves, {decoded_forms} changes, {payload_bytes} payload bytes, types {counts:?}",
        paths.len()
    );
    for ((save_version, change_version, record_type, flags), count) in variants {
        eprintln!("save_version={save_version} change_version={change_version} type={record_type} flags={flags:#010x} count={count}");
    }
}
